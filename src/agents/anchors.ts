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

/** `defaultWeights` as they stood when the gauntlet was established. */
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
  resource: 0.7,
  courtAgent: 0.35,
  courtLead: 1.1,
  guildCard: 1.0,
  initiative: 1.2,
  handCard: 0.25,
  outrage: 1.0,
};

/**
 * The gauntlet ladder, oldest to newest. A candidate must beat the newest
 * anchor separated and regress against none of the older ones.
 */
export const anchorLadder: string[] = ['anchor-greedy-v0', 'anchor-mcts300-v0'];
