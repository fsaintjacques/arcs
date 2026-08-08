/**
 * Headless simulation CLI.
 *
 *   npm run sim -- --agents greedy,greedy,random --games 200
 *   npm run sim -- --agents mcts,greedy --games 50 --seed 7
 *   npm run sim -- --agents greedy,greedy --games 1 --verbose
 *   npm run sim -- --agents mcts,greedy --opts '{"iterations":800}'
 */
import { AMBITIONS, actionCard, ambitionCount, cardName, POWER_THRESHOLD } from '../engine';
import { agentNames, makeAgent } from '../agents';
import { playGame, simulate } from './runner';
import { computeStats, pairedStats } from './stats';

function parseArgs(argv: string[]): Record<string, string> {
  const args: Record<string, string> = {};
  for (let i = 0; i < argv.length; i++) {
    const m = argv[i].match(/^--([a-z-]+)(?:=(.*))?$/i);
    if (!m) continue;
    args[m[1]] = m[2] ?? (argv[i + 1] && !argv[i + 1].startsWith('--') ? argv[++i] : 'true');
  }
  return args;
}

const args = parseArgs(process.argv.slice(2));

if (args.help === 'true') {
  console.log(`Arcs simulation CLI

  --agents     comma-separated agent list, one per seat (default greedy,greedy,greedy)
               available: ${agentNames().join(', ')}
  --games      batch size (default 100)
  --seed       base seed; games are fully reproducible from it (default 1)
  --setup      starting setup index (default 0; each game rotates on from here)
  --opts       JSON passed to every agent, e.g. '{"iterations":800}'
  --no-rotate  keep seats fixed instead of rotating agents through them
  --unpaired   give every game its own deal instead of holding it fixed across
               a block of seatings (much noisier; use only to show the contrast)
  --verbose    play a single game and print a turn log
`);
  process.exit(0);
}

const names = (args.agents ?? 'greedy,greedy,greedy').split(',').map((n) => n.trim());
const games = Number(args.games ?? 100);
const seed = Number(args.seed ?? 1);
const setupIndex = Number(args.setup ?? 0);
const agentOpts = args.opts ? (JSON.parse(args.opts) as Record<string, unknown>) : undefined;

if (names.length < 2 || names.length > 4) {
  console.error(`Arcs is a 2-4 player game; got ${names.length} agents.`);
  process.exit(1);
}

const agents = names.map((n) => makeAgent(n, agentOpts));
const players = agents.length;

if (args.verbose === 'true') {
  const r = playGame(agents, {
    players,
    seed,
    setupIndex,
    onDecision: (state, player, action) => {
      const a = action as { t: string; card?: number; ambition?: string; mode?: string };
      if (!['lead', 'follow', 'declareAmbition', 'passInitiative', 'seize'].includes(a.t)) return;
      const card = a.card !== undefined ? `${cardName(a.card)} (${actionCard(a.card).pips}p)` : '';
      const detail = [a.mode, card, a.ambition ? `→ ${a.ambition}` : ''].filter(Boolean).join(' ');
      console.log(`  ch${state.chapter} r${state.stats.rounds + 1}  P${player} ${a.t} ${detail}`);
    },
  });

  console.log(`\nseed ${r.seed} — ${names.join(' vs ')}`);
  r.power.forEach((power, seat) => {
    const ps = r.state.playerStates[seat];
    const counts = AMBITIONS.map((a) => `${a.slice(0, 3)} ${ambitionCount(ps, a)}`).join('  ');
    console.log(
      `  P${seat} ${names[seat].padEnd(11)} power ${String(power).padStart(3)}   ${counts}   guild ${ps.guildCards.length}`,
    );
  });
  const threshold = POWER_THRESHOLD[players];
  const how = r.power[r.winner] >= threshold ? `reached ${threshold} Power` : 'led after chapter 5';
  console.log(`  winner P${r.winner} (${names[r.winner]}) — ${how}`);
  console.log(
    `  ${r.chapters} chapters, ${r.state.stats.rounds} rounds, ${r.state.stats.battles} battles, ${r.state.stats.ambitionsDeclared} ambitions declared`,
  );
  process.exit(0);
}

const t0 = performance.now();
const sim = simulate(agents, {
  players,
  games,
  seed,
  setupIndex,
  rotateSeats: args['no-rotate'] !== 'true',
  paired: args.unpaired !== 'true',
});
const dt = performance.now() - t0;
const played = sim.games.length;
const st = computeStats(sim, names, dt / played);

console.log(
  `\n=== ${players} players — ${played} games, seed ${seed} (${(dt / 1000).toFixed(1)}s, ${(dt / played).toFixed(0)}ms/game)`,
);
if (sim.paired) {
  console.log(
    `paired: ${played / sim.blockSize} deals × ${sim.blockSize} seatings — every agent plays each deal from every seat`,
  );
}
console.log(
  `chapters ${st.meanChapters.toFixed(2)}   rounds ${st.meanRounds.toFixed(1)}   battles ${st.meanBattles.toFixed(1)}   ambitions declared ${st.meanDeclared.toFixed(1)}`,
);

console.log(
  `\n${'agent'.padEnd(12)}${'games'.padStart(6)}${'win%'.padStart(15)}${'power'.padStart(16)}${'rank'.padStart(6)}${'cities'.padStart(7)}${'ships'.padStart(7)}${'guild'.padStart(7)}`,
);
for (const a of st.agents) {
  console.log(
    a.name.padEnd(12) +
      String(a.games).padStart(6) +
      `${(a.winRate * 100).toFixed(1)}±${(a.winRateCi * 100).toFixed(1)}`.padStart(15) +
      `${a.meanPower.toFixed(1)}±${a.stdPower.toFixed(1)}`.padStart(16) +
      a.meanRank.toFixed(2).padStart(6) +
      a.meanCities.toFixed(1).padStart(7) +
      a.meanShips.toFixed(1).padStart(7) +
      a.meanGuildCards.toFixed(1).padStart(7),
  );
}

const pair = pairedStats(sim, names, 0, 1);
if (pair) {
  console.log(`\npaired head-to-head: ${pair.a} vs ${pair.b}`);
  const sign = pair.diff >= 0 ? '+' : '';
  console.log(
    `  ${sign}${pair.diff.toFixed(1)}±${pair.ci.toFixed(1)} pts of win share to ${pair.a}, over ${pair.blocks} deals`,
  );
  console.log(
    `  deals won by ${pair.a}: ${pair.aBetter}   by ${pair.b}: ${pair.bBetter}   split: ${pair.tied}`,
  );
  console.log(
    pair.separated
      ? `  the interval excludes zero — a real difference at this sample size`
      : `  the interval covers zero — NOT separated at this sample size`,
  );
}

console.log(`\nambition counts at game end`);
console.log(`${'agent'.padEnd(12)}${AMBITIONS.map((a) => a.padStart(9)).join('')}`);
for (const a of st.agents) {
  console.log(
    a.name.padEnd(12) + AMBITIONS.map((amb) => a.meanAmbition[amb].toFixed(1).padStart(9)).join(''),
  );
}

const maxCount = Math.max(...st.histogram.map((h) => h.count), 1);
console.log(`\nfinal Power distribution (all seats)`);
for (const h of st.histogram) {
  const bar = '█'.repeat(Math.max(h.count > 0 ? 1 : 0, Math.round((h.count / maxCount) * 40)));
  console.log(`  ${String(h.start).padStart(3)}–${String(h.start + 4).padEnd(3)} ${bar} ${h.count}`);
}
