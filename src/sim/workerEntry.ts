/**
 * Thread entry for the parallel runner. Rebuilds the agents from their specs
 * and runs the ordinary serial loop over its slice of block indices — the
 * whole point is that no simulation logic lives here.
 */
import { parentPort, workerData } from 'node:worker_threads';
import { makeAgent, type Agent } from '../agents';
import { simulate, type SimOptions } from './runner';
import type { AgentSpec } from './parallel';

const { specs, opts, blocks, timeAgentIndex } = workerData as {
  specs: AgentSpec[];
  opts: Omit<SimOptions, 'onGame' | 'blocks'>;
  blocks: number[];
  timeAgentIndex: number | null;
};

const sink = { ms: 0, decisions: 0 };
const agents = specs.map((spec, index) => {
  const agent = makeAgent(spec.name, spec.opts);
  if (index !== timeAgentIndex) return agent;
  const timed: Agent = {
    name: agent.name,
    choose(obs, actions, ctx) {
      const t0 = performance.now();
      const action = agent.choose(obs, actions, ctx);
      sink.ms += performance.now() - t0;
      sink.decisions++;
      return action;
    },
  };
  return timed;
});

const sim = simulate(agents, {
  ...opts,
  blocks,
  onGame: () => parentPort!.postMessage({ kind: 'progress' }),
});

parentPort!.postMessage({
  kind: 'done',
  games: sim.games,
  timing: timeAgentIndex === null ? null : sink,
});
