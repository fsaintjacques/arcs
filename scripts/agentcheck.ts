import { playGame } from '../src/sim/runner';
import { makeAgent } from '../src/agents';
for (const name of ['random', 'random+', 'greedy', 'mc', 'mcts-fast']) {
  const t0 = performance.now();
  try {
    const agents = Array.from({ length: 3 }, () => makeAgent(name));
    const r = playGame(agents, { players: 3, seed: 42, setupIndex: 0 });
    console.log(`${name.padEnd(11)} ok  power=[${r.power}] chapters=${r.chapters} decisions=${r.decisions} ${(performance.now()-t0).toFixed(0)}ms`);
  } catch (e) {
    console.log(`${name.padEnd(11)} FAIL ${(e as Error).message}`);
  }
}
