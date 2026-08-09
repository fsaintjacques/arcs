/**
 * Exact battle-roll distributions.
 *
 * The die faces are printed and finite, so a battle's outcome distribution
 * is a small convolution, not something to sample: there are at most 343
 * distinct pools (0-6 dice of three types), each with a few hundred distinct
 * outcome totals. Everything is memoized, so after warm-up a lookup is free.
 * FINDINGS.md flagged this as the lever nobody pulled — `greedy` judged
 * battles on 3 sampled rolls, mcts on one per iteration.
 */
import { DICE_PER_TYPE, DIE_FACES } from '../engine';
import type { DieType, RollTotals } from '../engine';

export interface BattleOutcome {
  totals: RollTotals;
  p: number;
}

const zero = (): RollTotals => ({
  selfHits: 0,
  intercept: 0,
  hits: 0,
  buildingHits: 0,
  keys: 0,
  skirmishBlanks: 0,
});

const keyOf = (t: RollTotals): string =>
  `${t.selfHits},${t.intercept},${t.hits},${t.buildingHits},${t.keys},${t.skirmishBlanks}`;

/** Distribution over totals for `n` dice of one type. */
function typeDistribution(type: DieType, n: number): BattleOutcome[] {
  let dist = new Map<string, BattleOutcome>([[keyOf(zero()), { totals: zero(), p: 1 }]]);
  const faces = DIE_FACES[type];
  for (let die = 0; die < n; die++) {
    const next = new Map<string, BattleOutcome>();
    for (const { totals, p } of dist.values()) {
      for (const f of faces) {
        const t: RollTotals = {
          selfHits: totals.selfHits + f.selfHits,
          intercept: totals.intercept + f.intercept,
          hits: totals.hits + f.hits,
          buildingHits: totals.buildingHits + f.buildingHits,
          keys: totals.keys + f.keys,
          skirmishBlanks:
            totals.skirmishBlanks + (type === 'skirmish' && f.hits === 0 ? 1 : 0),
        };
        const k = keyOf(t);
        const row = next.get(k);
        if (row) row.p += p / faces.length;
        else next.set(k, { totals: t, p: p / faces.length });
      }
    }
    dist = next;
  }
  return [...dist.values()];
}

const typeCache = new Map<string, BattleOutcome[]>();
const poolCache = new Map<string, BattleOutcome[]>();

function typeDist(type: DieType, n: number): BattleOutcome[] {
  const k = `${type}:${n}`;
  let d = typeCache.get(k);
  if (!d) typeCache.set(k, (d = typeDistribution(type, n)));
  return d;
}

/**
 * The exact outcome distribution of rolling this pool, sorted most probable
 * first. Pools are capped at `DICE_PER_TYPE` per type, as the game caps them.
 */
export function battleDistribution(assault: number, skirmish: number, raid: number): BattleOutcome[] {
  const a = Math.min(assault, DICE_PER_TYPE);
  const s = Math.min(skirmish, DICE_PER_TYPE);
  const r = Math.min(raid, DICE_PER_TYPE);
  const key = `${a}:${s}:${r}`;
  const hit = poolCache.get(key);
  if (hit) return hit;

  let dist = typeDist('assault', a);
  for (const other of [typeDist('skirmish', s), typeDist('raid', r)]) {
    const next = new Map<string, BattleOutcome>();
    for (const x of dist) {
      for (const y of other) {
        const t: RollTotals = {
          selfHits: x.totals.selfHits + y.totals.selfHits,
          intercept: x.totals.intercept + y.totals.intercept,
          hits: x.totals.hits + y.totals.hits,
          buildingHits: x.totals.buildingHits + y.totals.buildingHits,
          keys: x.totals.keys + y.totals.keys,
          skirmishBlanks: x.totals.skirmishBlanks + y.totals.skirmishBlanks,
        };
        const k = keyOf(t);
        const row = next.get(k);
        if (row) row.p += x.p * y.p;
        else next.set(k, { totals: t, p: x.p * y.p });
      }
    }
    dist = [...next.values()];
  }

  dist.sort((x, y) => y.p - x.p);
  poolCache.set(key, dist);
  return dist;
}

/**
 * The most probable outcomes covering at least `mass` of the distribution,
 * with probabilities renormalised to sum to 1 — expectation over these is
 * expectation over the whole distribution, truncated where it stops
 * mattering.
 */
export function topMass(dist: BattleOutcome[], mass: number): BattleOutcome[] {
  const out: BattleOutcome[] = [];
  let sum = 0;
  for (const o of dist) {
    out.push(o);
    sum += o.p;
    if (sum >= mass) break;
  }
  return out.map((o) => ({ totals: o.totals, p: o.p / sum }));
}

/** Per-component expectation of the pool, from the exact distribution. */
export function expectedBattle(assault: number, skirmish: number, raid: number): RollTotals {
  const acc = zero();
  for (const { totals, p } of battleDistribution(assault, skirmish, raid)) {
    acc.selfHits += totals.selfHits * p;
    acc.intercept += totals.intercept * p;
    acc.hits += totals.hits * p;
    acc.buildingHits += totals.buildingHits * p;
    acc.keys += totals.keys * p;
    acc.skirmishBlanks += totals.skirmishBlanks * p;
  }
  return acc;
}
