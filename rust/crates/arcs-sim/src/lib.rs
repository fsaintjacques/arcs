//! The measurement harness, ported from `src/sim/`.
//!
//! This crate is the lab's instrument, and `docs/FINDINGS.md` is a record of
//! what happens when the instrument is wrong: an unpaired runner once had an
//! agent losing to **a copy of itself by 14 points**, and a budget gate timed
//! inside saturated workers once reported `mcts` at 910 ms/decision against a
//! real ~10 ms. Both bugs looked like results. Every invariant below exists
//! because one of them cost real time, so they are ported as invariants, not
//! as suggestions:
//!
//! 1. **Permuted seatings, not rotations** ([`permutations`]). Rotation keeps
//!    the agents' cyclic order, and in a lead-and-follow game sitting behind a
//!    weak player is worth points.
//! 2. **Paired common random numbers** ([`simulate`]). One deal per block of
//!    `n!` games, so a lopsided deal is played from every seat before it
//!    counts.
//! 3. **A block is a pure function of its index.** That is what makes the
//!    batch partitionable ([`simulate_parallel`]) *and* what makes the
//!    parallel result byte-identical to the serial one.
//! 4. **The unit of observation is the block, not the game**
//!    ([`paired_stats`]).
//! 5. **Thinking time is sampled serially, in-process** ([`run_gauntlet`]) —
//!    wall-clock inside a saturated pool measures the pool.

pub mod gauntlet;
pub mod parallel;
pub mod runner;
pub mod stats;

pub use gauntlet::{GauntletOptions, GauntletReport, GauntletRow, Timed, Timing, run_gauntlet};
pub use parallel::{default_workers, simulate_parallel};
pub use runner::{
    AgentSpec, BoxAgent, GameResult, PlayOptions, SeatedGame, SimError, SimOptions, SimResult,
    build_table, permutations, play_game, play_game_logged, simulate, simulate_with_agents,
};
pub use stats::{AgentStats, BatchStats, PairedComparison, compute_stats, paired_stats};
