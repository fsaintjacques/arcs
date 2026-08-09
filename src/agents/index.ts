import { anchorWeightsV0, anchorWeightsV1 } from './anchors';
import { makeGreedy } from './greedy';
import { makeMcts } from './mcts';
import { makeMcts2 } from './mcts2';
import { makeMonteCarlo } from './montecarlo';
import { makeRandom, makeRandomPlus } from './random';
import { tunedWeightsV1 } from './weights/tuned1';
import type { Agent, AgentFactory } from './types';

export * from './types';
export * from './eval';
export * from './rollout';
export * from './random';
export * from './greedy';
export * from './montecarlo';
export * from './mcts';
export * from './mcts2';
export * from './anchors';
export * from './dicemath';
export * from './candidates';

export const agents: Record<string, AgentFactory> = {
  /** Uniform random over legal actions — the floor. */
  random: () => makeRandom(),
  /** Random, but never idles a turn away. The rollout policy. */
  'random+': () => makeRandomPlus(),
  /** One-step lookahead over the heuristic, cascades settled. */
  greedy: (opts) => makeGreedy(opts as never),
  /** Greedy without cascade settling — shows what settling is worth. */
  'greedy-flat': (opts) => makeGreedy({ settle: false, ...opts }, 'greedy-flat'),
  /** Flat Monte-Carlo over sampled worlds. */
  mc: (opts) => makeMonteCarlo(opts as never),
  /** Determinized ISMCTS with max^n backup. */
  mcts: (opts) => makeMcts(opts as never),
  /** A deliberately cheap MCTS, for fast batches. */
  'mcts-fast': (opts) => makeMcts({ iterations: 120, rolloutDepth: 20, ...opts }, 'mcts-fast'),
  /** MCTS with informed candidate trimming instead of blind narrow(). */
  'mcts-c': (opts) => makeMcts({ candidates: true, ...opts }, 'mcts-c'),
  /** Truncated PUCT search: eval leaves, eval priors, pooled worlds. */
  mcts2: (opts) => makeMcts2({ iterations: 600, ...opts }),
  /** The same brain with an interactive wall-clock budget. */
  'mcts2-play': (opts) => makeMcts2({ iterations: 100_000, timeMs: 600, ...opts }, 'mcts2-play'),
  /** CEM run-1 weights (weights/tuned1.ts) — experimental, not promoted. */
  'greedy-t1': (opts) => makeGreedy({ weights: tunedWeightsV1, ...opts }, 'greedy-t1'),
  'mcts-t1': (opts) => makeMcts({ weights: tunedWeightsV1, ...opts }, 'mcts-t1'),
  /** Frozen gauntlet anchors — see anchors.ts. Never retune these. */
  'anchor-greedy-v0': (opts) =>
    makeGreedy({ weights: anchorWeightsV0, ...opts }, 'anchor-greedy-v0'),
  'anchor-mcts300-v0': (opts) =>
    makeMcts({ iterations: 300, weights: anchorWeightsV0, ...opts }, 'anchor-mcts300-v0'),
  'anchor-mcts-c-v1': (opts) =>
    makeMcts(
      { iterations: 300, candidates: true, weights: anchorWeightsV1, ...opts },
      'anchor-mcts-c-v1',
    ),
};

export function makeAgent(name: string, opts?: Record<string, unknown>): Agent {
  const f = agents[name];
  if (!f) {
    throw new Error(`unknown agent '${name}' (available: ${Object.keys(agents).join(', ')})`);
  }
  return f(opts);
}

export function agentNames(): string[] {
  return Object.keys(agents);
}
