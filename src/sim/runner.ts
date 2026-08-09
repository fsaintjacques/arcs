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
  type SetupMode,
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
  /**
   * `deck` (default) draws one of the four setup cards for this player count;
   * `draw` invents a fresh legal opening per seed. See `SetupMode`.
   */
  setupMode?: SetupMode;
  /** Hard stop, to turn an engine livelock into a test failure. */
  maxDecisions?: number;
  onDecision?: (state: GameState, player: number, action: unknown) => void;
}

/** Play one seeded game to completion. Agents are seated in array order. */
export function playGame(agents: Agent[], opts: PlayOptions): GameResult {
  const rng = mulberry32(opts.seed);
  const mode = opts.setupMode ?? 'deck';
  const variant = makeVariant(opts.players, opts.setupIndex ?? 0, mode);
  const s = newGame(variant, rng, opts.setupIndex ?? 0, mode);
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
  /**
   * Hold the deal fixed across each block of `n!` seatings (default true), so
   * every agent meets the identical game in every seat. See `simulate`.
   */
  paired?: boolean;
  /**
   * Run only these block indices instead of `0..ceil(games/blockSize)-1`.
   * A block's games depend on nothing but its index, so a batch can be
   * partitioned across workers and reassembled without changing a single
   * seed or seating — the parallel runner leans on this.
   */
  blocks?: number[];
  onGame?: (result: GameResult, index: number, seating: number[], block: number) => void;
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
  games: { result: GameResult; seating: number[]; block: number }[];
  /** Games per block: `n!` when paired, otherwise 1. */
  blockSize: number;
  paired: boolean;
}

/**
 * Run a batch. `seating[seat] = agentIndex`.
 *
 * Two variance controls, and they do different jobs:
 *
 * **Permuted seating.** Seating cycles through every *permutation*, not just
 * rotations. Rotating alone leaves the agents' cyclic order fixed, and in a
 * lead-and-follow game that is a real advantage — sitting immediately after a
 * weak player is worth several points a game — so two identical agents would
 * post very different win rates.
 *
 * **Common random numbers.** The deal is held fixed across each block of `n!`
 * seatings, so within a block every agent plays the *same game* from every
 * seat. Without this the permutations are spread across `n!` unrelated deals
 * and cancel nothing: the agent that happened to draw the better cards wins,
 * and the comparison measures the shuffle. Pairing makes the deal a controlled
 * variable instead of a source of noise, and `pairedStats` reads the resulting
 * within-block differences — which is where the interval shrinks.
 *
 * `games` is rounded up to a whole number of blocks so no block is partial;
 * a half-finished block would reintroduce exactly the seat bias permuting is
 * there to remove.
 */
export function simulate(agents: Agent[], opts: SimOptions): SimResult {
  const games: SimResult['games'] = [];
  const n = agents.length;
  const perms = permutations(n);
  const rotate = opts.rotateSeats !== false;
  const paired = rotate && opts.paired !== false;
  const blockSize = paired ? perms.length : 1;
  const blockCount = Math.ceil(opts.games / blockSize);
  const blockList = opts.blocks ?? Array.from({ length: blockCount }, (_, b) => b);

  for (const block of blockList) {
    // One deal per block when paired; blocks are single games otherwise, so
    // the block index is the deal index either way.
    const seed = (opts.seed + block * 2654435761) >>> 0;
    const setupIndex = (opts.setupIndex ?? 0) + block;

    for (let k = 0; k < blockSize; k++) {
      const i = block * blockSize + k;
      const seating = rotate
        ? perms[(paired ? k : i) % perms.length]
        : Array.from({ length: n }, (_, seat) => seat);
      const seated = seating.map((agentIndex) => agents[agentIndex]);
      const result = playGame(seated, { ...opts, seed, setupIndex });
      games.push({ result, seating, block });
      opts.onGame?.(result, i, seating, block);
    }
  }
  return { games, blockSize, paired };
}
