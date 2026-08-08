/**
 * Measure the one free parameter in the belief model.
 *
 * DECLINE_ODDS is P(a follower does not Surpass | they held a legal Surpass).
 * Guessing it would make the belief model as arbitrary as the hand-set
 * evaluation weights it is meant to improve on, so it is fitted from play.
 *
 *   npx tsx tools/calibrate.ts [--agents greedy,greedy,greedy] [--games 200]
 */
import {
  actionCard,
  applyActionMut,
  getPending,
  makeVariant,
  mulberry32,
  newGame,
  observe,
  resolveChanceMut,
} from '../src/engine';
import { makeAgent } from '../src/agents';

const args = Object.fromEntries(
  process.argv.slice(2).flatMap((a, i, all) => (a.startsWith('--') ? [[a.slice(2), all[i + 1]]] : [])),
);
const names = (args.agents ?? 'greedy,greedy,greedy').split(',');
const games = Number(args.games ?? 200);

let couldSurpass = 0;
let declined = 0;

for (let g = 0; g < games; g++) {
  const players = names.length;
  const v = makeVariant(players, g % 6);
  const rng = mulberry32(g + 1);
  const s = newGame(v, rng, g % 6);
  const agents = names.map((n) => makeAgent(n));
  const ctxs = agents.map((_, player) => ({ variant: v, rng: mulberry32(g * 31 + player), player }));

  for (let i = 0; i < 40000; i++) {
    const node = getPending(s, v);
    if (node.kind === 'over') break;
    if (node.kind === 'chance') { resolveChanceMut(s, v, rng); continue; }

    // A follow decision with a legal Surpass on offer is one observation.
    const isFollow = s.phase === 'play' && s.round.lead !== null && s.round.turnIndex > 0;
    const surpassAvailable = isFollow && node.actions.some((a) => a.t === 'follow' && a.mode === 'surpass');

    const obs = observe(s, v, node.player);
    const action = agents[node.player].choose(obs, node.actions, ctxs[node.player]);

    if (surpassAvailable) {
      couldSurpass++;
      if (!(action.t === 'follow' && action.mode === 'surpass')) declined++;
    }
    applyActionMut(s, v, action);
  }
}

const p = declined / Math.max(1, couldSurpass);
console.log(`agents: ${names.join(', ')}   games: ${games}`);
console.log(`follows with a legal Surpass available: ${couldSurpass}`);
console.log(`  ... that declined it: ${declined}`);
console.log(`\nDECLINE_ODDS = ${p.toFixed(3)}`);
console.log(
  p > 0.9
    ? '  (near 1: declining says almost nothing — the signal is weak)'
    : p < 0.05
      ? '  (near 0: declining is near-proof they held nothing)'
      : '  (soft evidence, as expected)',
);
