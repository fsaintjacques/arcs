/**
 * Drives one game in the browser: the engine advances itself through chance
 * nodes and bot seats, and stops whenever a human seat owes a decision.
 *
 * The engine is the Rust one, compiled to wasm (`src/ui/session.ts`). The hook
 * never holds a game state it can mutate — it holds a `Session` handle and a
 * JSON snapshot of the position for rendering. Two things fall out of that:
 *
 *   - a decision is played by **index into the legal list**, so an unoffered
 *     action cannot be played, and
 *   - undo is a `restore()` of a saved position rather than a state clone plus
 *     an RNG stream replayed forward from its seed. Rust's `GameState` is
 *     `Copy` and the session snapshots its RNGs alongside it, so a rewound game
 *     re-draws exactly the dice and deals the player already saw.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { Action, GameState, VariantDef } from '../engine/types';
import { describeAction } from './describe';
import {
  createSession,
  pending,
  readActions,
  readAmbitionCounts,
  readStandings,
  readState,
  readVariant,
  type Session,
} from './session';

export interface GameConfig {
  players: number;
  /** Agent name per seat; `null` means a human plays it. */
  seats: (string | null)[];
  seed: number;
  setupIndex: number;
  /** Milliseconds to pause between bot decisions, so play is watchable. */
  botDelay: number;
}

export interface LogEntry {
  chapter: number;
  player: number | null;
  text: string;
}

export interface Standing {
  player: number;
  power: number;
  rank: number;
}

export interface GameHandle {
  variant: VariantDef;
  state: GameState;
  /** Legal actions when a human is on turn, otherwise empty. */
  actions: Action[];
  /** Seat on turn, or null at a chance node / game over. */
  actor: number | null;
  /** True while a bot or a chance node is being resolved. */
  busy: boolean;
  humanSeats: number[];
  log: LogEntry[];
  /** Final standings, best first — empty until the game is over. */
  standings: Standing[];
  /** Ambition tallies per seat, in `AMBITIONS` order, from the engine. */
  ambitionCounts: number[][];
  play: (a: Action) => void;
  /**
   * Rewind to the previous human decision (or the last one, from game over).
   * Never crosses a reveal: once dice are rolled, a card is dealt or drawn, or
   * a Court slot refills, everything before that moment is out of reach.
   */
  undo: () => void;
  canUndo: boolean;
  reset: (config?: Partial<GameConfig>) => void;
  config: GameConfig;
}

/** One rewind point: the game as it stood when a human was asked to decide. */
interface Rewind {
  /** Handle of the position saved inside the wasm session. */
  snap: number;
  /** Decisions and chance nodes resolved when it was taken. */
  step: number;
  log: LogEntry[];
}

/**
 * The things an undo must never carry a player back across: hidden
 * information that has since been revealed. Rewinding past a reveal would let
 * the player redecide with knowledge they did not have — roll dice and take
 * the battle back, secure a card and unsee what refilled the slot.
 *
 * Detected from state deltas rather than action types, because reveals happen
 * transitively — destroying a city Ransacks the Court, securing a Vox card
 * resolves it mid-action — and an enumeration of action names would go stale.
 * A chance node resolving (dice, deal) is flagged at the call site; the rest
 * is: the Court deck shrank (a hidden card flipped up), a human hand holds a
 * card it did not hold before (raid, mulligan redraw), or a Farseers peek
 * opened a Rival's hand.
 */
export interface InfoMark {
  courtDeck: number;
  peeking: boolean;
  hands: Set<number>[];
}

export function markInfo(s: GameState, humanSeats: number[]): InfoMark {
  return {
    courtDeck: s.courtDeck.length,
    peeking: s.peek !== null,
    hands: humanSeats.map((h) => new Set(s.playerStates[h].hand)),
  };
}

export function revealedSince(s: GameState, humanSeats: number[], before: InfoMark): boolean {
  if (s.courtDeck.length < before.courtDeck) return true;
  if (s.peek !== null && !before.peeking) return true;
  return humanSeats.some((h, i) => s.playerStates[h].hand.some((c) => !before.hands[i].has(c)));
}

/** The seats no agent is playing. */
function humansOf(config: GameConfig): number[] {
  return config.seats.slice(0, config.players).flatMap((name, i) => (name ? [] : [i]));
}

