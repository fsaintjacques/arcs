/**
 * Drives one game in the browser: the engine advances itself through chance
 * nodes and bot seats, and stops whenever a human seat owes a decision.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  applyActionMut,
  cloneState,
  getPending,
  makeVariant,
  mulberry32,
  newGame,
  observe,
  resolveChanceMut,
  standings,
  type Action,
  type GameState,
  type RNG,
  type VariantDef,
} from '../engine';
import { makeAgent, type Agent, type AgentCtx } from '../agents';
import { describeAction } from './describe';

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

function buildAgents(config: GameConfig): (Agent | null)[] {
  return config.seats.map((name) => (name ? makeAgent(name) : null));
}

/**
 * A seeded RNG that counts its draws, so a snapshot can be restored by
 * replaying the same stream forward. Undo needs this: state alone is not
 * enough, because a rewound game would otherwise re-draw different dice and
 * deals than the ones the player already saw.
 */
interface Stream {
  n: number;
  fn: RNG;
  /** Rewind to draw `n` by replaying the stream from its seed. */
  restore: (n: number) => void;
}

function makeStream(seed: number): Stream {
  let raw = mulberry32(seed);
  const s: Stream = {
    n: 0,
    fn: () => {
      s.n++;
      return raw();
    },
    restore: (n: number) => {
      raw = mulberry32(seed);
      for (let i = 0; i < n; i++) raw();
      s.n = n;
    },
  };
  return s;
}

/** One rewind point: the game as it stood when a human was asked to decide. */
interface Snapshot {
  state: GameState;
  draws: number;
  ctxDraws: number[];
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

export function useGame(initial: GameConfig): GameHandle {
  const [config, setConfig] = useState(initial);
  const [, forceRender] = useState(0);
  const bump = useCallback(() => forceRender((n) => n + 1), []);

  const variant = useMemo(
    () => makeVariant(config.players, config.setupIndex),
    [config.players, config.setupIndex],
  );

  const gameStream = useRef<Stream>(makeStream(config.seed));
  const ctxStreams = useRef<Stream[]>([]);
  const stateRef = useRef<GameState>(newGame(variant, gameStream.current.fn, config.setupIndex));
  const logRef = useRef<LogEntry[]>([]);
  const agentsRef = useRef<(Agent | null)[]>(buildAgents(config));
  const ctxsRef = useRef<AgentCtx[]>([]);
  const undoStack = useRef<Snapshot[]>([]);
  /** True once hidden information has been revealed since the last snapshot. */
  const revealedRef = useRef(false);
  const [busy, setBusy] = useState(false);
  const timer = useRef<number | null>(null);

  const start = useCallback(
    (c: GameConfig) => {
      const v = makeVariant(c.players, c.setupIndex);
      gameStream.current = makeStream(c.seed);
      stateRef.current = newGame(v, gameStream.current.fn, c.setupIndex);
      logRef.current = [];
      undoStack.current = [];
      revealedRef.current = false;
      agentsRef.current = buildAgents(c);
      ctxStreams.current = c.seats.map((_, player) =>
        makeStream((c.seed ^ (0x9e3779b9 * (player + 1))) >>> 0),
      );
      ctxsRef.current = c.seats.map((_, player) => ({
        variant: v,
        rng: ctxStreams.current[player].fn,
        player,
      }));
      bump();
    },
    [bump],
  );

  // Rebuild whenever the configuration changes.
  useEffect(() => {
    start(config);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config]);

  const record = useCallback((player: number | null, text: string) => {
    const s = stateRef.current;
    logRef.current = [...logRef.current.slice(-199), { chapter: s.chapter, player, text }];
  }, []);

  /**
   * Remember this moment as a rewind point. Called whenever the engine stops
   * at a human decision (and at game over, so the last action can be taken
   * back too). The guard makes the call idempotent — StrictMode runs effects
   * twice in development, and one moment must not become two rewind points.
   */
  const pushUndo = useCallback(() => {
    const top = undoStack.current[undoStack.current.length - 1];
    if (top && top.draws === gameStream.current.n && top.log === logRef.current) return;
    // A reveal happened since the last rewind point: everything before it is
    // out of reach, and the snapshot about to be pushed becomes the new floor.
    if (revealedRef.current) {
      undoStack.current.length = 0;
      revealedRef.current = false;
    }
    undoStack.current.push({
      state: cloneState(stateRef.current),
      draws: gameStream.current.n,
      ctxDraws: ctxStreams.current.map((s) => s.n),
      log: logRef.current,
    });
    if (undoStack.current.length > 100) undoStack.current.shift();
  }, []);

  /**
   * Advance until a human owes a decision or the game ends. Bot turns are
   * spaced out with a timer so the board can be watched.
   */
  const advance = useCallback(() => {
    const v = makeVariant(config.players, config.setupIndex);
    const s = stateRef.current;

    for (let guard = 0; guard < 500; guard++) {
      const node = getPending(s, v);
      if (node.kind === 'over') {
        pushUndo();
        setBusy(false);
        bump();
        return;
      }
      if (node.kind === 'chance') {
        // Dice and deals are reveals by definition.
        revealedRef.current = true;
        const rolling = s.phase === 'battleRoll' ? s.battle : null;
        const wasReroll = (rolling?.pendingReroll ?? 0) > 0;
        const hitsBefore = rolling?.hits ?? 0;
        resolveChanceMut(s, v, gameStream.current.fn);
        if (rolling) {
          // Keep the roll readable after the dice leave the table.
          const text = wasReroll
            ? `rerolls the blanks: +${rolling.hits - hitsBefore} hit${rolling.hits - hitsBefore === 1 ? '' : 's'}`
            : `rolls: ${describeRoll(rolling)}`;
          record(rolling.attacker, text);
        }
        continue;
      }
      const agent = agentsRef.current[node.player];
      if (!agent) {
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
        const ctx = ctxsRef.current[node.player] ?? {
          variant: v,
          rng: gameStream.current.fn,
          player: node.player,
        };
        const action = agent.choose(observe(s, v, node.player), node.actions, ctx);
        record(node.player, describeForLog(action, s, v));
        const humans = config.seats.flatMap((name, i) => (name ? [] : [i]));
        const before = markInfo(s, humans);
        applyActionMut(s, v, action);
        if (revealedSince(s, humans, before)) revealedRef.current = true;
        advance();
      }, config.botDelay);
      return;
    }
    setBusy(false);
    bump();
  }, [bump, config.botDelay, config.players, config.seats, config.setupIndex, pushUndo, record]);

