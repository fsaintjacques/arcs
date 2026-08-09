/** Count how often the once-never-taken abilities are offered and taken (FINDINGS.md). */
import { COURT_DECK, getPending, makeVariant, type Action } from '../src/engine';
import { makeAgent } from '../src/agents';
import { playGame } from '../src/sim/runner';

const farseers = COURT_DECK.find((c) => c.name === 'Farseers')!.id;
const counts: Record<string, { offered: number; taken: number }> = {
  recycle: { offered: 0, taken: 0 },
  union: { offered: 0, taken: 0 },
  execute: { offered: 0, taken: 0 },
};
type A = Action & { card?: number; cards?: number[]; played?: number; name?: string };
const kind = (a: A): string | null =>
  a.t === 'cardPrelude' && a.card === farseers && a.cards !== undefined
    ? 'recycle'
    : a.t === 'cardPrelude' && a.played !== undefined
      ? 'union'
      : a.t === 'cardAction' && a.name === 'execute'
        ? 'execute'
        : null;

for (let g = 0; g < 40; g++) {
  const setupIndex = g % 6;
  const v = makeVariant(3, setupIndex);
  const agents = Array.from({ length: 3 }, () => makeAgent('greedy'));
  playGame(agents, {
    players: 3,
    seed: 1000 + g,
    setupIndex,
    onDecision: (state, _player, action) => {
      const node = getPending(state, v);
      if (node.kind !== 'decision') return;
      const offered = new Set(node.actions.map((a) => kind(a as A)).filter(Boolean));
      for (const k of offered) counts[k as string].offered++;
      const took = kind(action as A);
      if (took) counts[took].taken++;
    },
  });
}
for (const [k, c] of Object.entries(counts)) {
  console.log(`${k}: offered at ${c.offered} nodes, taken ${c.taken}`);
}
