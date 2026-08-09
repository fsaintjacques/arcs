/**
 * Truncated determinized ISMCTS with PUCT — the search built for the
 * evaluation this repo spent three milestones improving.
 *
 * FINDINGS.md records the same result three times: evaluation quality pays
 * at 1 ply and drowns by thirty random rollout decisions. So this agent has
 * no rollouts. Structure, and why each piece is here:
 *
 *   - **Truncated leaves.** An iteration descends the tree and values the
 *     reached position with `valueVector` directly. The 30-decision random+
 *     rollout was the per-iteration budget hog *and* the filter that kept
 *     eval improvements from mattering; dropping it buys both.
 *   - **PUCT with eval priors.** At a node's expansion, every candidate is
 *     applied once on a clone and scored with `relativeEvaluate`; a softmax
 *     over those scores becomes the prior. Selection is
 *     Q + cPuct · P · √ΣN / (1 + N), with max^n Q per seat as before —
 *     opponents maximise themselves, not a coalition against the root.
 *   - **World pooling.** `worlds` determinizations are sampled once per
 *     decision and cycled across iterations, so each world is searched
 *     several times instead of paying `determinize` per iteration.
 *   - **Exact battle chance at the frontier.** When an iteration ends on an
 *     unrolled battle, the leaf value is the probability-weighted eval over
 *     `battleDistribution`'s top mass instead of one sampled roll. Chance
 *     met mid-descent stays open-loop (sampled), as in `mcts`.
 *   - Candidates come from `generateCandidates`, node keys from
 *     `encodeAction`.
 *
 * The ablation switches (`priors`, `rolloutLeaf`, `worlds: 0`) exist so the
 * gauntlet can price each idea separately.
 */
import {
  applyActionMut,
  applyBattleRollMut,
  cloneState,
  determinize,
  encodeAction,
  getPending,
  resolveChanceMut,
} from '../engine';
import type { Action, GameState, RNG } from '../engine';
import { generateCandidates } from './candidates';
import { battleDistribution, topMass } from './dicemath';
import { defaultWeights, relativeEvaluate, type Weights } from './eval';
import { rollout, terminalVector, valueVector } from './rollout';
import type { Agent } from './types';

export interface Mcts2Opts {
  /** Iteration cap per decision. */
  iterations: number;
  /** Wall-clock budget in ms; whichever of the two limits hits first. */
  timeMs?: number;
  /** PUCT exploration constant. */
  cPuct: number;
  /** Cap on candidates per node (see `generateCandidates`). */
  maxActions: number;
  /** Determinizations pooled per decision; 0 = fresh world per iteration. */
  worlds: number;
  /** Softmax temperature over prior evals, in evaluation units. */
  priorTemp: number;
  /** Probability mass enumerated exactly at frontier battles. */
  battleMass: number;
  /** Descent-depth safety cap. */
  maxDepth: number;
  /** Ablation: uniform priors instead of eval priors. */
  priors: boolean;
  /** Ablation: value leaves with a random+ rollout instead of the eval. */
  rolloutLeaf: boolean;
  weights: Partial<Weights>;
}

interface Node {
  player: number;
  visits: number;
  /** Backed-up value per seat (max^n). */
  totals: number[];
  children: Map<string, Node>;
  action: Action | null;
  /** Softmax prior per action key, fixed at expansion time. */
  priors: Map<string, number> | null;
}

function newNode(player: number, action: Action | null, players: number): Node {
  return { player, visits: 0, totals: Array(players).fill(0), children: new Map(), action, priors: null };
}

