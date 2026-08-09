/** encodeAction keys are stable, distinct, and cheaper than JSON. */
import { describe, expect, it } from 'vitest';
import { encodeAction, getPending, makeVariant } from '../src/engine';
import { makeAgent } from '../src/agents';
import { playGame } from '../src/sim/runner';

describe('encodeAction', () => {
  it('never collides where JSON.stringify distinguishes, across full games', () => {
    // Sweep every decision node of a few games and assert the encoding is
    // injective within each offered action list — the property node keys need.
    for (const players of [2, 3, 4]) {
      const v = makeVariant(players, 1);
      const agents = Array.from({ length: players }, () => makeAgent('random+'));
      playGame(agents, {
        players,
        seed: 33 + players,
        setupIndex: 1,
        onDecision: (state) => {
          const node = getPending(state, v);
          if (node.kind !== 'decision') return;
          const keys = new Set(node.actions.map(encodeAction));
          expect(keys.size).toBe(new Set(node.actions.map((a) => JSON.stringify(a))).size);
        },
      });
    }
  });

  it('distinguishes the cases JSON key order or emptiness could blur', () => {
    expect(encodeAction({ t: 'cardPrelude', card: 16 })).not.toBe(
      encodeAction({ t: 'cardPrelude', card: 16, cards: [] }),
    );
    expect(encodeAction({ t: 'repair', system: 3, building: null })).not.toBe(
      encodeAction({ t: 'repair', system: 3, building: 0 }),
    );
    expect(encodeAction({ t: 'peekTarget', target: null })).not.toBe(
      encodeAction({ t: 'peekTarget', target: 0 }),
    );
    expect(encodeAction({ t: 'assignHit', target: 'ship', fresh: true })).not.toBe(
      encodeAction({ t: 'assignHit', target: 'building', building: 1 }),
    );
  });
});
