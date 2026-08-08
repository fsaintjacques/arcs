import type { Action, Observation, RNG, VariantDef } from '../engine';

export interface AgentCtx {
  variant: VariantDef;
  /** RNG for the agent's own randomness (rollouts, tie-breaks). */
  rng: RNG;
  /** Seat this agent is playing. */
  player: number;
}

/**
 * An agent picks one of the legal actions at every decision node.
 *
 * Agents are handed an `Observation`, not the raw state, so they cannot read
 * Rival hands or deck order by accident — Arcs is an imperfect-information
 * game and honest bots are the point of the lab. Search agents call
 * `determinize(obs, variant, rng)` to sample a complete world to search in.
 *
 * `obs.state` must be treated as read-only; use `applyAction` (pure) or
 * `cloneState` + the `*Mut` functions to look ahead.
 */
export interface Agent {
  name: string;
  choose(obs: Observation, actions: Action[], ctx: AgentCtx): Action;
}

export type AgentFactory = (opts?: Record<string, unknown>) => Agent;
