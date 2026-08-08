/**
 * Reconstruct the box's setup-card deck.
 *
 * The printed 12 cards are not in the rulebook, and they are hand-designed for
 * balance. Rather than invent 12 by hand or draw uniformly (which produces
 * lopsided openings), this scores a large pool of legal draws and keeps the
 * most balanced 4 per player count, then prints them as a literal table to
 * paste into setup.ts.
 *
 *   npx tsx tools/pick-setups.ts
 */
import { generateSetup, makeVariant, clusterOf, CLUSTER_COUNT } from '../src/engine';

/** Ring distance between two clusters, 0-3. */
function ringGap(a: number, b: number): number {
  const d = Math.abs(a - b) % CLUSTER_COUNT;
  return Math.min(d, CLUSTER_COUNT - d);
}

/** Lower is better. Penalises unequal starts and players sitting on top of each other. */
function imbalance(players: number, seed: number): number | null {
  const s = generateSetup(players, seed);
  const v = makeVariant(players, seed);

  const slots: number[] = [];
  const distinctTypes: number[] = [];
  const homes: number[] = [];
  for (const st of s.starts) {
    const a = v.systems[st.a];
    const b = v.systems[st.b];
    // Build room is the most durable positional advantage there is.
    slots.push(a.buildingSlots + b.buildingSlots);
    distinctTypes.push(new Set([a.planetType, b.planetType]).size);
    homes.push(clusterOf(st.a));
  }

  const spread = (xs: number[]) => {
    const m = xs.reduce((p, q) => p + q, 0) / xs.length;
    return xs.reduce((p, q) => p + (q - m) ** 2, 0) / xs.length;
  };

  // Everyone should have somewhere to build, and two different resources.
  let score = spread(slots) * 6 + distinctTypes.filter((n) => n < 2).length * 4;

  // Nobody should be crowded: maximise the smallest gap between home clusters.
  let minGap = 9;
  for (let i = 0; i < homes.length; i++) {
    for (let j = i + 1; j < homes.length; j++) minGap = Math.min(minGap, ringGap(homes[i], homes[j]));
  }
  if (minGap === 0) return null; // two players homing in one cluster
  score += (3 - minGap) * 1.5;

  return score;
}

const label: Record<number, string[]> = {
  2: ['Frontiers', 'Mirrors', 'Crossroads', 'Verge'],
  3: ['Triangulum', 'Wheelhouse', 'Divide', 'Reaches'],
  4: ['Quadrants', 'Bastions', 'Sprawl', 'Crown'],
};

const out: string[] = [];
for (const players of [2, 3, 4]) {
  const scored: { seed: number; score: number }[] = [];
  for (let seed = 0; seed < 20000; seed++) {
    const sc = imbalance(players, seed);
    if (sc !== null) scored.push({ seed, score: sc });
  }
  scored.sort((a, b) => a.score - b.score || a.seed - b.seed);

  // Keep the best four that differ in which clusters are out of play, so the
  // deck offers four genuinely different maps rather than four near-copies.
  const kept: { seed: number; score: number }[] = [];
  const usedDead = new Set<string>();
  for (const c of scored) {
    const key = generateSetup(players, c.seed).outOfPlay.join(',');
    if (usedDead.has(key)) continue;
    usedDead.add(key);
    kept.push(c);
    if (kept.length === 4) break;
  }

  console.error(`${players}p best scores: ${kept.map((k) => k.score.toFixed(2)).join(', ')}`);
  out.push(`  ${players}: [`);
  kept.forEach((k, i) => {
    const s = generateSetup(players, k.seed);
    const starts = s.starts
      .map((st) => `{ a: ${st.a}, b: ${st.b}, c: [${st.c.join(', ')}] }`)
      .join(', ');
    out.push(`    // balance ${k.score.toFixed(2)} (from draw ${k.seed})`);
    out.push(
      `    { name: '${label[players][i]}', outOfPlay: [${s.outOfPlay.join(', ')}], starts: [${starts}] },`,
    );
  });
  out.push('  ],');
}
console.log(out.join('\n'));
