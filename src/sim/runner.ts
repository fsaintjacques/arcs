import {
  applyActionMut,
  getPending,
  makeVariant,
  mulberry32,
  newGame,
  observe,
  resolveChanceMut,
  standings,
  type GameState,
  type VariantDef,
} from '../engine';
import type { Agent } from '../agents';

export interface GameResult {
  seed: number;
  variant: VariantDef;
  state: GameState;
  /** Final Power per seat. */
  power: number[];
  /** Winning seat. */
  winner: number;
  /** Finishing rank per seat, 0 = winner. */
  ranks: number[];
  chapters: number;
  decisions: number;
  /** Decisions taken by each seat. */
  decisionsBy: number[];
}

export interface PlayOptions {
  players: number;
  seed: number;
  setupIndex?: number;
  /** Hard stop, to turn an engine livelock into a test failure. */
  maxDecisions?: number;
  onDecision?: (state: GameState, player: number, action: unknown) => void;
}

/** Play one seeded game to completion. Agents are seated in array order. */
export function playGame(agents: Agent[], opts: PlayOptions): GameResult {
  const rng = mulberry32(opts.seed);
  const variant = makeVariant(opts.players, opts.setupIndex ?? 0);
  const s = newGame(variant, rng, opts.setupIndex ?? 0);
  const limit = opts.maxDecisions ?? 200_000;

  const ctxs = agents.map((_, player) => ({
    variant,
    rng: mulberry32((opts.seed ^ (0x9e3779b9 * (player + 1))) >>> 0),
    player,
  }));

  let decisions = 0;
  const decisionsBy = agents.map(() => 0);

  for (;;) {
    const node = getPending(s, variant);
    if (node.kind === 'over') break;
    if (node.kind === 'chance') {
      resolveChanceMut(s, variant, rng);
      continue;
    }
    if (decisions++ > limit) {
      throw new Error(`game did not terminate after ${limit} decisions (phase ${s.phase})`);
    }
    const player = node.player;
    const obs = observe(s, variant, player);
    const action = agents[player].choose(obs, node.actions, ctxs[player]);
    opts.onDecision?.(s, player, action);
    decisionsBy[player]++;
    applyActionMut(s, variant, action);
  }

  const table = standings(s);
  const ranks = agents.map(() => 0);
  for (const row of table) ranks[row.player] = row.rank;

  return {
    seed: opts.seed,
    variant,
    state: s,
    power: s.playerStates.map((p) => p.power),
    winner: table[0].player,
    ranks,
    chapters: s.chapter,
    decisions,
    decisionsBy,
  };
}

export interface SimOptions extends Omit<PlayOptions, 'seed'> {
  games: number;
  seed: number;
  /** Cycle agents through every seating permutation (default true). */
  rotateSeats?: boolean;
  onGame?: (result: GameResult, index: number, seating: number[]) => void;
}

/** Every permutation of `n` seats, in a stable order. n <= 4, so at most 24. */
export function permutations(n: number): number[][] {
  if (n <= 1) return [[0]];
  const out: number[][] = [];
  for (const rest of permutations(n - 1)) {
    for (let i = 0; i <= rest.length; i++) {
      out.push([...rest.slice(0, i), n - 1, ...rest.slice(i)]);
    }
  }
  return out;
}

export interface SimResult {
  /** Results in play order, plus the seating used for each. */
  games: { result: GameResult; seating: number[] }[];
}

/**
 * Run a batch. `seating[seat] = agentIndex`.
 *
 * Seating cycles through every *permutation*, not just rotations. Rotating
 * alone leaves the agents' cyclic order fixed, and in a game with lead-and-
 * follow turn order that is a real advantage — sitting immediately after a
 * weak player is worth several points a game — so two identical agents would
 * post very different win rates. Permuting removes it.
 */
export function simulate(agents: Agent[], opts: SimOptions): SimResult {
  const games: SimResult['games'] = [];
  const n = agents.length;
  const perms = permutations(n);

  for (let i = 0; i < opts.games; i++) {
    const seating =
      opts.rotateSeats === false ? Array.from({ length: n }, (_, seat) => seat) : perms[i % perms.length];
    const seated = seating.map((agentIndex) => agents[agentIndex]);
    const result = playGame(seated, {
      ...opts,
      seed: (opts.seed + i * 2654435761) >>> 0,
      setupIndex: (opts.setupIndex ?? 0) + i,
    });
    games.push({ result, seating });
    opts.onGame?.(result, i, seating);
  }
  return { games };
}
