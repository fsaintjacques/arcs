//! Batch throughput, printed rather than asserted.
//!
//! `docs/FINDINGS.md` names batch size as the binding constraint on every
//! measurement in the lab — "making the search cheaper is therefore also a
//! measurement improvement" — so these numbers are the point of the port. Run
//! with:
//!
//! ```text
//! cargo test -p arcs-sim --release --test bench -- --ignored --nocapture
//! ```
//!
//! Ignored by default, for the reason FINDINGS gives at a larger scale: a
//! timing test inside a normal suite measures whatever else the machine is
//! doing.

use std::time::Instant;

use arcs_sim::{AgentSpec, SimOptions, default_workers, simulate, simulate_parallel};

fn specs(names: &[&str]) -> Vec<AgentSpec> {
    names.iter().map(|n| AgentSpec::new(*n)).collect()
}

/// Games per second, serial and across the pool, for each tier of agent.
///
/// The speedup is reported against the worker count rather than against the
/// core count on purpose: FINDINGS records a healthy TS pool at 5x on nine
/// workers, and a pool that is *not* delivering close to its worker count is
/// the first thing to suspect when a batch looks slower than it should.
#[test]
#[ignore = "timing; run explicitly with --release --ignored --nocapture"]
fn batch_throughput() {
    let workers = default_workers();
    println!(
        "\n{:<26}{:>10}{:>12}{:>12}{:>9}",
        "table (3p)", "games", "serial g/s", "pool g/s", "speedup"
    );

    for (label, names, games) in [
        ("random x3", ["random", "random", "random"], 6_000usize),
        ("greedy x3", ["greedy", "greedy", "greedy"], 600),
        ("mcts x3", ["mcts", "mcts", "mcts"], 60),
        ("mcts2 x3", ["mcts2", "mcts2", "mcts2"], 60),
    ] {
        let specs = specs(&names);
        let opts = SimOptions {
            players: 3,
            games,
            seed: 1,
            ..SimOptions::default()
        };

        let t0 = Instant::now();
        let serial = simulate(&specs, &opts).expect("serial batch");
        let serial_s = t0.elapsed().as_secs_f64();

        let t1 = Instant::now();
        let pool = simulate_parallel(&specs, &opts, workers, None).expect("pool batch");
        let pool_s = t1.elapsed().as_secs_f64();

        assert_eq!(serial.games, pool.games, "{label}: pool changed the result");
        println!(
            "{label:<26}{:>10}{:>12.1}{:>12.1}{:>8.1}x",
            serial.games.len(),
            serial.games.len() as f64 / serial_s,
            pool.games.len() as f64 / pool_s,
            serial_s / pool_s
        );
    }
    println!(
        "\n{workers} workers of {} cores\n",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    );
}
