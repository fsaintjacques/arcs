/**
 * Worker-pool batch runner for node.
 *
 * The paired harness's unit of meaning is the *block* — one deal played from
 * every seating — and a block's games depend only on its index (`simulate`'s
 * `blocks` option). So the pool hands each worker a slice of block indices,
 * the worker runs the ordinary serial loop over them, and the parent stitches
 * the results back in block order. Same seeds, same seatings, same stats as
 * the serial run — a test asserts byte-identical `computeStats` output.
 *
 * Agents cross the thread boundary as `{name, opts}` specs rebuilt by
 * `makeAgent` in the worker, since closures do not survive structured clone.
 * The convenient side effect: anything measurable must be registered in
 * src/agents/index.ts.
 *
 * Node-only (worker_threads); the web UI keeps its own single simWorker.
 */
import os from 'node:os';
import { Worker } from 'node:worker_threads';
import type { SimOptions, SimResult } from './runner';
import { permutations } from './runner';

/** How to build an agent on the far side of a thread boundary. */
export interface AgentSpec {
  name: string;
  opts?: Record<string, unknown>;
}

export interface ParallelSimOptions extends Omit<SimOptions, 'onGame'> {
  /** Thread count; defaults to all cores but one. */
  workers?: number;
  /** Time this agent's `choose` calls in-worker (index into the spec list). */
  timeAgentIndex?: number;
  onProgress?: (done: number, total: number) => void;
}

export interface ParallelSimResult extends SimResult {
  /** Wall-clock spent inside the timed agent's `choose`, if requested. */
  timing?: { ms: number; decisions: number };
}

interface WorkerDone {
  kind: 'done';
  games: SimResult['games'];
  timing: { ms: number; decisions: number } | null;
}
type WorkerMessage = { kind: 'progress' } | WorkerDone;

export async function simulateParallel(
  specs: AgentSpec[],
  opts: ParallelSimOptions,
): Promise<ParallelSimResult> {
  const rotate = opts.rotateSeats !== false;
  const paired = rotate && opts.paired !== false;
  const blockSize = paired ? permutations(specs.length).length : 1;
  const blockCount = Math.ceil(opts.games / blockSize);
  const totalGames = blockCount * blockSize;
  const workers = Math.max(1, Math.min(opts.workers ?? os.availableParallelism() - 1, blockCount));

  // Contiguous slices keep each worker's setup cache warm across neighbours.
  const slices: number[][] = Array.from({ length: workers }, () => []);
  for (let b = 0; b < blockCount; b++) slices[Math.floor((b * workers) / blockCount)].push(b);

  // Only plain data crosses the thread boundary: callbacks (onProgress here,
  // onDecision from PlayOptions) do not survive structured clone.
  const simOpts: Omit<SimOptions, 'onGame' | 'blocks' | 'onDecision'> = {
    players: opts.players,
    games: opts.games,
    seed: opts.seed,
    setupIndex: opts.setupIndex,
    setupMode: opts.setupMode,
    maxDecisions: opts.maxDecisions,
    rotateSeats: opts.rotateSeats,
    paired: opts.paired,
  };
  const { onProgress, timeAgentIndex } = opts;
  let done = 0;

  const results = await Promise.all(
    slices.map(
      (blocks) =>
        new Promise<WorkerDone>((resolve, reject) => {
          const worker = new Worker(new URL('./workerEntry.ts', import.meta.url), {
            workerData: { specs, opts: simOpts, blocks, timeAgentIndex: timeAgentIndex ?? null },
            execArgv: ['--import', 'tsx'],
          });
          worker.on('message', (m: WorkerMessage) => {
            if (m.kind === 'progress') onProgress?.(++done, totalGames);
            else resolve(m);
          });
          worker.on('error', reject);
          worker.on('exit', (code) => {
            if (code !== 0) reject(new Error(`sim worker exited with code ${code}`));
          });
        }),
    ),
  );

  const games = results.flatMap((r) => r.games).sort((a, b) => a.block - b.block);
  const timing =
    timeAgentIndex === undefined
      ? undefined
      : results.reduce(
          (acc, r) => ({
            ms: acc.ms + (r.timing?.ms ?? 0),
            decisions: acc.decisions + (r.timing?.decisions ?? 0),
          }),
          { ms: 0, decisions: 0 },
        );

  return { games, blockSize, paired, timing };
}
