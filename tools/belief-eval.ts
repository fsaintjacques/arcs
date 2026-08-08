/**
 * Offline accuracy of the belief model — no games played against anything.
 *
 * At many points in real games, determinize both ways and score the sampled
 * worlds against the hand that was actually held. Two metrics:
 *
 *   recall  — fraction of the true hand that the sampled world got right
 *   impossible — cards dealt that the observer had already watched be played
 *
 * The second should be 0 after the `revealed` fix; it was 1.56 per world.
 */
import {
  applyActionMut,
  computeBelief,
  determinize,
  getPending,
  makeVariant,
  marginals,
  mulberry32,
  newGame,
  observe,
  resolveChanceMut,
} from '../src/engine';
import { makeAgent } from '../src/agents';

const args = Object.fromEntries(
  process.argv.slice(2).flatMap((a, i, all) => (a.startsWith('--') ? [[a.slice(2), all[i + 1]]] : [])),
);
const games = Number(args.games ?? 60);
const worldsPer = 8;

const score = {
  belief: { hit: 0, total: 0, impossible: 0, logLik: 0, n: 0 },
  uniform: { hit: 0, total: 0, impossible: 0, logLik: 0, n: 0 },
};

for (let g = 0; g < games; g++) {
  const v = makeVariant(3, g % 6);
  const rng = mulberry32(g + 1);
  const s = newGame(v, rng, g % 6);
  const agents = [makeAgent('greedy'), makeAgent('greedy'), makeAgent('greedy')];
  const ctxs = agents.map((_, player) => ({ variant: v, rng: mulberry32(g * 31 + player), player }));

  for (let i = 0; i < 40000; i++) {
    const node = getPending(s, v);
    if (node.kind === 'over') break;
    if (node.kind === 'chance') { resolveChanceMut(s, v, rng); continue; }

    // Only score where there is something to infer from.
    if (s.declines.length >= 2 && i % 5 === 0) {
      const me = node.player;
      const obs = observe(s, v, me);
      const truth = s.playerStates.map((p) => new Set(p.hand));

      for (const mode of ['belief', 'uniform'] as const) {
        const acc = score[mode];
        for (let w = 0; w < worldsPer; w++) {
          const world = determinize(obs, v, mulberry32(i * 7919 + w * 104729 + g), {
            belief: mode === 'belief',
          });
          for (let p = 0; p < 3; p++) {
            if (p === me) continue;
            for (const c of world.playerStates[p].hand) {
              acc.total++;
              if (truth[p].has(c)) acc.hit++;
              if (s.revealed.includes(c)) acc.impossible++;
            }
          }
        }
        // Marginal log-likelihood of the true hands under this model.
        const belief = computeBelief(s, v, me);
        if (mode === 'uniform') for (const row of belief.weight) row.fill(1);
        const known = new Set([...s.playerStates[me].hand, ...s.revealed,
          ...s.round.played.filter((c) => !c.faceDown || c.player === me).map((c) => c.card)]);
        const pool = v.actionDeck.map((c) => c.id).filter((c) => !known.has(c));
        for (let p = 0; p < 3; p++) {
          if (p === me) continue;
          const m = marginals(belief, pool, p, s.playerStates[p].hand.length);
          for (const c of s.playerStates[p].hand) {
            const q = m.get(c);
            if (q && q > 0) { acc.logLik += Math.log(q); acc.n++; }
          }
        }
      }
    }
    applyActionMut(s, v, agents[node.player].choose(observe(s, v, node.player), node.actions, ctxs[node.player]));
  }
}

for (const mode of ['uniform', 'belief'] as const) {
  const a = score[mode];
  console.log(
    `${mode.padEnd(8)} recall ${((100 * a.hit) / Math.max(1, a.total)).toFixed(2)}%   ` +
    `impossible cards ${a.impossible}   ` +
    `mean log-lik ${(a.logLik / Math.max(1, a.n)).toFixed(4)}`,
  );
}
const lift = (score.belief.hit / score.belief.total) / (score.uniform.hit / score.uniform.total);
console.log(`\nrecall lift: ${((lift - 1) * 100).toFixed(1)}%`);
