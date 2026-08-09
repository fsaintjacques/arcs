/**
 * Run a candidate through the strength gauntlet and print a ledger row for
 * docs/GAUNTLET.md.
 *
 *   npx tsx tools/gauntlet.ts --candidate mcts --games 240 --seed 1
 *   npx tsx tools/gauntlet.ts --candidate greedy --opts '{"samples":5}' --anchors anchor-greedy-v0
 */
import { execSync } from 'node:child_process';
import { anchorLadder } from '../src/agents';
import { runGauntlet, type AgentSpec } from '../src/sim/gauntlet';

const args = Object.fromEntries(
  process.argv.slice(2).flatMap((a, i, all) => (a.startsWith('--') ? [[a.slice(2), all[i + 1]]] : [])),
);

const candidate: AgentSpec = {
  name: args.candidate ?? 'mcts',
  opts: args.opts ? (JSON.parse(args.opts) as Record<string, unknown>) : undefined,
};
const anchors: AgentSpec[] = (args.anchors ? args.anchors.split(',') : anchorLadder).map(
  (name) => ({ name: name.trim() }),
);
const games = Number(args.games ?? 240);
const seed = Number(args.seed ?? 1);
const budgetMs = Number(args['budget-ms'] ?? 30);

console.log(`gauntlet: ${candidate.name} vs [${anchors.map((a) => a.name).join(', ')}]`);
console.log(`${games} games per anchor, seed ${seed}, budget ${budgetMs}ms/decision\n`);

const t0 = performance.now();
const report = runGauntlet(candidate, anchors, {
  gamesPerAnchor: games,
  seed,
  budgetMs,
  onProgress: (anchor, done, total) => {
    if (done % 30 === 0) console.log(`  ${anchor}: ${done}/${total}`);
  },
});
const minutes = (performance.now() - t0) / 60_000;

console.log(`\n${'anchor'.padEnd(20)}${'games'.padStart(6)}${'diff'.padStart(14)}${'sep'.padStart(5)}${'win%'.padStart(7)}${'ms/dec'.padStart(8)}`);
for (const row of report.rows) {
  const sign = row.pair.diff >= 0 ? '+' : '';
  console.log(
    row.anchor.padEnd(20) +
      String(row.games).padStart(6) +
      `${sign}${row.pair.diff.toFixed(1)}±${row.pair.ci.toFixed(1)}`.padStart(14) +
      (row.pair.separated ? 'yes' : 'no').padStart(5) +
      (row.winRate * 100).toFixed(1).padStart(7) +
      row.msPerDecision.toFixed(1).padStart(8),
  );
}
console.log(`\n${report.passed ? 'PASSED' : 'not passed'} (budget ${report.budgetOk ? 'ok' : 'EXCEEDED'}), ${minutes.toFixed(1)} min`);

let commit = 'unknown';
try {
  commit = execSync('git rev-parse --short HEAD', { encoding: 'utf8' }).trim();
} catch {
  // outside a git checkout; leave 'unknown'
}
const date = new Date().toISOString().slice(0, 10);
console.log(`\nledger rows for docs/GAUNTLET.md:`);
for (const row of report.rows) {
  const sign = row.pair.diff >= 0 ? '+' : '';
  console.log(
    `| ${date} | ${candidate.name}@${commit} | ${row.anchor} | ${row.games} | ` +
      `${sign}${row.pair.diff.toFixed(1)}±${row.pair.ci.toFixed(1)} | ` +
      `${row.pair.separated ? 'yes' : 'no'} | ${row.msPerDecision.toFixed(1)} | seed ${seed} |`,
  );
}
