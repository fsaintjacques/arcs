/**
 * Drives one game in the browser: the engine advances itself through chance
 * nodes and bot seats, and stops whenever a human seat owes a decision.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  applyActionMut,
  getPending,
  makeVariant,
  mulberry32,
  newGame,
  observe,
  resolveChanceMut,
  standings,
  type Action,
  type GameState,
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
  reset: (config?: Partial<GameConfig>) => void;
  config: GameConfig;
}

function buildAgents(config: GameConfig): (Agent | null)[] {
  return config.seats.map((name) => (name ? makeAgent(name) : null));
}

export function useGame(initial: GameConfig): GameHandle {
  const [config, setConfig] = useState(initial);
  const [, forceRender] = useState(0);
  const bump = useCallback(() => forceRender((n) => n + 1), []);

  const variant = useMemo(
    () => makeVariant(config.players, config.setupIndex),
    [config.players, config.setupIndex],
  );

  const rngRef = useRef(mulberry32(config.seed));
  const stateRef = useRef<GameState>(newGame(variant, rngRef.current, config.setupIndex));
  const logRef = useRef<LogEntry[]>([]);
  const agentsRef = useRef<(Agent | null)[]>(buildAgents(config));
  const ctxsRef = useRef<AgentCtx[]>([]);
  const [busy, setBusy] = useState(false);
  const timer = useRef<number | null>(null);

  const start = useCallback(
    (c: GameConfig) => {
      const v = makeVariant(c.players, c.setupIndex);
      rngRef.current = mulberry32(c.seed);
      stateRef.current = newGame(v, rngRef.current, c.setupIndex);
      logRef.current = [];
      agentsRef.current = buildAgents(c);
      ctxsRef.current = c.seats.map((_, player) => ({
        variant: v,
        rng: mulberry32((c.seed ^ (0x9e3779b9 * (player + 1))) >>> 0),
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
   * Advance until a human owes a decision or the game ends. Bot turns are
   * spaced out with a timer so the board can be watched.
   */
  const advance = useCallback(() => {
    const v = makeVariant(config.players, config.setupIndex);
    const s = stateRef.current;

    for (let guard = 0; guard < 500; guard++) {
      const node = getPending(s, v);
      if (node.kind === 'over') {
        setBusy(false);
        bump();
        return;
      }
      if (node.kind === 'chance') {
        resolveChanceMut(s, v, rngRef.current);
        continue;
      }
      const agent = agentsRef.current[node.player];
      if (!agent) {
        setBusy(false);
        bump();
        return;
      }
      // A bot owes a decision: schedule it so the UI can paint first.
      setBusy(true);
      bump();
      timer.current = window.setTimeout(() => {
        const ctx = ctxsRef.current[node.player] ?? {
          variant: v,
          rng: rngRef.current,
          player: node.player,
        };
        const action = agent.choose(observe(s, v, node.player), node.actions, ctx);
        record(node.player, describeForLog(action, s, v));
        applyActionMut(s, v, action);
        advance();
      }, config.botDelay);
      return;
    }
    setBusy(false);
    bump();
  }, [bump, config.botDelay, config.players, config.setupIndex, record]);

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
      applyActionMut(cur, v, a);
      advance();
    },
    [advance, config.players, config.setupIndex, record],
  );

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
    reset,
    config,
  };
}

function describeForLog(a: Action, s: GameState, v: VariantDef): string {
  return describeAction(a, s, v);
}

export function finalStandings(s: GameState) {
  return standings(s);
}
