//! The harness invariants themselves: determinism, permuted seatings, paired
//! blocks and what `paired_stats` reads off them.
//!
//! Ports the sim-facing tests from `tests/agents.test.ts` ("strength ladder")
//! and the properties `src/sim/runner.ts` and `docs/FINDINGS.md` state in
//! prose. The self-play test is the one that matters most: it is the exact
//! measurement bug FINDINGS records as the worst in the project.

use arcs_engine::SetupMode;
use arcs_sim::{AgentSpec, SimOptions, compute_stats, paired_stats, simulate};

fn specs(names: &[&str]) -> Vec<AgentSpec> {
    names.iter().map(|n| AgentSpec::new(*n)).collect()
}

fn opts(players: u8, games: usize, seed: u64) -> SimOptions {
    SimOptions {
        players,
        games,
        seed,
        ..SimOptions::default()
    }
}

/// Same seed, same batch. Everything else in this file — and every ledger row
/// the CLI prints — is worth nothing without this.
#[test]
fn a_batch_is_reproducible_from_its_seed() {
    let s = specs(&["random+", "random"]);
    let a = simulate(&s, &opts(2, 8, 5)).expect("batch runs");
    let b = simulate(&s, &opts(2, 8, 5)).expect("batch runs");
    assert_eq!(a, b);
}

/// A block is a pure function of its index: running blocks `[1, 3]` alone
/// reproduces those blocks of the whole batch, seed for seed and seating for
/// seating. (Ports `parallel.test.ts` "never splits a block across workers"
/// — the property the worker pool is built on.)
#[test]
fn a_block_is_a_pure_function_of_its_index() {
    let s = specs(&["random+", "random"]);
    let whole = simulate(&s, &opts(2, 8, 5)).expect("batch runs");
    let partial = simulate(
        &s,
        &SimOptions {
            blocks: Some(vec![1, 3]),
            ..opts(2, 8, 5)
        },
    )
    .expect("batch runs");

    let wanted: Vec<_> = whole
        .games
        .iter()
        .filter(|g| g.block == 1 || g.block == 3)
        .collect();
    assert_eq!(partial.games.len(), wanted.len());
    for (got, want) in partial.games.iter().zip(wanted) {
        assert_eq!(got.result.seed, want.result.seed);
        assert_eq!(got.result.winner, want.result.winner);
        assert_eq!(got.seating, want.seating);
        assert_eq!(got.result.power, want.result.power);
    }
}

/// **The self-play check.** Two identical agents must not come out separated.
///
/// FINDINGS: an unpaired, rotation-only harness had an agent losing to a copy
/// of itself by 14 points, in 7 of 8 replications — "that is bias, not
/// variance, and it is larger than most of the effects this file has
/// reported". Under paired common random numbers the answer is not merely
/// insignificant but *exactly* zero: within a block the two agents play the
/// same deal from every seat, and since they are identical the games are the
/// same games, so each seat's win lands once on each agent.
#[test]
fn an_agent_does_not_beat_a_copy_of_itself() {
    for (players, games) in [(2u8, 24usize), (3, 36)] {
        let names = vec!["greedy"; players as usize];
        let sim = simulate(&specs(&names), &opts(players, games, 9)).expect("batch runs");
        let pair = paired_stats(&sim, &names, 0, 1).expect("paired batch");
        assert_eq!(
            pair.diff, 0.0,
            "{players}p: identical agents differ by {} points",
            pair.diff
        );
        assert!(!pair.separated, "{players}p: identical agents separated");
        assert_eq!(pair.a_better, 0);
        assert_eq!(pair.b_better, 0);
        assert_eq!(pair.tied, pair.blocks);
    }
}

