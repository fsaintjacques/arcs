/**
 * CEM-tuned weights, run 1 (2026-08-09): 30 generations, pop 24, elite 6,
 * 96 paired games per member vs the M2-default greedy, seed 1
 * (tools/tune-cem.ts). Generated file — retune rather than hand-edit.
 *
 * Gauntlet verdict (docs/GAUNTLET.md): clearly stronger on the greedy
 * carrier, no separated transfer to mcts, so NOT promoted to
 * defaultWeights. Registered as greedy-t1 / mcts-t1 for experiments;
 * re-test when search values leaves with the evaluation directly.
 */
import type { Weights } from '../eval';

export const tunedWeightsV1: Weights = {
  power: 1,
  declaredLead: 0.9768,
  declaredContest: 0.5434,
  latentAmbition: 0.3485,
  freshShip: 0.4884,
  damagedShip: 0.1182,
  starport: 0.958,
  city: 2.7369,
  control: 0.8077,
  resourceSlot: 0.351,
  resourceValue: { material: 0.5087, fuel: 0.5397, weapon: 0.3801, relic: 1.2298, psionic: 0.9883 },
  courtAgent: 0.3715,
  courtLead: 1.3767,
  guildCard: 0.9824,
  initiative: 0.9054,
  handCard: 0.1131,
  handPips: 0.2623,
  handActionable: 0.1613,
  handHighCard: 0.2193,
  declarableLead: 0.2386,
  outrage: 0.9169,
};
