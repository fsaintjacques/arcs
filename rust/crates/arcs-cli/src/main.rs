//! `arcs` — the headless simulation and gauntlet CLI.
//!
//! Ports `src/sim/cli.ts` and `tools/gauntlet.ts`, including the markdown
//! ledger row the gauntlet prints for `docs/GAUNTLET.md`. The row carries the
//! short git SHA because a strength claim is only reproducible if the code
//! that produced it can be found again.
//!
//! ```text
//! arcs sim --agents greedy,greedy,random --games 240 --seed 1
//! arcs gauntlet --candidate mcts-c --games 240 --seed 1
//! ```

mod gauntlet_cmd;
mod opts;
mod sim_cmd;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "arcs", about = "Arcs simulation harness and strength gauntlet")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a batch of games and print the paired comparison.
    Sim(sim_cmd::SimArgs),
    /// Run a candidate through the strength gauntlet.
    Gauntlet(gauntlet_cmd::GauntletArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Sim(args) => sim_cmd::run(args),
        Command::Gauntlet(args) => gauntlet_cmd::run(args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// The short SHA of the checkout that produced a measurement.
///
/// `unknown` outside a git checkout, as in the TS tool — a ledger row is worth
/// more with a provenance hole in it than not at all.
pub fn short_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Today's UTC date as `YYYY-MM-DD`, for the ledger row.
///
/// Howard Hinnant's `civil_from_days`, rather than a date crate: the ledger
/// needs ten characters and this crate's dependency list is already the widest
/// in the workspace.
pub fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
