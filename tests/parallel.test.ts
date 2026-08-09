/** The worker pool changes throughput, never results. */
import { describe, expect, it } from 'vitest';
import { makeAgent } from '../src/agents';
import { simulateParallel } from '../src/sim/parallel';
import { simulate } from '../src/sim/runner';
import { computeStats, pairedStats } from '../src/sim/stats';

const NAMES = ['random+', 'random'];
const OPTS = { players: 2, games: 8, seed: 5 };

describe('simulateParallel', () => {
  it('reproduces the serial run exactly, whatever the worker count', async () => {
    const serial = simulate(NAMES.map((n) => makeAgent(n)), OPTS);
    const specs = NAMES.map((name) => ({ name }));

    for (const workers of [1, 3]) {
      const parallel = await simulateParallel(specs, { ...OPTS, workers });
      expect(parallel.games.length).toBe(serial.games.length);
      expect(parallel.blockSize).toBe(serial.blockSize);

      // Same stats to the last digit: same seeds, seatings and outcomes.
      const a = computeStats(serial, NAMES, 0);
      const b = computeStats(parallel, NAMES, 0);
      expect(JSON.stringify(b)).toBe(JSON.stringify(a));
      expect(pairedStats(parallel, NAMES)).toEqual(pairedStats(serial, NAMES));
    }
  }, 60_000);

  it('never splits a block across workers', () => {
    // A block is one deal played from every seating; the runner's `blocks`
    // option is the contract that makes distributing them safe.
    const agents = NAMES.map((n) => makeAgent(n));
    const whole = simulate(agents, OPTS);
    const partial = simulate(agents, { ...OPTS, blocks: [1, 3] });

    const wanted = whole.games.filter((g) => g.block === 1 || g.block === 3);
    expect(partial.games.map((g) => g.result.seed)).toEqual(wanted.map((g) => g.result.seed));
    expect(partial.games.map((g) => g.result.winner)).toEqual(wanted.map((g) => g.result.winner));
    expect(partial.games.map((g) => g.seating)).toEqual(wanted.map((g) => g.seating));
  });

  it('reports in-worker timing for the requested agent', async () => {
    const specs = NAMES.map((name) => ({ name }));
    const sim = await simulateParallel(specs, { ...OPTS, games: 4, workers: 2, timeAgentIndex: 0 });
    expect(sim.timing).toBeDefined();
    expect(sim.timing!.decisions).toBeGreaterThan(0);
    expect(sim.timing!.ms).toBeGreaterThanOrEqual(0);
  }, 60_000);
});