export function makeMcts2(opts: Partial<Mcts2Opts> = {}, name = 'mcts2'): Agent {
  const iterations = opts.iterations ?? 400;
  const timeMs = opts.timeMs;
  const cPuct = opts.cPuct ?? 1.5;
  const maxActions = opts.maxActions ?? 16;
  const worldCount = opts.worlds ?? 16;
  const priorTemp = opts.priorTemp ?? 1.0;
  const battleMass = opts.battleMass ?? 0.9;
  const maxDepth = opts.maxDepth ?? 64;
  const usePriors = opts.priors ?? true;
  const rolloutLeaf = opts.rolloutLeaf ?? false;
  const w: Weights = { ...defaultWeights, ...opts.weights };

  return {
    name,
    choose(obs, actions, ctx) {
      const v = ctx.variant;
      if (actions.length === 1) return actions[0];

      const root = newNode(ctx.player, null, obs.state.players);
      const pool: GameState[] =
        worldCount > 0
          ? Array.from({ length: worldCount }, () => determinize(obs, v, ctx.rng))
          : [];
      const deadline = timeMs === undefined ? Infinity : performance.now() + timeMs;

      for (let iter = 0; iter < iterations; iter++) {
        if (iter > 0 && iter % 8 === 0 && performance.now() >= deadline) break;
        const state =
          worldCount > 0 ? cloneState(pool[iter % worldCount]) : determinize(obs, v, ctx.rng);
        descend(root, state, 0);
      }

      // Most-visited root child: robust to a single lucky evaluation.
      let best: Action = actions[0];
      let bestVisits = -1;
      for (const child of root.children.values()) {
        if (child.visits > bestVisits) {
          bestVisits = child.visits;
          best = child.action!;
        }
      }
      return best;

      /** Value the frontier position without descending further. */
      function frontierValue(state: GameState, rng: RNG): number[] {
        if (rolloutLeaf) return rollout(state, v, rng, { depth: 30, weights: w });
        const pending = getPending(state, v);
        if (pending.kind === 'over') return terminalVector(state);
        // An unrolled battle: exact expectation over the printed dice.
        if (pending.kind === 'chance' && state.phase === 'battleRoll' && state.battle!.pendingReroll === 0) {
          const d = state.battle!.dice;
          const outcomes = topMass(battleDistribution(d.assault, d.skirmish, d.raid), battleMass);
          const players = state.players;
          const acc = Array(players).fill(0) as number[];
          for (const o of outcomes) {
            const branch = cloneState(state);
            applyBattleRollMut(branch, v, o.totals);
            const val = valueVector(branch, v, w);
            for (let p = 0; p < players; p++) acc[p] += o.p * val[p];
          }
          return acc;
        }
        return valueVector(state, v, w);
      }

      function descend(node: Node, state: GameState, depth: number): number[] {
        // Resolve chance owed before the node — open loop, except that a
        // battle roll at the frontier is valued exactly in frontierValue.
        for (let guard = 0; guard < 64; guard++) {
          const n = getPending(state, v);
          if (n.kind !== 'chance') break;
          if (node.visits === 0 && state.phase === 'battleRoll' && !rolloutLeaf) break;
          resolveChanceMut(state, v, ctx.rng);
        }

        const pending = getPending(state, v);
        if (pending.kind !== 'decision') {
          const value =
            pending.kind === 'over' ? terminalVector(state) : frontierValue(state, ctx.rng);
          backup(node, value);
          return value;
        }
        if (depth >= maxDepth) {
          const value = frontierValue(state, ctx.rng);
          backup(node, value);
          return value;
        }

        node.player = pending.player;

        // First visit: value the frontier and stop — one new node per
        // iteration. Priors wait for the second visit, so the many nodes
        // the search never returns to never pay for them.
        if (node.visits === 0) {
          const value = frontierValue(state, ctx.rng);
          backup(node, value);
          return value;
        }

        const candidates = generateCandidates(state, v, pending.player, pending.actions, {
          max: maxActions,
          weights: w,
        });
        if (node.priors === null) {
          node.priors = computePriors(state, pending.player, candidates);
        }

        const chosen = puctChild(node, candidates);
        if (!chosen) {
          const value = frontierValue(state, ctx.rng);
          backup(node, value);
          return value;
        }
        applyActionMut(state, v, chosen.action!);
        const value = descend(chosen, state, depth + 1);
        backup(node, value);
        return value;
      }

      function computePriors(state: GameState, player: number, candidates: Action[]): Map<string, number> {
        const priors = new Map<string, number>();
        if (!usePriors || candidates.length === 0) {
          for (const a of candidates) priors.set(encodeAction(a), 1 / Math.max(1, candidates.length));
          return priors;
        }
        const scores: { key: string; score: number }[] = [];
        let bestScore = -Infinity;
        for (const a of candidates) {
          const key = encodeAction(a);
          let score = -Infinity;
          try {
            const after = cloneState(state);
            applyActionMut(after, v, a);
            score = relativeEvaluate(after, v, player, w);
          } catch {
            // Illegal in this world; keep -Infinity so its prior floors out.
          }
          scores.push({ key, score });
          if (score > bestScore) bestScore = score;
        }
        let total = 0;
        for (const s of scores) {
          const p = Math.exp(Math.min(0, (s.score - bestScore) / priorTemp));
          priors.set(s.key, p);
          total += p;
        }
        for (const [k, p] of priors) priors.set(k, p / total);
        return priors;
      }

      function puctChild(node: Node, candidates: Action[]): Node | null {
        // Availability varies per world; explore over what this world allows.
        let sumVisits = 0;
        for (const a of candidates) sumVisits += node.children.get(encodeAction(a))?.visits ?? 0;
        const sqrtSum = Math.sqrt(sumVisits + 1);
        const parentAvg = node.visits > 0 ? node.totals[node.player] / node.visits : 0;
        const fallbackPrior = 1 / Math.max(1, candidates.length);

        let best: Node | null = null;
        let bestAction: Action | null = null;
        let bestScore = -Infinity;
        for (const a of candidates) {
          const key = encodeAction(a);
          const child = node.children.get(key);
          const visits = child?.visits ?? 0;
          // Unvisited children start from the parent's running value rather
          // than infinity, so the prior actually orders first exploration.
          const q = child && visits > 0 ? child.totals[node.player] / visits : parentAvg;
          const p = node.priors!.get(key) ?? fallbackPrior;
          const score = q + cPuct * p * (sqrtSum / (1 + visits));
          if (score > bestScore) {
            bestScore = score;
            best = child ?? null;
            bestAction = a;
          }
        }
        if (bestAction === null) return null;
        if (best === null) {
          best = newNode(node.player, bestAction, obs.state.players);
          node.children.set(encodeAction(bestAction), best);
        }
        return best;
      }
    },
  };
}

function backup(node: Node, value: number[]): void {
  node.visits++;
  for (let i = 0; i < value.length; i++) node.totals[i] += value[i];
}
