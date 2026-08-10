//! The strength gauntlet: the only way a bot earns a strength claim here.
//! Ported from `src/sim/gauntlet.ts`.
//!
//! A candidate plays a 3-player table against two copies of each frozen anchor
//! ([`arcs_agents::ANCHOR_LADDER`]), through the paired, seat-permuted harness
//! [`simulate_with_agents`] already provides. The pass rule, applied by every
//! milestone:
//!
//! 1. a separated positive paired diff against the **newest** anchor,
//! 2. no separated regression against any older anchor, and
//! 3. mean thinking time within the per-decision budget — speed is a gate, not
//!    a footnote, because batch measurement is the binding constraint on
//!    everything else in this lab.
//!
//! Results are appended to `docs/GAUNTLET.md` by the `arcs` CLI.

use std::cell::Cell;
use std::time::Instant;

use arcs_agents::{Agent, AgentCtx};
use arcs_engine::{Action, Observation, SetupMode};

use crate::parallel::simulate_parallel;
use crate::runner::{AgentSpec, BoxAgent, SimError, SimOptions, simulate_with_agents};
use crate::stats::{PairedComparison, paired_stats};

/// Wall-clock spent inside one agent's `choose`, and how many calls it was.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Timing {
    pub ms: f64,
    pub decisions: u64,
}

impl Timing {
    pub fn ms_per_decision(&self) -> f64 {
        self.ms / self.decisions.max(1) as f64
    }
}

/// Wrap an agent so every `choose` call is timed into a caller-owned cell.
///
/// The cell is borrowed rather than shared through an `Arc` on purpose: the
/// only correct place to read this number is a **serial, in-process** sample,
/// so a wrapper that could not be sent to a worker pool is a wrapper that
/// cannot be misused. See [`run_gauntlet`] for why that matters.
pub struct Timed<'a> {
    inner: BoxAgent<'static>,
    sink: &'a Cell<Timing>,
}

impl<'a> Timed<'a> {
    pub fn new(inner: BoxAgent<'static>, sink: &'a Cell<Timing>) -> Self {
        Timed { inner, sink }
    }
}

impl Agent for Timed<'_> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn choose(&mut self, obs: &Observation, legal: &[Action], ctx: &mut AgentCtx) -> usize {
        let t0 = Instant::now();
        let choice = self.inner.choose(obs, legal, ctx);
        let mut t = self.sink.get();
        t.ms += t0.elapsed().as_secs_f64() * 1000.0;
        t.decisions += 1;
        self.sink.set(t);
        choice
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct GauntletOptions {
    /// Games per anchor row; rounded up to whole paired blocks (6 at 3p).
    pub games_per_anchor: usize,
    pub seed: u64,
    /// Per-decision wall-clock budget for the candidate, ms.
    pub budget_ms: f64,
    /// Threads. 1 means in-process, which is exactly reproducible in tests.
    pub workers: usize,
    /// Games in the serial timing sample when running parallel (default 6 —
    /// one paired block). Thinking time is a property of the agent, not of
    /// batch contention: timed inside saturated workers, `mcts` once reported
    /// 910 ms/decision against its real ~10 ms, so the budget gate is measured
    /// in-process.
    pub timing_games: usize,
    /// `Deck` (the game as played) or `Draw`.
    pub setup_mode: SetupMode,
}

