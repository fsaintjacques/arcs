/**
 * One-step lookahead over `relativeEvaluate`.
 *
 * The agent determinizes its observation once per decision, so the lookahead
 * happens in a complete legal world rather than one with blanked-out hands,
 * then plays the action with the best resulting position.
 *
 * Actions in Arcs open cascades — a Battle rolls dice and then asks where the
 * hits land; a Move off a starport asks whether to keep Catapulting. A naive
 * one-step eval scores `battle` *before* any damage exists and so never fights.
 * `settle()` therefore plays the cascade out to the end of the action, taking
 * chance nodes from the RNG and resolving the agent's own follow-up decisions
 * with the same greedy rule.
 */
import {
  applyAction,
  applyBattleRollMut,
  cloneState,
  determinize,
  getPending,
  resolveChance,
} from '../engine';
import type { Action, GameState, VariantDef } from '../engine';
import { battleDistribution, topMass } from './dicemath';
import { defaultWeights, relativeEvaluate, type Weights } from './eval';
import type { Agent, AgentCtx } from './types';

export interface GreedyOpts {
  weights: Partial<Weights>;
  /** Play cascades out before scoring. Turning this off makes a weaker bot. */
  settle: boolean;
  /**
   * Cascade chance nodes are sampled this many times and the values
   * averaged, so a battle is judged on (an estimate of) its expectation.
   */
  samples: number;
  /**
   * How to value the battle roll. `sample` (default) averages `samples`
   * rolls; `exact` takes the expectation over `battleDistribution`. Exact
   * measured *weaker* for this 1-ply agent — see FINDINGS.md: with ~17
   * battle variants on offer, the argmax over noisy estimates plays the
   * optimistic tail, and that optimism was subsidising battles the
   * evaluation underprices. Kept as an option for search agents and
   * experiments.
   */
  battles: 'sample' | 'exact';
  /** Probability mass enumerated when `battles: 'exact'` (default 0.95). */
  battleMass: number;
}

/** Phases that belong to the action currently being resolved. */
const CASCADE_PHASES = new Set(['battleRoll', 'battleAssign', 'catapult']);

/**
 * Resolve the rest of the current action: chance nodes from the RNG, and the
 * agent's own cascade decisions by a 1-ply argmax on the same evaluation.
 */
function settle(
  s: GameState,
  v: VariantDef,
  player: number,
  w: Weights,
  rng: () => number,
  limit = 64,
): GameState {
  let cur = s;
  for (let i = 0; i < limit; i++) {
    const node = getPending(cur, v);
    if (node.kind === 'over') break;
    if (node.kind === 'chance') {
      if (!CASCADE_PHASES.has(cur.phase)) break; // a chapter deal is not our business
      cur = resolveChance(cur, v, rng);
      continue;
    }
    if (!CASCADE_PHASES.has(cur.phase) || node.player !== player) break;

    let best: GameState | null = null;
    let bestValue = -Infinity;
    for (const a of node.actions) {
      let next: GameState;
      try {
        next = applyAction(cur, v, a);
      } catch {
        continue;
      }
      const value = relativeEvaluate(next, v, player, w);
      if (value > bestValue) {
        bestValue = value;
        best = next;
      }
    }
    if (!best) break;
    cur = best;
  }
  return cur;
}

export function makeGreedy(opts: Partial<GreedyOpts> = {}, name = 'greedy'): Agent {
  const w: Weights = { ...defaultWeights, ...opts.weights };
  const doSettle = opts.settle ?? true;
  const samples = Math.max(1, opts.samples ?? 3);
  const exactBattles = opts.battles === 'exact';
  const battleMass = opts.battleMass ?? 0.95;

  return {
    name,
    choose(obs, actions, ctx: AgentCtx) {
      const v = ctx.variant;
      const world = determinize(obs, v, ctx.rng);
      let best: Action = actions[0];
      let bestValue = -Infinity;

      for (const a of actions) {
        let after: GameState;
        try {
          after = applyAction(world, v, a);
        } catch {
          continue; // illegal in this sampled world (hidden-information mismatch)
        }

        let value: number;
        if (!doSettle) {
          value = relativeEvaluate(after, v, ctx.player, w);
        } else if (
          exactBattles &&
          getPending(after, v).kind === 'chance' &&
          after.phase === 'battleRoll' &&
          after.battle!.pendingReroll === 0
        ) {
          // The dice are printed: value the battle as the exact expectation
          // over its outcome distribution, each outcome settled to the end
          // of the action, instead of averaging a few sampled rolls.
          const d = after.battle!.dice;
          const outcomes = topMass(battleDistribution(d.assault, d.skirmish, d.raid), battleMass);
          let total = 0;
          for (const o of outcomes) {
            const branch = cloneState(after);
            applyBattleRollMut(branch, v, o.totals);
            total += o.p * relativeEvaluate(settle(branch, v, ctx.player, w, ctx.rng), v, ctx.player, w);
          }
          value = total;
        } else if (getPending(after, v).kind === 'chance' && CASCADE_PHASES.has(after.phase)) {
          // A cascade chance with no closed form here (Skirmishers reroll):
          // average over sampled resolutions.
          let total = 0;
          for (let i = 0; i < samples; i++) {
            total += relativeEvaluate(settle(after, v, ctx.player, w, ctx.rng), v, ctx.player, w);
          }
          value = total / samples;
        } else {
          value = relativeEvaluate(settle(after, v, ctx.player, w, ctx.rng), v, ctx.player, w);
        }

        if (value > bestValue) {
          bestValue = value;
          best = a;
        }
      }
      return best;
    },
  };
}
