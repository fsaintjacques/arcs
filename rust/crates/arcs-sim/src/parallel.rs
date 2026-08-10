//! Partitioned batch runner. Ported from `src/sim/parallel.ts`.
//!
//! The paired harness's unit of meaning is the *block* — one deal played from
//! every seating — and a block's games depend only on its index
//! ([`SimOptions::blocks`]). So the pool hands each thread a slice of block
//! indices, the thread runs the ordinary serial loop over them, and the parent
//! stitches the results back in block order. Same seeds, same seatings, same
//! stats as the serial run: `tests/parallel.rs` asserts the two produce
//! **byte-identical** statistics.
//!
//! Agents cross the thread boundary as [`AgentSpec`]s rebuilt on the far side,
//! for the same reason the TS pool does it: a bot's state is not shareable, and
//! rebuilding it forces everything measurable to exist in the registry.
//!
//! The guarantee has one precondition, and it is worth naming because it is
//! the kind of thing that decays silently: **an agent's choice must not depend
//! on wall-clock time**. `mcts2-play` carries a per-decision `time_ms` budget
//! and therefore searches deeper on an idle machine than on a busy one; run it
//! through here and the parallel result will not reproduce the serial one. It
//! is an interactive preset, not a measurement preset, which is why the
//! gauntlet does not use it.

use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::runner::{
    AgentSpec, SeatedGame, SimError, SimOptions, SimResult, build_table, simulate_with_agents,
};

/// Threads to use when the caller does not say: all cores but one, so a long
/// batch leaves the machine usable.
pub fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1).max(1))
        .unwrap_or(1)
}

/// Run a batch across `workers` threads.
///
/// `on_block` is called after each block finishes, with `(done, total)` block
/// counts. It runs on whichever worker finished the block, so it must be cheap
/// and thread-safe — and, per FINDINGS, it must **not** be used to time an
/// agent: wall-clock inside a saturated pool measures the pool. The gauntlet
/// samples thinking time serially for exactly that reason.
pub fn simulate_parallel(
    specs: &[AgentSpec],
    opts: &SimOptions,
    workers: usize,
    on_block: Option<&(dyn Fn(usize, usize) + Sync)>,
) -> Result<SimResult, SimError> {
    let block_count = match &opts.blocks {
        Some(list) => list.len(),
        None => opts.block_count(),
    };
    if block_count == 0 {
        return Ok(SimResult {
            games: Vec::new(),
            block_size: opts.block_size(),
            paired: opts.rotate_seats && opts.paired,
        });
    }
    let all_blocks: Vec<usize> = match &opts.blocks {
        Some(list) => list.clone(),
        None => (0..block_count).collect(),
    };
    let workers = workers.clamp(1, block_count);

    // Contiguous slices, as in the TS pool: neighbouring blocks share nothing
    // but they do share a code path, and keeping a worker on a run of them
    // keeps its caches warm.
    let mut slices: Vec<Vec<usize>> = vec![Vec::new(); workers];
    for (i, &b) in all_blocks.iter().enumerate() {
        slices[i * workers / block_count].push(b);
    }

    let done = AtomicUsize::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .expect("rayon pool");

    let parts: Vec<Result<Vec<SeatedGame>, SimError>> = pool.install(|| {
        slices
            .par_iter()
            .map(|blocks| {
                // One table per worker, built once and reused across its
                // blocks — the agents' scratch buffers and search arenas are
                // the allocation the batch would otherwise repeat per game.
                let mut agents = build_table(specs)?;
                let mut out = Vec::new();
                for &block in blocks {
                    // One block at a time, so progress is reported at block
                    // granularity and a failure names the block that failed.
                    let sub = SimOptions {
                        blocks: Some(vec![block]),
                        ..opts.clone()
                    };
                    out.extend(simulate_with_agents(&mut agents, &sub)?.games);
                    if let Some(cb) = on_block {
                        cb(done.fetch_add(1, Ordering::Relaxed) + 1, block_count);
                    }
                }
                Ok(out)
            })
            .collect()
    });

    let mut games: Vec<SeatedGame> = Vec::new();
    for part in parts {
        games.extend(part?);
    }
    // Stable sort by block: within a block the seatings were pushed in `k`
    // order by the serial loop, and a stable sort preserves it. That is what
    // makes the reassembled batch identical to a serial one game for game.
    games.sort_by_key(|g| g.block);

    Ok(SimResult {
        games,
        block_size: opts.block_size(),
        paired: opts.rotate_seats && opts.paired,
    })
}