impl Default for GauntletOptions {
    fn default() -> Self {
        GauntletOptions {
            games_per_anchor: 240,
            seed: 1,
            budget_ms: 30.0,
            workers: 1,
            timing_games: 6,
            setup_mode: SetupMode::Deck,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct GauntletRow {
    pub anchor: String,
    pub games: usize,
    /// Candidate vs one anchor copy, per paired block. Positive favours the
    /// candidate.
    pub pair: PairedComparison,
    /// Candidate's absolute win rate; fair share at a 3p table is 1/3.
    pub win_rate: f64,
    /// Candidate's mean thinking time per decision, ms — always sampled
    /// serially.
    pub ms_per_decision: f64,
}

#[derive(Clone, PartialEq, Debug)]
pub struct GauntletReport {
    pub candidate: String,
    pub budget_ms: f64,
    pub rows: Vec<GauntletRow>,
    /// Every row within the per-decision budget.
    pub budget_ok: bool,
    /// The full pass rule above.
    pub passed: bool,
}

/// Progress hook: `(anchor name, blocks done, blocks total)`.
pub type ProgressFn<'a> = &'a (dyn Fn(&str, usize, usize) + Sync);

/// Run the candidate against each anchor in ladder order (oldest to newest).
pub fn run_gauntlet(
    candidate: &AgentSpec,
    anchors: &[AgentSpec],
    opts: &GauntletOptions,
    on_progress: Option<ProgressFn<'_>>,
) -> Result<GauntletReport, SimError> {
    let mut rows = Vec::with_capacity(anchors.len());

    for anchor in anchors {
        let specs = [candidate.clone(), anchor.clone(), anchor.clone()];
        let names = [
            candidate.name.as_str(),
            anchor.name.as_str(),
            anchor.name.as_str(),
        ];
        let sim_opts = SimOptions {
            players: 3,
            games: opts.games_per_anchor,
            seed: opts.seed,
            setup_mode: opts.setup_mode,
            ..SimOptions::default()
        };

        let sink = Cell::new(Timing::default());
        let sim = if opts.workers > 1 {
            let progress = on_progress
                .map(|cb| move |done: usize, total: usize| cb(anchor.name.as_str(), done, total));
            let sim = simulate_parallel(
                &specs,
                &sim_opts,
                opts.workers,
                progress
                    .as_ref()
                    .map(|f| f as &(dyn Fn(usize, usize) + Sync)),
            )?;
            // The budget gate is timed in-process on a short serial sample —
            // see `timing_games`. This deliberately re-plays the first block
            // rather than reading anything the pool measured.
            let mut table = timed_table(candidate, anchor, &sink)?;
            simulate_with_agents(
                &mut table,
                &SimOptions {
                    games: opts.timing_games,
                    ..sim_opts.clone()
                },
            )?;
            sim
        } else {
            let mut table = timed_table(candidate, anchor, &sink)?;
            simulate_with_agents(&mut table, &sim_opts)?
        };

        // The two anchor copies are symmetric under seat permutation, so the
        // paired comparison against either copy reads the same effect.
        let pair = paired_stats(&sim, &names, 0, 1).expect("the gauntlet is a paired batch");
        let wins = sim
            .games
            .iter()
            .filter(|g| g.seating[g.result.winner] == 0)
            .count();

        rows.push(GauntletRow {
            anchor: anchor.name.clone(),
            games: sim.games.len(),
            pair,
            win_rate: wins as f64 / sim.games.len().max(1) as f64,
            ms_per_decision: sink.get().ms_per_decision(),
        });
    }

    let budget_ok = rows.iter().all(|r| r.ms_per_decision <= opts.budget_ms);
    let regressed = rows.iter().any(|r| r.pair.separated && r.pair.diff < 0.0);
    let passed = rows.last().is_some_and(|newest| {
        newest.pair.separated && newest.pair.diff > 0.0 && !regressed && budget_ok
    });

    Ok(GauntletReport {
        candidate: candidate.name.clone(),
        budget_ms: opts.budget_ms,
        rows,
        budget_ok,
        passed,
    })
}

/// The 3p table with the candidate's `choose` calls timed.
fn timed_table<'a>(
    candidate: &AgentSpec,
    anchor: &AgentSpec,
    sink: &'a Cell<Timing>,
) -> Result<Vec<BoxAgent<'a>>, SimError> {
    Ok(vec![
        Box::new(Timed::new(candidate.build()?, sink)),
        anchor.build()?,
        anchor.build()?,
    ])
}
