/** The gauntlet points the right way, and terminal values match the table. */
import { describe, expect, it } from 'vitest';
import { makeVariant, mulberry32, newGame, standings } from '../src/engine';
import { makeAgent, terminalVector } from '../src/agents';
import { playGame } from '../src/sim/runner';
import { runGauntlet } from '../src/sim/gauntlet';

describe('terminalVector', () => {
  it('breaks Power ties by turn order from initiative, like the table', () => {
    const v = makeVariant(3, 0);
    const s = newGame(v, mulberry32(1), 0);
    s.playerStates[0].power = 10;
    s.playerStates[1].power = 10;
    s.playerStates[2].power = 4;

    s.initiative = 1; // turn order 1,2,0 — the tie goes to player 1
    expect(terminalVector(s)).toEqual([0, 1, 0]);
    s.initiative = 0; // turn order 0,1,2 — the tie goes to player 0
    expect(terminalVector(s)).toEqual([1, 0, 0]);
    s.initiative = 2; // turn order 2,0,1 — still player 0, ahead of 1
    expect(terminalVector(s)).toEqual([1, 0, 0]);
  });

  it('agrees with the standings of a finished game', () => {
    const agents = Array.from({ length: 3 }, () => makeAgent('random+'));
    const r = playGame(agents, { players: 3, seed: 4 });
    const vec = terminalVector(r.state);
    expect(vec[r.winner]).toBe(1);
    expect(vec.reduce((a, b) => a + b, 0)).toBe(1);
    expect(r.winner).toBe(standings(r.state)[0].player);
  });
});

describe('runGauntlet', () => {
  // One paired block (6 games) per anchor: a smoke test of the plumbing and
  // the sign, not a strength measurement.
  it('reads a decisive matchup the right way', () => {
    const report = runGauntlet({ name: 'greedy' }, [{ name: 'random' }], {
      gamesPerAnchor: 6,
      seed: 3,
    });
    expect(report.rows).toHaveLength(1);
    expect(report.rows[0].games).toBe(6);
    expect(report.rows[0].pair.diff).toBeGreaterThan(0);
    expect(report.rows[0].winRate).toBeGreaterThan(0.5);
    expect(report.rows[0].msPerDecision).toBeGreaterThan(0);
    expect(report.passed).toBe(true);
  });

  it('fails a candidate that blows the thinking budget', () => {
    const report = runGauntlet({ name: 'greedy' }, [{ name: 'random' }], {
      gamesPerAnchor: 6,
      seed: 3,
      budgetMs: 0,
    });
    expect(report.rows[0].pair.diff).toBeGreaterThan(0);
    expect(report.budgetOk).toBe(false);
    expect(report.passed).toBe(false);
  });
});
