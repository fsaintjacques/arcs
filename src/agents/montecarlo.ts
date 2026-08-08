/**
 * Flat Monte-Carlo: sample a world, play every candidate action out several
 * times with a cheap policy, keep the action with the best mean value.
 *
 * No tree, so it is blind to its own follow-up decisions, but it is the
 * cheapest agent that reasons about consequences several plies deep and it is
 * a useful control for whether MCTS's tree is actually earning its cost.
 */
import { applyAction, cloneState, determinize } from '../engine';
import type { Action, GameState } from '../engine';
import { defaultWeights, type Weights } from './eval';
import { narrow, rollout } from './rollout';
import type { Agent } from './types';

export interface MonteCarloOpts {
  /** Rollouts per candidate action. */
  rollouts: number;
  /** Decisions played per rollout before the heuristic takes over. */
  depth: number;
  /** Cap on candidate actions considered (see `narrow`). */
  maxActions: number;
  weights: Partial<Weights>;
}

export function makeMonteCarlo(opts: Partial<MonteCarloOpts> = {}, name = 'mc'): Agent {
  const rollouts = opts.rollouts ?? 12;
  const depth = opts.depth ?? 40;
  const maxActions = opts.maxActions ?? 16;
  const w: Weights = { ...defaultWeights, ...opts.weights };

  return {
    name,
    choose(obs, actions, ctx) {
      const v = ctx.variant;
      const candidates = narrow(actions, maxActions);
      let best: Action = candidates[0];
      let bestValue = -Infinity;

      for (const a of candidates) {
        let total = 0;
        let played = 0;
        for (let i = 0; i < rollouts; i++) {
          const world = determinize(obs, v, ctx.rng);
          let next: GameState;
          try {
            next = applyAction(world, v, a);
          } catch {
            continue;
          }
          total += rollout(cloneState(next), v, ctx.rng, { depth, weights: w })[ctx.player];
          played++;
        }
        if (played === 0) continue;
        const value = total / played;
        if (value > bestValue) {
          bestValue = value;
          best = a;
        }
      }
      return best;
    },
  };
}
