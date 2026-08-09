/**
 * Cross-entropy weight tuning for eval.ts, on the paired harness.
 *
 * Every generation samples a population of weight vectors (log-space, so
 * weights stay positive), scores each as a greedy carrier against a fixed
 * opponent on the *same* deal blocks — common random numbers, so the ranking
 * within a generation is paired — and refits a diagonal Gaussian on the
 * elites. CEM has no learning rate to babysit and its population step is
 * embarrassingly parallel, which is why it won over SPSA here.
 *
 *   npx tsx tools/tune-cem.ts --gens 30 --pop 24 --elite 6 --games 96 --seed 1
 *   npx tsx tools/tune-cem.ts --resume tools/out/cem.json
 *
 * The winner is only a candidate: it still has to pass the gauntlet before
 * anything ships as defaultWeights (docs/GAUNTLET.md).
 */
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import { mulberry32 } from '../src/engine';
import { defaultWeights, type Weights } from '../src/agents';
import { simulateParallel } from '../src/sim/parallel';
import { pairedStats } from '../src/sim/stats';

const args = Object.fromEntries(
  process.argv.slice(2).flatMap((a, i, all) => (a.startsWith('--') ? [[a.slice(2), all[i + 1]]] : [])),
);

const POP = Number(args.pop ?? 24);
const ELITE = Number(args.elite ?? 6);
const GENS = Number(args.gens ?? 30);
const GAMES = Number(args.games ?? 96);
const SEED = Number(args.seed ?? 1);
const OPPONENT = args.opponent ?? 'greedy';
const OUT = args.out ?? 'tools/out/cem.json';
const SIGMA0 = Number(args.sigma ?? 0.3);
const SIGMA_FLOOR = 0.05;

/** `power` is the unit everything else is measured in; tuning it is scale. */
const RESOURCES = ['material', 'fuel', 'weapon', 'relic', 'psionic'] as const;
const SCALARS = (Object.keys(defaultWeights) as (keyof Weights)[]).filter(
  (k) => k !== 'power' && k !== 'resourceValue',
);
const DIM = SCALARS.length + RESOURCES.length;

function toVector(w: Weights): number[] {
  const v = SCALARS.map((k) => Math.log(Math.max(1e-3, w[k] as number)));
  for (const r of RESOURCES) v.push(Math.log(Math.max(1e-3, w.resourceValue[r])));
  return v;
}

function toWeights(v: number[]): Weights {
  const w: Weights = { ...defaultWeights, resourceValue: { ...defaultWeights.resourceValue } };
  SCALARS.forEach((k, i) => ((w[k] as number) = Math.exp(v[i])));
  RESOURCES.forEach((r, i) => (w.resourceValue[r] = Math.exp(v[SCALARS.length + i])));
  return w;
}

/** Paired win-share diff of a candidate weight vector vs the fixed opponent. */
async function fitness(v: number[], genSeed: number): Promise<number> {
  const weights = toWeights(v);
  const sim = await simulateParallel(
    [{ name: 'greedy', opts: { weights } }, { name: OPPONENT }, { name: OPPONENT }],
    { players: 3, games: GAMES, seed: genSeed, workers: 1 },
  );
  const pair = pairedStats(sim, ['candidate', OPPONENT, OPPONENT], 0, 1);
  return pair ? pair.diff : -Infinity;
}

/** Run up to `limit` promises at a time. */
async function pool<T, R>(items: T[], limit: number, run: (x: T, i: number) => Promise<R>): Promise<R[]> {
  const out: R[] = Array(items.length);
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(limit, items.length) }, async () => {
      for (;;) {
        const i = next++;
        if (i >= items.length) return;
        out[i] = await run(items[i], i);
      }
    }),
  );
  return out;
}

interface Checkpoint {
  gen: number;
  mean: number[];
  sigma: number[];
  bestFitness: number;
  bestVector: number[];
}

let start: Checkpoint = {
  gen: 0,
  mean: toVector(defaultWeights),
  sigma: Array(DIM).fill(SIGMA0),
  bestFitness: -Infinity,
  bestVector: toVector(defaultWeights),
};
if (args.resume) {
  start = JSON.parse(readFileSync(args.resume, 'utf8')) as Checkpoint;
  console.log(`resuming from ${args.resume} at generation ${start.gen}`);
}

const gauss = (rng: () => number) => {
  const u = Math.max(rng(), 1e-12);
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * rng());
};

console.log(
  `CEM: pop ${POP}, elite ${ELITE}, ${GENS} generations, ${GAMES} games/member vs ${OPPONENT}, seed ${SEED}, ${DIM} dims`,
);

mkdirSync('tools/out', { recursive: true });
const { mean, sigma } = start;
let { bestFitness, bestVector } = start;

for (let gen = start.gen; gen < GENS; gen++) {
  const rng = mulberry32((SEED ^ (gen * 0x9e3779b9)) >>> 0);
  // One deal set per generation: every member meets the same games.
  const genSeed = (SEED + 7919 * (gen + 1)) >>> 0;

  const members = Array.from({ length: POP }, (_, m) =>
    // The incumbent mean rides along un-perturbed so a generation can never
    // move to a population that lost to its own center.
    m === 0 ? [...mean] : mean.map((mu, d) => mu + sigma[d] * gauss(rng)),
  );

  const t0 = performance.now();
  const scores = await pool(members, Math.max(1, os.availableParallelism() - 1), (v) =>
    fitness(v, genSeed),
  );
  const ranked = members
    .map((v, i) => ({ v, f: scores[i] }))
    .sort((a, b) => b.f - a.f);
  const elites = ranked.slice(0, ELITE);

  for (let d = 0; d < DIM; d++) {
    const mu = elites.reduce((s, e) => s + e.v[d], 0) / ELITE;
    const sd = Math.sqrt(elites.reduce((s, e) => s + (e.v[d] - mu) ** 2, 0) / ELITE);
    mean[d] = mu;
    sigma[d] = Math.max(SIGMA_FLOOR, sd);
  }
  if (elites[0].f > bestFitness) {
    bestFitness = elites[0].f;
    bestVector = [...elites[0].v];
  }

  const secs = ((performance.now() - t0) / 1000).toFixed(0);
  console.log(
    `gen ${String(gen + 1).padStart(2)}: elite ${elites.map((e) => e.f.toFixed(1)).join(' ')} ` +
      `| mean-fit ${(scores.reduce((a, b) => a + b, 0) / POP).toFixed(1)} | ${secs}s`,
  );

  writeFileSync(
    OUT,
    JSON.stringify({ gen: gen + 1, mean, sigma, bestFitness, bestVector } satisfies Checkpoint),
  );
}

const tuned = toWeights(mean);
console.log(`\nbest single member: ${bestFitness.toFixed(1)} pts`);
console.log(`elite-mean weights (the candidate — gauntlet it before promoting):\n`);
console.log(JSON.stringify(tuned, null, 2));
