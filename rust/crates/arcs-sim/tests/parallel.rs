//! The worker pool changes throughput, never results.
//! Ports `tests/parallel.test.ts`.

use arcs_sim::{
    AgentSpec, SimOptions, compute_stats, default_workers, paired_stats, simulate,
    simulate_parallel,
};

const NAMES: [&str; 2] = ["random+", "random"];

fn specs() -> Vec<AgentSpec> {
    NAMES.iter().map(|n| AgentSpec::new(*n)).collect()
}

fn opts() -> SimOptions {
    SimOptions {
        players: 2,
        games: 8,
        seed: 5,
        ..SimOptions::default()
    }
}

/// Ports "reproduces the serial run exactly, whatever the worker count".
///
/// The TS test compares `JSON.stringify` of the two stat tables; the Rust
/// equivalent is the `Debug` rendering, which prints every float in full
/// precision. "Byte-identical" is the real claim: not "statistically the
/// same", but the same digits, because the pool changes *when* work happens
/// and nothing else.
#[test]
fn reproduces_the_serial_run_exactly_whatever_the_worker_count() {
    let specs = specs();
    let serial = simulate(&specs, &opts()).expect("serial batch");

    for workers in [1usize, 3, 8] {
        let parallel = simulate_parallel(&specs, &opts(), workers, None).expect("parallel batch");
        assert_eq!(parallel.games.len(), serial.games.len());
        assert_eq!(parallel.block_size, serial.block_size);
        assert_eq!(parallel.games, serial.games, "{workers} workers");

        let a = compute_stats(&serial, &NAMES, 0.0);
        let b = compute_stats(&parallel, &NAMES, 0.0);
        assert_eq!(
            format!("{b:?}"),
            format!("{a:?}"),
            "{workers} workers: stats differ"
        );
        assert_eq!(
            paired_stats(&parallel, &NAMES, 0, 1),
            paired_stats(&serial, &NAMES, 0, 1)
        );
    }
}

/// The same guarantee for a search agent, whose decisions cost enough that a
/// scheduler could plausibly perturb them. It cannot: `mcts` is seeded per
/// seat and iteration-bounded, so only `mcts2-play`'s wall-clock budget —
/// deliberately not a measurement preset — would break this.
#[test]
fn a_search_agent_reproduces_across_workers_too() {
    let names = ["mcts-fast", "random+"];
    let specs: Vec<AgentSpec> = names.iter().map(|n| AgentSpec::new(*n)).collect();
    let o = SimOptions {
        players: 2,
        games: 4,
        seed: 11,
        ..SimOptions::default()
    };
    let serial = simulate(&specs, &o).expect("serial batch");
    let parallel = simulate_parallel(&specs, &o, 4, None).expect("parallel batch");
    assert_eq!(parallel.games, serial.games);
}

/// Every block is played exactly once, whatever the split. A worker that
/// dropped or duplicated a block would still produce a plausible-looking
/// number, which is why this is asserted rather than assumed.
#[test]
fn every_block_runs_exactly_once() {
    let specs = specs();
    for workers in [1usize, 2, 3, 5] {
        let sim = simulate_parallel(&specs, &opts(), workers, None).expect("parallel batch");
        let mut blocks: Vec<usize> = sim.games.iter().map(|g| g.block).collect();
        blocks.dedup();
        assert_eq!(blocks, vec![0, 1, 2, 3], "{workers} workers");
        assert_eq!(sim.games.len(), 8);
    }
}

/// Progress is reported once per block, on whichever worker finished it.
///
/// FINDINGS is explicit that this hook is *not* a place to time an agent —
/// wall-clock inside a saturated pool is a property of the pool. That is why
/// this crate has no in-worker timing to port: the TS test "reports in-worker
/// timing for the requested agent" covers a facility the gauntlet then had to
/// stop trusting, so the port keeps only the serial sample (see
/// `tests/gauntlet.rs`).
#[test]
fn progress_is_reported_once_per_block() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let seen = AtomicUsize::new(0);
    let cb = |_done: usize, total: usize| {
        assert_eq!(total, 4);
        seen.fetch_add(1, Ordering::Relaxed);
    };
    simulate_parallel(&specs(), &opts(), 3, Some(&cb)).expect("parallel batch");
    assert_eq!(seen.load(Ordering::Relaxed), 4);
}

#[test]
fn default_workers_leaves_the_machine_usable() {
    let w = default_workers();
    assert!(w >= 1);
    assert!(
        w < std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            + 1
    );
}
