/** What does the 1-ply argmax pick at wide nodes, and why do we miss it? */
import { applyAction, controlOf, getPending, makeVariant, type Action, type GameState, type VariantDef } from '../src/engine';
import { defaultWeights, generateCandidates, makeAgent, relativeEvaluate } from '../src/agents';
import { playGame } from '../src/sim/runner';

function argmax(s: GameState, v: VariantDef, player: number, actions: Action[]): Action {
  let best = actions[0];
  let bestValue = -Infinity;
  for (const a of actions) {
    try {
      const value = relativeEvaluate(applyAction(s, v, a), v, player, defaultWeights);
      if (value > bestValue) { bestValue = value; best = a; }
    } catch { /* illegal in world */ }
  }
  return best;
}

const kindCount = new Map<string, number>();
const missKind = new Map<string, number>();
const moveMiss: string[] = [];

for (let g = 0; g < 6; g++) {
  const v = makeVariant(3, g % 6);
  playGame(Array.from({ length: 3 }, () => makeAgent('greedy')), {
    players: 3, seed: 500 + g, setupIndex: g % 6,
    onDecision: (state, player) => {
      const node = getPending(state, v);
      if (node.kind !== 'decision' || node.actions.length <= 12) return;
      const want = argmax(state, v, player, node.actions);
      kindCount.set(want.t, (kindCount.get(want.t) ?? 0) + 1);
      const cands = generateCandidates(state, v, player, node.actions, { max: 12, weights: defaultWeights });
      if (cands.includes(want)) return;
      missKind.set(want.t, (missKind.get(want.t) ?? 0) + 1);
      if (want.t === 'move' && moveMiss.length < 15) {
        const m = want as Extract<Action, { t: 'move' }>;
        const st = state.systems[m.to];
        let rivals = 0;
        for (let p = 0; p < state.players; p++) if (p !== player) rivals += st.fresh[p] + st.damaged[p];
        const fromShips = state.systems[m.from].fresh[player];
        moveMiss.push(
          `to=${m.to}(${v.systems[m.to].kind}) ships=${m.ships}/${fromShips} rivalsAtTo=${rivals} ` +
          `rivalBldgs=${st.buildings.filter((b) => b.player !== player).length} myControlTo=${controlOf(state, m.to) === player} myControlFrom=${controlOf(state, m.from) === player}`,
        );
      }
    },
  });
}
console.log('argmax kinds at wide nodes:', [...kindCount.entries()].sort((a, b) => b[1] - a[1]));
console.log('missed by candidates:', [...missKind.entries()].sort((a, b) => b[1] - a[1]));
console.log('sample missed moves:');
for (const m of moveMiss) console.log(' ', m);

// Appendix: where does the argmax rank within its kind, and how many slots
// does its kind get from the round-robin?
import { generateCandidates as gc } from '../src/agents';
{
  const v = makeVariant(3, 0);
  let report = 0;
  playGame(Array.from({ length: 3 }, () => makeAgent('greedy')), {
    players: 3, seed: 500, setupIndex: 0,
    onDecision: (state, player) => {
      const node = getPending(state, v);
      if (node.kind !== 'decision' || node.actions.length <= 12 || report >= 12) return;
      const want = argmax(state, v, player, node.actions);
      const cands = gc(state, v, player, node.actions, { max: 12, weights: defaultWeights });
      if (cands.includes(want)) return;
      report++;
      const kinds = new Map<string, number>();
      for (const a of node.actions) kinds.set(a.t, (kinds.get(a.t) ?? 0) + 1);
      const slots = new Map<string, number>();
      for (const a of cands) slots.set(a.t, (slots.get(a.t) ?? 0) + 1);
      console.log(
        `MISS kind=${want.t} | node kinds: ${[...kinds.entries()].map(([k, n]) => `${k}×${n}`).join(' ')} | ` +
        `slots: ${[...slots.entries()].map(([k, n]) => `${k}×${n}`).join(' ')}`,
      );
    },
  });
}
