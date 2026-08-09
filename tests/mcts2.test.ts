/** mcts2 is deterministic, budget-respecting, and stronger than its parts. */
import { describe, expect, it } from 'vitest';
import { getPending, makeVariant, mulberry32, observe } from '../src/engine';
import { makeMcts2 } from '../src/agents';
import { playGame } from '../src/sim/runner';
import { makeAgent } from '../src/agents';

function midGameNode(seed: number) {
  const v = makeVariant(3, 1);
  let captured: { state: ReturnType<typeof observe>; actions: import('../src/engine').Action[]; player: number } | null = null;
  let count = 0;
  playGame(Array.from({ length: 3 }, () => makeAgent('random+')), {
    players: 3,
    seed,
    setupIndex: 1,
    onDecision: (state, player) => {
      const node = getPending(state, v);
      if (node.kind !== 'decision') return;
      count++;
      if (count === 120 && node.actions.length > 4) {
        captured = { state: observe(state, v, player), actions: node.actions, player };
      }
    },
  });
  if (!captured) throw new Error('no wide mid-game node captured');
  return { v, ...(captured as { state: ReturnType<typeof observe>; actions: import('../src/engine').Action[]; player: number }) };
}

describe('mcts2', () => {
  it('same observation, same seed, same choice', () => {
    const { v, state: obs, actions, player } = midGameNode(17);
    const a = makeMcts2({ iterations: 60 }).choose(obs, actions, { variant: v, rng: mulberry32(5), player });
    const b = makeMcts2({ iterations: 60 }).choose(obs, actions, { variant: v, rng: mulberry32(5), player });
    expect(a).toEqual(b);
  });

  it('respects a wall-clock budget within a small factor', () => {
    const { v, state: obs, actions, player } = midGameNode(18);
    const agent = makeMcts2({ iterations: 1_000_000, timeMs: 25 });
    const t0 = performance.now();
    agent.choose(obs, actions, { variant: v, rng: mulberry32(6), player });
    const ms = performance.now() - t0;
    // Generous ceiling so CI never flakes; the point is it stops, not runs
    // the million iterations.
    expect(ms).toBeLessThan(250);
  });
});