/// The same table read through `compute_stats`: permuted seatings split the
/// wins evenly between identical agents. (Ports `agents.test.ts` "identical
/// agents split evenly once seats are permuted", tightened from the TS
/// tolerance of 0.35 to exact equality, which pairing earns.)
#[test]
fn identical_agents_split_evenly_once_seats_are_permuted() {
    let names = ["greedy", "greedy"];
    let sim = simulate(&specs(&names), &opts(2, 24, 9)).expect("batch runs");
    let st = compute_stats(&sim, &names, 0.0);
    assert_eq!(st.agents[0].wins, st.agents[1].wins);
    assert_eq!(st.agents[0].games, st.agents[1].games);
}

/// Ports `agents.test.ts` "greedy beats random decisively".
#[test]
fn greedy_beats_random_decisively() {
    let names = ["greedy", "random"];
    let sim = simulate(&specs(&names), &opts(2, 12, 5)).expect("batch runs");
    let st = compute_stats(&sim, &names, 0.0);
    assert!(
        st.agents[0].win_rate > 0.8,
        "greedy won {:.2}",
        st.agents[0].win_rate
    );
}

/// Ports `agents.test.ts` "random+ beats random", at a batch size the effect
/// can actually be seen at.
///
/// The TS test asks 20 games; that is the exact mistake FINDINGS spends a
/// section on ("`random+` *is* an improvement — four null readings were the
/// harness"), where a real effect hid four times under 40-game batches. The
/// port keeps the assertion and fixes the sample: at 1200 paired games the
/// separation is unambiguous, and the engine runs them in a fraction of a
/// second.
#[test]
fn random_plus_beats_random() {
    let names = ["random+", "random"];
    let sim = simulate(&specs(&names), &opts(2, 1200, 6)).expect("batch runs");
    let pair = paired_stats(&sim, &names, 0, 1).expect("paired batch");
    assert!(
        pair.diff > 0.0 && pair.separated,
        "random+ vs random read {:+.1}±{:.1}",
        pair.diff,
        pair.ci
    );
}

/// A batch is rounded up to whole blocks, and every block really is one deal
/// played from all `n!` seatings.
#[test]
fn a_paired_block_is_one_deal_from_every_seating() {
    let sim =
        simulate(&specs(&["random", "random", "random"]), &opts(3, 7, 2)).expect("batch runs");
    assert_eq!(sim.block_size, 6);
    assert_eq!(sim.games.len(), 12, "7 games round up to two blocks of six");

    for block in 0..2 {
        let games: Vec<_> = sim.games.iter().filter(|g| g.block == block).collect();
        assert_eq!(games.len(), 6);
        let seed = games[0].result.seed;
        assert!(
            games.iter().all(|g| g.result.seed == seed),
            "one deal per block"
        );
        let mut seatings: Vec<_> = games.iter().map(|g| g.seating.clone()).collect();
        seatings.sort();
        seatings.dedup();
        assert_eq!(seatings.len(), 6, "every seating exactly once");
    }
}

/// Unpaired batches are still available — FINDINGS uses them only to show the
/// contrast — but `paired_stats` refuses to read one. A block-difference
/// estimator applied to unrelated deals is precisely the broken instrument.
#[test]
fn paired_stats_refuses_an_unpaired_batch() {
    let names = ["greedy", "greedy"];
    let sim = simulate(
        &specs(&names),
        &SimOptions {
            paired: false,
            ..opts(2, 8, 9)
        },
    )
    .expect("batch runs");
    assert!(!sim.paired);
    assert_eq!(sim.block_size, 1);
    assert!(paired_stats(&sim, &names, 0, 1).is_none());
}

/// `Draw` openings work end to end. FINDINGS keeps both modes because the
/// opening is a variable: the printed deck answers "who wins Arcs", free
/// draws answer "is this agent better".
#[test]
fn free_draw_openings_run() {
    let sim = simulate(
        &specs(&["random+", "random"]),
        &SimOptions {
            setup_mode: SetupMode::Draw,
            ..opts(2, 8, 3)
        },
    )
    .expect("batch runs");
    assert_eq!(sim.games.len(), 8);
}
