/** Every registered agent plays a legal, terminating game and beats the floor. */
import { describe, expect, it } from 'vitest';
import { agentNames, makeAgent } from '../src/agents';
import { playGame, simulate } from '../src/sim/runner';
import { computeStats } from '../src/sim/stats';
import { narrow } from '../src/agents';
import { getPending, makeVariant } from '../src/engine';

/** Keep the search agents cheap so the suite stays fast. */
const FAST: Record<string, Record<string, unknown>> = {
  mc: { rollouts: 2, depth: 12, maxActions: 4 },
  mcts: { iterations: 20, rolloutDepth: 8, maxActions: 4, maxDepth: 4 },
  'mcts-fast': { iterations: 20, rolloutDepth: 8, maxActions: 4, maxDepth: 4 },
  'mcts-c': { iterations: 20, rolloutDepth: 8, maxActions: 4, maxDepth: 4 },
  'mcts-t1': { iterations: 20, rolloutDepth: 8, maxActions: 4, maxDepth: 4 },
  'anchor-mcts300-v0': { iterations: 20, rolloutDepth: 8, maxActions: 4, maxDepth: 4 },
  'anchor-mcts-c-v1': { iterations: 20, rolloutDepth: 8, maxActions: 4, maxDepth: 4 },
};

describe('agents', () => {
  for (const name of agentNames()) {
    it(`${name} plays a complete 3-player game`, () => {
      const agents = Array.from({ length: 3 }, () => makeAgent(name, FAST[name]));
      const r = playGame(agents, { players: 3, seed: 8, setupIndex: 1 });
      expect(r.state.phase).toBe('over');
      expect(r.power.every((p) => p >= 0)).toBe(true);
      expect(r.ranks.slice().sort()).toEqual([0, 1, 2]);
    });
  }

  it('only ever plays an action that was on offer', () => {
    for (const name of agentNames()) {
      const agents = Array.from({ length: 2 }, () => makeAgent(name, FAST[name]));
      playGame(agents, {
        players: 2,
        seed: 15,
        setupIndex: 0,
        onDecision: (state, player, action) => {
          const node = getPending(state, makeVariant(2, 0));
          expect(node.kind).toBe('decision');
          if (node.kind !== 'decision') return;
          expect(node.player).toBe(player);
          const offered = node.actions.map((a) => JSON.stringify(a));
          expect(offered).toContain(JSON.stringify(action));
        },
      });
    }
  });

  it('rejects an unknown agent by name', () => {
    expect(() => makeAgent('nope')).toThrow(/unknown agent/);
  });
});

describe('narrow', () => {
  const actions = [
    { t: 'move', i: 1 },
    { t: 'move', i: 2 },
    { t: 'move', i: 3 },
    { t: 'move', i: 4 },
    { t: 'build', i: 5 },
    { t: 'endTurn', i: 6 },
  ];

  it('is a no-op when the list already fits', () => {
    expect(narrow(actions, 10)).toBe(actions);
  });

  it('keeps at least one of every action kind', () => {
    const kept = narrow(actions, 3);
    expect(kept).toHaveLength(3);
    expect(new Set(kept.map((a) => a.t))).toEqual(new Set(['move', 'build', 'endTurn']));
  });

  it('is deterministic, so a search node keeps the same children', () => {
    expect(narrow(actions, 4)).toEqual(narrow(actions, 4));
  });
});

describe('strength ladder', () => {
  // Small batches: this is a smoke test that the ladder points the right way,
  // not a benchmark. The README numbers come from much larger runs.
  it('greedy beats random decisively', () => {
    const agents = [makeAgent('greedy'), makeAgent('random')];
    const sim = simulate(agents, { players: 2, games: 12, seed: 5 });
    const st = computeStats(sim, ['greedy', 'random'], 0);
    expect(st.agents[0].winRate).toBeGreaterThan(0.8);
  });

  it('random+ beats random', () => {
    const agents = [makeAgent('random+'), makeAgent('random')];
    const sim = simulate(agents, { players: 2, games: 20, seed: 6 });
    const st = computeStats(sim, ['random+', 'random'], 0);
    expect(st.agents[0].winRate).toBeGreaterThan(0.5);
  });

  it('identical agents split evenly once seats are permuted', () => {
    const agents = [makeAgent('greedy'), makeAgent('greedy')];
    const sim = simulate(agents, { players: 2, games: 24, seed: 9 });
    const st = computeStats(sim, ['a', 'b'], 0);
    expect(Math.abs(st.agents[0].winRate - st.agents[1].winRate)).toBeLessThan(0.35);
  });
});
