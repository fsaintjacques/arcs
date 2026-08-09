/** The exact battle distribution agrees with the dice it summarises. */
import { describe, expect, it } from 'vitest';
import { expectedFace, mulberry32, rollBattle } from '../src/engine';
import { battleDistribution, expectedBattle, topMass } from '../src/agents';

describe('battleDistribution', () => {
  it('sums to probability 1 for every pool shape', () => {
    for (const [a, s, r] of [[0, 0, 0], [1, 0, 0], [3, 2, 1], [6, 6, 6]] as const) {
      const p = battleDistribution(a, s, r).reduce((x, o) => x + o.p, 0);
      expect(p).toBeCloseTo(1, 9);
    }
  });

  it('matches the per-die expected faces exactly', () => {
    const [a, s, r] = [4, 3, 2];
    const exact = expectedBattle(a, s, r);
    const ea = expectedFace('assault');
    const es = expectedFace('skirmish');
    const er = expectedFace('raid');
    expect(exact.hits).toBeCloseTo(a * ea.hits + s * es.hits + r * er.hits, 9);
    expect(exact.selfHits).toBeCloseTo(a * ea.selfHits + s * es.selfHits + r * er.selfHits, 9);
    expect(exact.keys).toBeCloseTo(a * ea.keys + s * es.keys + r * er.keys, 9);
    expect(exact.buildingHits).toBeCloseTo(
      a * ea.buildingHits + s * es.buildingHits + r * er.buildingHits,
      9,
    );
    expect(exact.intercept).toBeCloseTo(a * ea.intercept + s * es.intercept + r * er.intercept, 9);
    // Half the skirmish faces are blank.
    expect(exact.skirmishBlanks).toBeCloseTo(s / 2, 9);
  });

  it('agrees with 100k seeded Monte-Carlo rolls', () => {
    const dice = { assault: 3, skirmish: 2, raid: 2 };
    const rng = mulberry32(77);
    const n = 100_000;
    const acc = { hits: 0, selfHits: 0, keys: 0, buildingHits: 0 };
    for (let i = 0; i < n; i++) {
      const t = rollBattle(dice, rng);
      acc.hits += t.hits;
      acc.selfHits += t.selfHits;
      acc.keys += t.keys;
      acc.buildingHits += t.buildingHits;
    }
    const exact = expectedBattle(dice.assault, dice.skirmish, dice.raid);
    expect(acc.hits / n).toBeCloseTo(exact.hits, 1);
    expect(acc.selfHits / n).toBeCloseTo(exact.selfHits, 1);
    expect(acc.keys / n).toBeCloseTo(exact.keys, 1);
    expect(acc.buildingHits / n).toBeCloseTo(exact.buildingHits, 1);
  });

  it('topMass renormalises and keeps the most probable outcomes', () => {
    const dist = battleDistribution(6, 6, 6);
    const top = topMass(dist, 0.95);
    expect(top.length).toBeLessThan(dist.length);
    expect(top.reduce((x, o) => x + o.p, 0)).toBeCloseTo(1, 9);
    // Sorted most-probable-first, so the head of topMass is the mode.
    expect(top[0].totals).toEqual(dist[0].totals);
  });
});
