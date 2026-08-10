//! `arcs sim` — a batch and its paired comparison. Ported from
//! `src/sim/cli.ts`.

use std::time::Instant;

use arcs_agents::agent_names;
use arcs_engine::{AmbitionId, SetupMode, ambition_count, encode_action, setup::power_threshold};
use arcs_sim::{
    AgentSpec, PlayOptions, SimOptions, SimResult, build_table, compute_stats, default_workers,
    paired_stats, play_game_logged, simulate, simulate_parallel,
};
use clap::Args;

use crate::opts::parse_agent_opts;

#[derive(Args)]
pub struct SimArgs {
    /// Comma-separated agent list, one per seat.
    #[arg(long, default_value = "greedy,greedy,greedy")]
    agents: String,
    /// Batch size, rounded up to whole paired blocks.
    #[arg(long, default_value_t = 100)]
    games: usize,
    /// Base seed; the batch is fully reproducible from it.
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Seeds the opening: which of the printed setup cards is drawn.
    #[arg(long, default_value_t = 0)]
    setup: u64,
    /// Invent a fresh legal opening per deal instead of drawing a printed
    /// setup card — thousands of boards, for large batches.
    #[arg(long)]
    free_setup: bool,
    /// JSON passed to every agent, e.g. '{"iterations":800}'.
    #[arg(long)]
    opts: Option<String>,
    /// Keep seats fixed instead of permuting agents through them.
    #[arg(long)]
    no_rotate: bool,
    /// Give every game its own deal instead of holding it fixed across a block
    /// of seatings (much noisier; use only to show the contrast).
    #[arg(long)]
    unpaired: bool,
    /// Worker threads; 1 runs in-process.
    #[arg(long)]
    workers: Option<usize>,
    /// Play a single game and print a turn log.
    #[arg(long)]
    verbose: bool,
}

pub fn run(args: SimArgs) -> Result<(), String> {
    let names: Vec<String> = args
        .agents
        .split(',')
        .map(|n| n.trim().to_string())
        .collect();
    if !(2..=4).contains(&names.len()) {
        return Err(format!(
            "Arcs is a 2-4 player game; got {} agents. Available: {}",
            names.len(),
            agent_names().join(", ")
        ));
    }
    let players = names.len() as u8;
    let agent_opts = match &args.opts {
        Some(json) => parse_agent_opts(json)?,
        None => Default::default(),
    };
    let specs: Vec<AgentSpec> = names
        .iter()
        .map(|n| AgentSpec::with_opts(n, agent_opts))
        .collect();
    let setup_mode = if args.free_setup {
        SetupMode::Draw
    } else {
        SetupMode::Deck
    };

    if args.verbose {
        return verbose_game(&specs, players, args.seed, args.setup, setup_mode);
    }

    let sim_opts = SimOptions {
        players,
        games: args.games,
        seed: args.seed,
        setup_index: args.setup,
        setup_mode,
        rotate_seats: !args.no_rotate,
        paired: !args.unpaired,
        ..SimOptions::default()
    };
    let workers = args.workers.unwrap_or_else(default_workers);

    let t0 = Instant::now();
    let sim: SimResult = if workers > 1 {
        simulate_parallel(&specs, &sim_opts, workers, None).map_err(|e| e.to_string())?
    } else {
        simulate(&specs, &sim_opts).map_err(|e| e.to_string())?
    };
    let dt = t0.elapsed().as_secs_f64() * 1000.0;
    let played = sim.games.len();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let st = compute_stats(&sim, &name_refs, dt / played as f64);

    println!(
        "\n=== {players} players — {played} games, seed {} ({:.1}s, {:.2}ms/game, {:.0} games/s)",
        args.seed,
        dt / 1000.0,
        dt / played as f64,
        played as f64 / (dt / 1000.0)
    );
    if sim.paired {
        println!(
            "paired: {} deals × {} seatings — every agent plays each deal from every seat",
            played / sim.block_size,
            sim.block_size
        );
    }
    println!(
        "chapters {:.2}   rounds {:.1}   battles {:.1}   ambitions declared {:.1}",
        st.mean_chapters, st.mean_rounds, st.mean_battles, st.mean_declared
    );

    println!(
        "\n{:<12}{:>6}{:>15}{:>16}{:>6}{:>7}{:>7}{:>7}",
        "agent", "games", "win%", "power", "rank", "cities", "ships", "guild"
    );
    for a in &st.agents {
        println!(
            "{:<12}{:>6}{:>15}{:>16}{:>6.2}{:>7.1}{:>7.1}{:>7.1}",
            a.name,
            a.games,
            format!("{:.1}±{:.1}", a.win_rate * 100.0, a.win_rate_ci * 100.0),
            format!("{:.1}±{:.1}", a.mean_power, a.std_power),
            a.mean_rank,
            a.mean_cities,
            a.mean_ships,
            a.mean_guild_cards
        );
    }

    if let Some(pair) = paired_stats(&sim, &name_refs, 0, 1) {
        println!("\npaired head-to-head: {} vs {}", pair.a, pair.b);
        println!(
            "  {}{:.1}±{:.1} pts of win share to {}, over {} deals",
            if pair.diff >= 0.0 { "+" } else { "" },
            pair.diff,
            pair.ci,
            pair.a,
            pair.blocks
        );
        println!(
            "  deals won by {}: {}   by {}: {}   split: {}",
            pair.a, pair.a_better, pair.b, pair.b_better, pair.tied
        );
        println!(
            "{}",
            if pair.separated {
                "  the interval excludes zero — a real difference at this sample size"
            } else {
                "  the interval covers zero — NOT separated at this sample size"
            }
        );
    }

    println!("\nambition counts at game end");
    print!("{:<12}", "agent");
    for amb in AmbitionId::ALL {
        print!("{:>9}", format!("{amb:?}").to_lowercase());
    }
    println!();
    for a in &st.agents {
        print!("{:<12}", a.name);
        for amb in AmbitionId::ALL {
            print!("{:>9.1}", a.mean_ambition[amb.as_index()]);
        }
        println!();
    }

    let max_count = st
        .histogram
        .iter()
        .map(|h| h.count)
        .max()
        .unwrap_or(1)
        .max(1);
    println!("\nfinal Power distribution (all seats)");
    for h in &st.histogram {
        let width = ((h.count as f64 / max_count as f64) * 40.0).round() as usize;
        let bar = "█".repeat(width.max(usize::from(h.count > 0)));
        println!("  {:>3}–{:<3} {bar} {}", h.start, h.start + 4, h.count);
    }
    Ok(())
}

