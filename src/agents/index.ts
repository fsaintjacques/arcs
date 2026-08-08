import { makeGreedy } from './greedy';
import { makeMcts } from './mcts';
import { makeMonteCarlo } from './montecarlo';
import { makeRandom, makeRandomPlus } from './random';
import type { Agent, AgentFactory } from './types';

export * from './types';
export * from './eval';
export * from './rollout';
export * from './random';
export * from './greedy';
export * from './montecarlo';
export * from './mcts';

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
