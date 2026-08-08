/// <reference lib="webworker" />
/**
 * Runs simulation batches off the main thread so the UI stays responsive
 * while a few hundred games play out.
 */
import { makeAgent } from '../agents';
import { playGame } from '../sim/runner';
import { computeStats } from '../sim/stats';
import type { SimResult } from '../sim/runner';
import { permutations } from '../sim/runner';

export interface SimRequest {
  agents: string[];
  games: number;
  seed: number;
  setupIndex: number;
}

export interface SimProgress {
  kind: 'progress';
  done: number;
  total: number;
}

export interface SimDone {
  kind: 'done';
  stats: ReturnType<typeof computeStats>;
}

export type SimMessage = SimProgress | SimDone | { kind: 'error'; message: string };

self.onmessage = (e: MessageEvent<SimRequest>) => {
  const { agents: names, games, seed, setupIndex } = e.data;
  try {
    const agents = names.map((n) => makeAgent(n));
    const perms = permutations(agents.length);
    const collected: SimResult['games'] = [];
    const t0 = performance.now();

    for (let i = 0; i < games; i++) {
      const seating = perms[i % perms.length];
      const result = playGame(
        seating.map((agentIndex) => agents[agentIndex]),
        {
          players: agents.length,
          seed: (seed + i * 2654435761) >>> 0,
          setupIndex: setupIndex + i,
        },
      );
      collected.push({ result, seating });
      if (i % 5 === 4 || i === games - 1) {
        self.postMessage({ kind: 'progress', done: i + 1, total: games } satisfies SimProgress);
      }
    }

    const stats = computeStats({ games: collected }, names, (performance.now() - t0) / games);
    self.postMessage({ kind: 'done', stats } satisfies SimDone);
  } catch (err) {
    self.postMessage({ kind: 'error', message: (err as Error).message });
  }
};