  useEffect(() => {
    advance();
    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config]);

  const s = stateRef.current;
  const node = getPending(s, variant);
  const humanSeats = config.seats.flatMap((name, i) => (name ? [] : [i]));
  const actor = node.kind === 'decision' ? node.player : null;
  const humanTurn = actor !== null && !agentsRef.current[actor];

  const play = useCallback(
    (a: Action) => {
      const v = makeVariant(config.players, config.setupIndex);
      const cur = stateRef.current;
      const pending = getPending(cur, v);
      if (pending.kind !== 'decision') return;
      record(pending.player, describeForLog(a, cur, v));
      const humans = config.seats.flatMap((name, i) => (name ? [] : [i]));
      const before = markInfo(cur, humans);
      applyActionMut(cur, v, a);
      if (revealedSince(cur, humans, before)) revealedRef.current = true;
      advance();
    },
    [advance, config.players, config.seats, config.setupIndex, record],
  );

  /**
   * Rewind to the previous rewind point. The stack top is always *this*
   * moment (it was pushed when the engine stopped here), so undo discards it
   * and restores the one before — the human's previous decision, with any bot
   * turns in between unwound along with it. The stored copy is cloned on the
   * way out so repeated undo keeps working, and every RNG stream is replayed
   * to its recorded position so the rewound game re-draws the same dice and
   * deals the player already saw.
   */
  const undo = useCallback(() => {
    if (timer.current !== null || undoStack.current.length < 2) return;
    undoStack.current.pop();
    const snap = undoStack.current[undoStack.current.length - 1];
    stateRef.current = cloneState(snap.state);
    logRef.current = snap.log;
    revealedRef.current = false;
    gameStream.current.restore(snap.draws);
    ctxStreams.current.forEach((stream, i) => stream.restore(snap.ctxDraws[i] ?? 0));
    setBusy(false);
    bump();
  }, [bump]);

  const reset = useCallback(
    (patch?: Partial<GameConfig>) => {
      if (timer.current !== null) window.clearTimeout(timer.current);
      setConfig((c) => ({ ...c, ...patch }));
    },
    [],
  );

  return {
    variant,
    state: s,
    actions: humanTurn && node.kind === 'decision' ? node.actions : [],
    actor,
    busy: busy || (actor !== null && !humanTurn),
    humanSeats,
    log: logRef.current,
    play,
    undo,
    canUndo: undoStack.current.length >= 2 && timer.current === null,
    reset,
    config,
  };
}

function describeForLog(a: Action, s: GameState, v: VariantDef): string {
  return describeAction(a, s, v);
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

export function finalStandings(s: GameState) {
  return standings(s);
}