/// One game with a turn log, the TS `--verbose` mode.
fn verbose_game(
    specs: &[AgentSpec],
    players: u8,
    seed: u64,
    setup_index: u64,
    setup_mode: SetupMode,
) -> Result<(), String> {
    let mut agents = build_table(specs).map_err(|e| e.to_string())?;
    let play = PlayOptions {
        players,
        seed,
        setup_index,
        setup_mode,
        ..PlayOptions::default()
    };
    let r = play_game_logged(&mut agents, &play, &mut |state, player, action| {
        use arcs_engine::Action::*;
        if !matches!(
            action,
            Lead { .. } | Follow { .. } | DeclareAmbition { .. } | PassInitiative | Seize { .. }
        ) {
            return;
        }
        println!(
            "  ch{} r{}  P{} {}",
            state.chapter,
            state.stats.rounds + 1,
            player.as_index(),
            encode_action(action)
        );
    })
    .map_err(|e| e.to_string())?;

    println!(
        "\nseed {} — {}",
        r.seed,
        specs
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(" vs ")
    );
    for (seat, power) in r.power.iter().enumerate() {
        let ps = r.state.player(arcs_engine::Player(seat as u8));
        let counts: Vec<String> = AmbitionId::ALL
            .iter()
            .map(|&amb| {
                let label = format!("{amb:?}").to_lowercase();
                format!("{} {}", &label[..3], ambition_count(ps, amb))
            })
            .collect();
        println!(
            "  P{seat} {:<11} power {:>3}   {}   guild {}",
            specs[seat].name,
            power,
            counts.join("  "),
            ps.guild_cards.len()
        );
    }
    let threshold = power_threshold(players);
    let how = if r.power[r.winner] >= threshold {
        format!("reached {threshold} Power")
    } else {
        "led after chapter 5".to_string()
    };
    println!("  winner P{} ({}) — {how}", r.winner, specs[r.winner].name);
    println!(
        "  {} chapters, {} rounds, {} battles, {} ambitions declared",
        r.chapters, r.state.stats.rounds, r.state.stats.battles, r.state.stats.ambitions_declared
    );
    Ok(())
}
