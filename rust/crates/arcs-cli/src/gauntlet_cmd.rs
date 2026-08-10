//! `arcs gauntlet` — run a candidate through the ladder and print a ledger
//! row for `docs/GAUNTLET.md`. Ported from `tools/gauntlet.ts`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use arcs_agents::ANCHOR_LADDER;
use arcs_engine::SetupMode;
use arcs_sim::{AgentSpec, GauntletOptions, default_workers, run_gauntlet};
use clap::Args;

use crate::opts::parse_agent_opts;
use crate::{short_sha, today_utc};

#[derive(Args)]
pub struct GauntletArgs {
    /// Registry name of the agent under test.
    #[arg(long, default_value = "mcts")]
    candidate: String,
    /// JSON passed to the candidate, e.g. '{"battles":"exact"}'.
    #[arg(long)]
    opts: Option<String>,
    /// Comma-separated anchors, oldest first. Defaults to the frozen ladder.
    #[arg(long)]
    anchors: Option<String>,
    /// Games per anchor. 240 (40 deals × 6 seatings) is the promotion floor.
    #[arg(long, default_value_t = 240)]
    games: usize,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Per-decision thinking-time budget, ms.
    #[arg(long, default_value_t = 30.0)]
    budget_ms: f64,
    /// Worker threads; 1 runs in-process.
    #[arg(long)]
    workers: Option<usize>,
    /// Games in the serial timing sample when running parallel.
    #[arg(long, default_value_t = 6)]
    timing_games: usize,
    /// Invent a fresh legal opening per deal instead of drawing a printed
    /// setup card.
    #[arg(long)]
    free_setup: bool,
}

pub fn run(args: GauntletArgs) -> Result<(), String> {
    let candidate = AgentSpec::with_opts(
        &args.candidate,
        match &args.opts {
            Some(json) => parse_agent_opts(json)?,
            None => Default::default(),
        },
    );
    let anchor_names: Vec<String> = match &args.anchors {
        Some(list) => list.split(',').map(|n| n.trim().to_string()).collect(),
        None => ANCHOR_LADDER.iter().map(|n| n.to_string()).collect(),
    };
    let anchors: Vec<AgentSpec> = anchor_names.iter().map(AgentSpec::new).collect();
    let workers = args.workers.unwrap_or_else(default_workers);

    println!(
        "gauntlet: {} vs [{}]",
        candidate.name,
        anchor_names.join(", ")
    );
    println!(
        "{} games per anchor, seed {}, budget {}ms/decision, {workers} workers\n",
        args.games, args.seed, args.budget_ms
    );

    let opts = GauntletOptions {
        games_per_anchor: args.games,
        seed: args.seed,
        budget_ms: args.budget_ms,
        workers,
        timing_games: args.timing_games,
        setup_mode: if args.free_setup {
            SetupMode::Draw
        } else {
            SetupMode::Deck
        },
    };

    let t0 = Instant::now();
    let last = AtomicUsize::new(0);
    let progress = |anchor: &str, done: usize, total: usize| {
        if done.is_multiple_of(5) && last.swap(done, Ordering::Relaxed) != done {
            println!("  {anchor}: {done}/{total} blocks");
        }
    };
    let report =
        run_gauntlet(&candidate, &anchors, &opts, Some(&progress)).map_err(|e| e.to_string())?;
    let minutes = t0.elapsed().as_secs_f64() / 60.0;

    println!(
        "\n{:<20}{:>6}{:>14}{:>5}{:>7}{:>8}",
        "anchor", "games", "diff", "sep", "win%", "ms/dec"
    );
    for row in &report.rows {
        println!(
            "{:<20}{:>6}{:>14}{:>5}{:>7.1}{:>8.2}",
            row.anchor,
            row.games,
            format!("{}±{:.1}", signed(row.pair.diff), row.pair.ci),
            if row.pair.separated { "yes" } else { "no" },
            row.win_rate * 100.0,
            row.ms_per_decision
        );
    }
    println!(
        "\n{} (budget {}), {minutes:.1} min",
        if report.passed {
            "PASSED"
        } else {
            "not passed"
        },
        if report.budget_ok { "ok" } else { "EXCEEDED" }
    );

    let date = today_utc();
    let commit = short_sha();
    println!("\nledger rows for docs/GAUNTLET.md:");
    for row in &report.rows {
        println!(
            "| {date} | {}@{commit} | {} | {} | {}±{:.1} | {} | {:.2} | seed {} |",
            candidate.name,
            row.anchor,
            row.games,
            signed(row.pair.diff),
            row.pair.ci,
            if row.pair.separated { "yes" } else { "no" },
            row.ms_per_decision,
            args.seed
        );
    }
    Ok(())
}

fn signed(x: f64) -> String {
    format!("{}{x:.1}", if x >= 0.0 { "+" } else { "" })
}
