/**
 * Shared plumbing for the sampling agents: playing a position out with a cheap
 * policy, and turning the reached position into a value for every seat.
 */
import { applyActionMut, getPending, resolveChanceMut, standings } from '../engine';
import type { GameState, RNG, VariantDef } from '../engine';
import { evaluate, type Weights } from './eval';

/**
 * Value vector over seats: each seat's share of the total heuristic value,
 * summing to 1. Shares are what max^n backup wants — bounded, comparable
 * across positions, and zero-sum-ish without assuming two players.
 */
export function valueVector(s: GameState, v: VariantDef, w: Weights): number[] {
  const raw = s.playerStates.map((_, p) => evaluate(s, v, p, w));
  const min = Math.min(...raw);
  // Shift so every share is non-negative, then normalise.
  const shifted = raw.map((x) => x - min + 1e-6);
  const total = shifted.reduce((a, b) => a + b, 0);
  return shifted.map((x) => x / total);
}

/**
 * Terminal value: 1 for the winner, 0 for everyone else. Power ties are broken
 * by turn order from the initiative holder — exactly the rule the table plays
 * by (`standings`, p19) — so search backs up the result the game would
 * actually award rather than splitting a tie the rules never split.
 */
export function terminalVector(s: GameState): number[] {
  const out: number[] = s.playerStates.map(() => 0);
  out[standings(s)[0].player] = 1;
  return out;
}

export interface RolloutOpts {
  /** Decisions to play before falling back to the heuristic. */
  depth: number;
  weights: Weights;
}

/**
 * Play `depth` decisions with a uniform-random-but-not-idle policy, then value
 * the position. Cheap on purpose: rollout quality matters much less than the
 * number of rollouts at this branching factor.
 */
export function rollout(
  start: GameState,
  v: VariantDef,
  rng: RNG,
  opts: RolloutOpts,
): number[] {
  const s = start;
  for (let i = 0; i < opts.depth; i++) {
    const node = getPending(s, v);
    if (node.kind === 'over') return terminalVector(s);
    if (node.kind === 'chance') {
      resolveChanceMut(s, v, rng);
      continue;
    }
    const useful = node.actions.filter((a) => a.t !== 'endTurn' && a.t !== 'seize');
    const pool = useful.length > 0 ? useful : node.actions;
    applyActionMut(s, v, pool[Math.floor(rng() * pool.length)]);
  }
  const node = getPending(s, v);
  if (node.kind === 'over') return terminalVector(s);
  return valueVector(s, v, opts.weights);
}

/**
 * Trim a wide action list to `max` entries. Arcs offers hundreds of legal
 * actions at some nodes (every ship count on every edge, every dice split), and
 * a fixed-budget search spends itself on breadth if it sees them all.
 *
 * The trim is **deterministic**: same action list in, same subset out. A
 * determinized search revisits a node under different sampled worlds, and a
 * randomised trim would give the node a different child set each time, so its
 * statistics would describe no particular decision. Kinds are taken
 * round-robin, so no action type is ever invisible however many `move`
 * variations crowd the list.
 */
export function narrow<T extends { t: string }>(actions: T[], max: number): T[] {
  if (actions.length <= max) return actions;
  const byKind = new Map<string, T[]>();
  for (const a of actions) {
    const list = byKind.get(a.t) ?? [];
    list.push(a);
    byKind.set(a.t, list);
  }
  const kinds = [...byKind.keys()];
  const out: T[] = [];
  for (let round = 0; out.length < max; round++) {
    let added = false;
    for (const k of kinds) {
      const list = byKind.get(k)!;
      if (round >= list.length) continue;
      out.push(list[round]);
      added = true;
      if (out.length >= max) break;
    }
    if (!added) break;
  }
  return out;
}
