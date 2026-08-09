/**
 * The strength gauntlet: the only way a bot earns a strength claim here.
 *
 * A candidate plays a 3-player table against two copies of each frozen anchor
 * (src/agents/anchors.ts), through the paired, seat-permuted harness that
 * `simulate` already provides. The pass rule, applied by every milestone:
 *
 *   1. a separated positive paired diff against the *newest* anchor,
 *   2. no separated regression against any older anchor, and
 *   3. mean thinking time within the per-decision budget — speed is a gate,
 *      not a footnote, because batch measurement is the binding constraint.
 *
 * Results are appended to docs/GAUNTLET.md via tools/gauntlet.ts.
 */
import { makeAgent, type Agent } from '../agents';
import type { SetupMode } from '../engine';
import { simulateParallel, type AgentSpec } from './parallel';
import { simulate, type SimResult } from './runner';
import { pairedStats, type PairedComparison } from './stats';

export type { AgentSpec } from './parallel';

export interface GauntletOptions {
  /** Games per anchor row; rounded up to whole paired blocks (6 at 3p). */
  gamesPerAnchor: number;
  seed: number;
  /** Per-decision wall-clock budget for the candidate, ms (default 30). */
  budgetMs?: number;
  /** Threads (default 1: in-process, exactly reproducible in tests). */
  workers?: number;
  /**
   * Games in the serial timing sample when running parallel (default 6 — one
   * paired block). Thinking time is a property of the agent, not of batch
   * contention: timed inside saturated workers, mcts reported 910ms/decision
   * against its real ~10ms, so the budget gate is measured in-process.
   */
  timingGames?: number;
  /** `deck` (default) or `draw` — see SetupMode. */
  setupMode?: SetupMode;
  onProgress?: (anchor: string, done: number, total: number) => void;
}

export interface GauntletRow {
  anchor: string;
  games: number;
  /** Candidate vs one anchor copy, per paired block. Positive favours the candidate. */
  pair: PairedComparison;
  /** Candidate's absolute win rate; fair share at a 3p table is 1/3. */
  winRate: number;
  /** Candidate's mean thinking time per decision, ms. */
  msPerDecision: number;
}

export interface GauntletReport {
  candidate: string;
  budgetMs: number;
  rows: GauntletRow[];
  /** Every row within the per-decision budget. */
  budgetOk: boolean;
  /** The full pass rule above. */
  passed: boolean;
}

/** Wrap an agent so every `choose` call is timed into `sink`. */
function timed(agent: Agent, sink: { ms: number; decisions: number }): Agent {
  return {
    name: agent.name,
    choose(obs, actions, ctx) {
      const t0 = performance.now();
      const action = agent.choose(obs, actions, ctx);
      sink.ms += performance.now() - t0;
      sink.decisions++;
      return action;
    },
  };
}

/**
 * Run the candidate against each anchor in ladder order (oldest to newest).
 */
export async function runGauntlet(
  candidate: AgentSpec,
  anchors: AgentSpec[],
  opts: GauntletOptions,
): Promise<GauntletReport> {
  const budgetMs = opts.budgetMs ?? 30;
  const workers = opts.workers ?? 1;
  const rows: GauntletRow[] = [];

  for (const anchor of anchors) {
    const sink = { ms: 0, decisions: 0 };
    const timedTable = () => [
      timed(makeAgent(candidate.name, candidate.opts), sink),
      makeAgent(anchor.name, anchor.opts),
      makeAgent(anchor.name, anchor.opts),
    ];

    let sim: SimResult;
    if (workers > 1) {
      sim = await simulateParallel([candidate, anchor, anchor], {
        players: 3,
        games: opts.gamesPerAnchor,
        seed: opts.seed,
        setupMode: opts.setupMode,
        workers,
        onProgress: (done, total) => opts.onProgress?.(anchor.name, done, total),
      });
      // The budget gate is timed in-process on a short serial sample — see
      // the `timingGames` note above.
      simulate(timedTable(), {
        players: 3,
        games: opts.timingGames ?? 6,
        seed: opts.seed,
        setupMode: opts.setupMode,
      });
    } else {
      sim = simulate(timedTable(), {
        players: 3,
        games: opts.gamesPerAnchor,
        seed: opts.seed,
        setupMode: opts.setupMode,
        onGame: (_r, i) => opts.onProgress?.(anchor.name, i + 1, opts.gamesPerAnchor),
      });
    }

    // The two anchor copies are symmetric under seat permutation, so the
    // paired comparison against either copy reads the same effect.
    const pair = pairedStats(sim, [candidate.name, anchor.name, anchor.name], 0, 1);
    if (!pair) throw new Error('gauntlet requires the paired harness');

    let wins = 0;
    for (const g of sim.games) if (g.seating[g.result.winner] === 0) wins++;

    rows.push({
      anchor: anchor.name,
      games: sim.games.length,
      pair,
      winRate: wins / sim.games.length,
      msPerDecision: sink.ms / Math.max(1, sink.decisions),
    });
  }

  const newest = rows[rows.length - 1];
  const budgetOk = rows.every((r) => r.msPerDecision <= budgetMs);
  const regressed = rows.some((r) => r.pair.separated && r.pair.diff < 0);
  const passed =
    rows.length > 0 && newest.pair.separated && newest.pair.diff > 0 && !regressed && budgetOk;

  return { candidate: candidate.name, budgetMs, rows, budgetOk, passed };
}