/**
 * The log line for one action. A Copy and the card burned to seize go down
 * face down and are never turned back up, so naming them would hand the reader
 * what the table cannot see. `known` is true when the acting seat's hidden
 * information is the reader's to see.
 */
export function describeForLog(
  a: Action,
  s: GameState,
  v: VariantDef,
  known: boolean,
): string {
  if (!known) {
    if (a.t === 'seize') return 'Seize the initiative, burning a card face down';
    if (a.t === 'follow' && a.mode === 'copy') {
      return 'Copy face down — 1 action of the lead card';
    }
  }
  return describeAction(a, s, v);
}

export function useGame(initial: GameConfig): GameHandle {
  const [config, setConfig] = useState(initial);
  const [, forceRender] = useState(0);
  const bump = useCallback(() => forceRender((n) => n + 1), []);

  const sessionRef = useRef<Session | null>(null);
  const variantRef = useRef<VariantDef | null>(null);
  const stateRef = useRef<GameState | null>(null);
  const actionsRef = useRef<Action[]>([]);
  const logRef = useRef<LogEntry[]>([]);
  const undoStack = useRef<Rewind[]>([]);
  /** True once hidden information has been revealed since the last snapshot. */
  const revealedRef = useRef(false);
  /** Nodes resolved so far — the identity of "this moment", for undo. */
  const stepRef = useRef(0);
  const [busy, setBusy] = useState(false);
  const timer = useRef<number | null>(null);

  /** Re-read the position out of the session for rendering. */
  const sync = useCallback(() => {
    const session = sessionRef.current!;
    stateRef.current = readState(session);
    actionsRef.current = readActions(session);
  }, []);

  /** Replace the session with a fresh game, and clear everything about the old one. */
  const build = useCallback(
    (c: GameConfig) => {
      sessionRef.current?.free();
      const session = createSession(c);
      sessionRef.current = session;
      variantRef.current = readVariant(session);
      logRef.current = [];
      undoStack.current = [];
      revealedRef.current = false;
      stepRef.current = 0;
      sync();
    },
    [sync],
  );

  const record = useCallback((player: number | null, text: string) => {
    const s = stateRef.current!;
    logRef.current = [...logRef.current.slice(-199), { chapter: s.chapter, player, text }];
  }, []);

  /**
   * Remember this moment as a rewind point. Called whenever the engine stops
   * at a human decision (and at game over, so the last action can be taken
   * back too). The guard makes the call idempotent — StrictMode runs effects
   * twice in development, and one moment must not become two rewind points.
   */
  const pushUndo = useCallback(() => {
    const session = sessionRef.current!;
    const top = undoStack.current[undoStack.current.length - 1];
    if (top && top.step === stepRef.current && top.log === logRef.current) return;
    // A reveal happened since the last rewind point: everything before it is
    // out of reach, and the snapshot about to be pushed becomes the new floor.
    if (revealedRef.current) {
      undoStack.current.length = 0;
      session.truncateSnapshots(0);
      revealedRef.current = false;
    }
    undoStack.current.push({
      snap: session.snapshot(),
      step: stepRef.current,
      log: logRef.current,
    });
    if (undoStack.current.length > 100) undoStack.current.shift();
  }, []);

  /**
   * Advance until a human owes a decision or the game ends. Bot turns are
   * spaced out with a timer so the board can be watched.
   */
  const advance = useCallback(() => {
    const session = sessionRef.current!;
    const humans = humansOf(config);

    for (let guard = 0; guard < 500; guard++) {
      const node = pending(session);
      if (node.kind === 'over') {
        pushUndo();
        setBusy(false);
        bump();
        return;
      }
      if (node.kind === 'chance') {
        // Dice and deals are reveals by definition.
        revealedRef.current = true;
        const before = stateRef.current!;
        const rolling = before.phase === 'battleRoll' ? before.battle : null;
        session.resolveChance();
        stepRef.current++;
        sync();
        const rolled = stateRef.current!.battle;
        if (rolling && rolled) {
          // Keep the roll readable after the dice leave the table.
          const gained = rolled.hits - rolling.hits;
          const text =
            rolling.pendingReroll > 0
              ? `rerolls the blanks: +${gained} hit${gained === 1 ? '' : 's'}`
              : `rolls: ${describeRoll(rolled)}`;
          record(rolled.attacker, text);
        }
        continue;
      }
      if (!config.seats[node.player]) {
        pushUndo();
        setBusy(false);
        bump();
        return;
      }
      // A bot owes a decision: schedule it so the UI can paint first.
      setBusy(true);
      bump();
      timer.current = window.setTimeout(() => {
        timer.current = null;
        const index = session.botChoose();
        const action = actionsRef.current[index];
        record(
          node.player,
          describeForLog(
            action,
            stateRef.current!,
            variantRef.current!,
            humans.includes(node.player),
          ),
        );
        const before = markInfo(stateRef.current!, humans);
        session.apply(index);
        stepRef.current++;
        sync();
        if (revealedSince(stateRef.current!, humans, before)) revealedRef.current = true;
        advance();
      }, config.botDelay);
      return;
    }
    setBusy(false);
    bump();
  }, [bump, config, pushUndo, record, sync]);

  // The first render happens before the effect below, so the session exists
  // from the very first paint rather than being null for a frame.
  if (sessionRef.current === null) build(config);

  // Rebuild and run whenever the configuration changes.
  useEffect(() => {
    build(config);
    bump();
    advance();
    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config]);

  const s = stateRef.current!;
  const node = pending(sessionRef.current!);
  const humanSeats = humansOf(config);
  const actor = node.kind === 'decision' ? node.player : null;
  const humanTurn = actor !== null && !config.seats[actor];

  const play = useCallback(
    (a: Action) => {
      const current = sessionRef.current!;
      const index = actionsRef.current.indexOf(a);
      const node = pending(current);
      if (index < 0 || node.kind !== 'decision') return;
      const humans = humansOf(config);
      record(
        node.player,
        describeForLog(a, stateRef.current!, variantRef.current!, humans.includes(node.player)),
      );
      const before = markInfo(stateRef.current!, humans);
      current.apply(index);
      stepRef.current++;
      sync();
      if (revealedSince(stateRef.current!, humans, before)) revealedRef.current = true;
      advance();
    },
    [advance, config, record, sync],
  );

  /**
   * Rewind to the previous rewind point. The stack top is always *this*
   * moment (it was pushed when the engine stopped here), so undo discards it
   * and restores the one before — the human's previous decision, with any bot
   * turns in between unwound along with it. The session's saved position
   * carries its RNGs, so the rewound game re-draws the same dice and deals the
   * player already saw.
   */
  const undo = useCallback(() => {
    if (timer.current !== null || undoStack.current.length < 2) return;
    undoStack.current.pop();
    const rewind = undoStack.current[undoStack.current.length - 1];
    sessionRef.current!.restore(rewind.snap);
    logRef.current = rewind.log;
    stepRef.current = rewind.step;
    revealedRef.current = false;
    sync();
    setBusy(false);
    bump();
  }, [bump, sync]);

  const reset = useCallback((patch?: Partial<GameConfig>) => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    setConfig((c) => ({ ...c, ...patch }));
  }, []);

  // Both are read from the engine rather than recomputed in TypeScript, and
  // both are cheap enough to take whenever the position changes.
  const standings = useMemo(
    () => (s.phase === 'over' ? readStandings(sessionRef.current!) : []),
    [s],
  );
  const ambitionCounts = useMemo(() => readAmbitionCounts(sessionRef.current!), [s]);

  return {
    variant: variantRef.current!,
    state: s,
    actions: humanTurn ? actionsRef.current : [],
    actor,
    busy: busy || (actor !== null && !humanTurn),
    humanSeats,
    log: logRef.current,
    standings,
    ambitionCounts,
    play,
    undo,
    canUndo: undoStack.current.length >= 2 && timer.current === null,
    reset,
    config,
  };
}

/** The roll as words, read at the moment the dice settle (nothing assigned yet). */
function describeRoll(b: NonNullable<GameState['battle']>): string {
  const parts = [
    b.hits > 0 ? `${b.hits} hit${b.hits === 1 ? '' : 's'}` : '',
    b.buildingHits > 0 ? `${b.buildingHits} building hit${b.buildingHits === 1 ? '' : 's'}` : '',
    b.keys > 0 ? `${b.keys} key${b.keys === 1 ? '' : 's'}` : '',
    b.selfHits > 0 ? `${b.selfHits} self-hit${b.selfHits === 1 ? '' : 's'}` : '',
    b.interceptResolved ? 'intercepted' : '',
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(', ') : 'all blanks';
}
