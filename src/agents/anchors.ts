/**
 * Frozen yardsticks for the strength gauntlet (docs/GAUNTLET.md).
 *
 * An anchor never moves once frozen: its weights are a literal copy taken on
 * the day it was frozen, not a reference to `defaultWeights`, so tuning the
 * live defaults cannot silently re-baseline every past measurement. The list
 * is append-only — a bot that beats the ladder becomes a *new* anchor
 * generation (`-v1`, `-v2`, …); an old anchor is never edited.
 *
 * `makeAgent(name)` with no opts is the anchor's identity. The factories still
 * merge caller opts on top so the test suite can run them cheaply (see the
 * FAST map in tests/agents.test.ts); the gauntlet itself never passes any.
 */
import type { Weights } from './eval';

/**
 * `defaultWeights` as they stood when the gauntlet was established,
 * re-expressed whenever the `Weights` struct grows: terms added later carry
 * weight 0 and the flat `resource: 0.7` became a uniform per-type map, so
 * the evaluation computes the identical number it did on freeze day.
 */
export const anchorWeightsV0: Weights = {
  power: 1,
  declaredLead: 0.9,
  declaredContest: 0.35,
  latentAmbition: 0.45,
  freshShip: 0.5,
  damagedShip: 0.2,
  starport: 1.4,
  city: 2.2,
  control: 0.5,
  resourceSlot: 0.4,
  resourceValue: { material: 0.7, fuel: 0.7, weapon: 0.7, relic: 0.7, psionic: 0.7 },
  courtAgent: 0.35,
  courtLead: 1.1,
  guildCard: 1.0,
  initiative: 1.2,
  handCard: 0.25,
  handPips: 0,
  handActionable: 0,
  handHighCard: 0,
  declarableLead: 0,
  outrage: 1.0,
};

/**
 * The M2 evaluation weights as frozen with `anchor-mcts-c-v1` (2026-08-09):
 * the hand-quality/scarcity terms at their hand-set values, before any
 * tuned weights are promoted.
 */
export const anchorWeightsV1: Weights = {
  power: 1,
  declaredLead: 0.9,
  declaredContest: 0.35,
  latentAmbition: 0.35,
  freshShip: 0.5,
  damagedShip: 0.2,
  starport: 1.4,
  city: 2.2,
  control: 0.5,
  resourceSlot: 0.4,
  resourceValue: { material: 0.65, fuel: 0.65, weapon: 0.5, relic: 0.85, psionic: 0.85 },
  courtAgent: 0.35,
  courtLead: 1.1,
  guildCard: 1.0,
  initiative: 1.2,
  handCard: 0.1,
  handPips: 0.15,
  handActionable: 0.1,
  handHighCard: 0.3,
  declarableLead: 0.5,
  outrage: 1.0,
};

/**
 * The gauntlet ladder, oldest to newest. A candidate must beat the newest
 * anchor separated and regress against none of the older ones.
 */
export const anchorLadder: string[] = [
  'anchor-greedy-v0',
  'anchor-mcts300-v0',
  'anchor-mcts-c-v1',
];
