//! The gauntlet points the right way, and the budget gate is a gate.
//! Ports `tests/gauntlet.test.ts`.

use arcs_sim::{AgentSpec, GauntletOptions, run_gauntlet};

/// Ports "reads a decisive matchup the right way". One paired block (6 games)
/// per anchor: a smoke test of the plumbing and the sign, not a strength
/// measurement.
#[test]
fn reads_a_decisive_matchup_the_right_way() {
    let report = run_gauntlet(
        &AgentSpec::new("greedy"),
        &[AgentSpec::new("random")],
        &GauntletOptions {
            games_per_anchor: 6,
            seed: 3,
            ..GauntletOptions::default()
        },
        None,
    )
    .expect("gauntlet runs");

    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].games, 6);
    assert!(report.rows[0].pair.diff > 0.0);
    assert!(report.rows[0].win_rate > 0.5);
    assert!(report.rows[0].ms_per_decision > 0.0);
    assert!(report.passed);
}

/// Ports "fails a candidate that blows the thinking budget". Speed is part of
/// the pass rule: a bot that cannot be measured cheaply is a worse instrument
/// whatever its strength, because batch size is the binding constraint on
/// everything else in the lab.
#[test]
fn fails_a_candidate_that_blows_the_thinking_budget() {
    let report = run_gauntlet(
        &AgentSpec::new("greedy"),
        &[AgentSpec::new("random")],
        &GauntletOptions {
            games_per_anchor: 6,
            seed: 3,
            budget_ms: 0.0,
            ..GauntletOptions::default()
        },
        None,
    )
    .expect("gauntlet runs");

    assert!(report.rows[0].pair.diff > 0.0);
    assert!(!report.budget_ok);
    assert!(!report.passed);
}

/// A separated regression against an *older* anchor fails the candidate even
/// when the newest row is positive. That rule is why the ladder is kept whole
/// rather than trimmed to the current champion.
#[test]
fn a_separated_regression_against_an_older_anchor_fails_the_run() {
    let report = run_gauntlet(
        &AgentSpec::new("random"),
        &[AgentSpec::new("greedy"), AgentSpec::new("random")],
        &GauntletOptions {
            games_per_anchor: 24,
            seed: 4,
            ..GauntletOptions::default()
        },
        None,
    )
    .expect("gauntlet runs");

    let old = &report.rows[0];
    assert!(old.pair.separated && old.pair.diff < 0.0, "{old:?}");
    assert!(!report.passed);
}

/// The candidate's thinking time is sampled **serially, in process**, whether
/// or not the batch itself ran on a pool.
///
/// FINDINGS: timed inside saturated workers, `mcts` reported 910 ms/decision
/// against a real ~10 ms — "thinking time is a property of the agent;
/// wall-clock inside a saturated pool is a property of the pool". So the
/// parallel and serial gauntlets must agree on the strength row exactly, and
/// must both report a timing in the same (small) neighbourhood rather than one
/// inflated by contention.
#[test]
fn timing_is_sampled_serially_whatever_the_worker_count() {
    let candidate = AgentSpec::new("greedy");
    let anchors = [AgentSpec::new("random")];
    let base = GauntletOptions {
        games_per_anchor: 12,
        seed: 3,
        timing_games: 6,
        ..GauntletOptions::default()
    };

    let serial = run_gauntlet(&candidate, &anchors, &base, None).expect("serial gauntlet");
    let parallel = run_gauntlet(
        &candidate,
        &anchors,
        &GauntletOptions { workers: 4, ..base },
        None,
    )
    .expect("parallel gauntlet");

    // The strength row is the same batch either way.
    assert_eq!(serial.rows[0].pair, parallel.rows[0].pair);
    assert_eq!(serial.rows[0].win_rate, parallel.rows[0].win_rate);
    // And both timings come from a serial in-process sample, so neither can
    // be a contention reading. A 90x gap is what the bug looked like.
    let (a, b) = (
        serial.rows[0].ms_per_decision,
        parallel.rows[0].ms_per_decision,
    );
    assert!(a > 0.0 && b > 0.0);
    assert!(
        b < a.max(1.0) * 20.0,
        "parallel timing {b:.2}ms vs serial {a:.2}ms — contention leaked into the gate"
    );
}

/// An unknown candidate fails loudly rather than silently measuring something
/// else.
#[test]
fn an_unknown_agent_is_an_error() {
    let err = run_gauntlet(
        &AgentSpec::new("mcts3"),
        &[AgentSpec::new("random")],
        &GauntletOptions {
            games_per_anchor: 6,
            ..GauntletOptions::default()
        },
        None,
    )
    .expect_err("unknown agent");
    assert!(err.to_string().contains("mcts3"));
}
