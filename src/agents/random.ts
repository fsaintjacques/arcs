import type { Agent } from './types';

/** Uniform random over legal actions. The floor baseline. */
export function makeRandom(): Agent {
  return {
    name: 'random',
    choose(_obs, actions, ctx) {
      return actions[Math.floor(ctx.rng() * actions.length)];
    },
  };
}

/**
 * Random, but never ends a turn while it still has actions to spend and never
 * throws cards away to seize. A meaningfully stronger floor than `random`,
 * and the default rollout policy for search agents.
 */
export function makeRandomPlus(): Agent {
  return {
    name: 'random+',
    choose(_obs, actions, ctx) {
      const useful = actions.filter((a) => a.t !== 'endTurn' && a.t !== 'seize');
      const pool = useful.length > 0 ? useful : actions;
      return pool[Math.floor(ctx.rng() * pool.length)];
    },
  };
}
