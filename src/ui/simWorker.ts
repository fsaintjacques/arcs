/// <reference lib="webworker" />
/**
 * Runs simulation batches off the main thread so the UI stays responsive
 * while a few hundred games play out.
 *
 * This is the one part of the UI still on the **TypeScript** engine. Play and
 * Watch drive the Rust engine through wasm (`session.ts`); a batch harness
 * wants the whole `src/sim` stack — paired blocks, seat permutation,
 * `pairedStats` — which is the Rust R6 milestone, not this one. Moving the
 * panel over is a straight swap once `arcs-sim` exists.
 */
import { makeAgent } from '../agents';
import { simulate } from '../sim/runner';
import { computeStats, pairedStats, type PairedComparison } from '../sim/stats';

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
  paired: PairedComparison | null;
}

export type SimMessage = SimProgress | SimDone | { kind: 'error'; message: string };

self.onmessage = (e: MessageEvent<SimRequest>) => {
  const { agents: names, games, seed, setupIndex } = e.data;
  try {
    const agents = names.map((n) => makeAgent(n));
    const t0 = performance.now();

    // Calls the same `simulate` as the CLI rather than reimplementing the loop,
    // so paired seeding and permuted seating cannot drift between the two.
    const sim = simulate(agents, {
      players: agents.length,
      games,
      seed,
      setupIndex,
      onGame: (_result, i) => {
        if (i % 5 === 4) {
          self.postMessage({ kind: 'progress', done: i + 1, total: games } satisfies SimProgress);
        }
      },
    });

    const played = sim.games.length;
    const stats = computeStats(sim, names, (performance.now() - t0) / played);
    self.postMessage({
      kind: 'done',
      stats,
      paired: pairedStats(sim, names, 0, 1),
    } satisfies SimDone);
  } catch (err) {
    self.postMessage({ kind: 'error', message: (err as Error).message });
  }
};
