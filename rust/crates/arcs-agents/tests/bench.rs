//! Throughput of the playing agents, printed rather than asserted.
//!
//! `docs/FINDINGS.md` names batch size as the binding constraint on every
//! measurement in the lab — "making the search cheaper is therefore also a
//! measurement improvement" — so these numbers are the point of the port, not
//! a nicety. Run with:
//!
//! ```text
//! cargo test -p arcs-agents --release --test bench -- --ignored --nocapture
//! ```
//!
//! Ignored by default: a timing test inside a normal suite measures whatever
//! else the machine is doing. FINDINGS records the same trap at a larger
//! scale, where thinking time sampled inside saturated workers reported an
//! agent 90x slower than it was.

use arcs_agents::{Agent, AgentCtx, AgentOpts, make_agent};
use arcs_engine::{
    Pending, Player, SetupMode, SplitMix64, apply_action_mut, get_pending, legal_actions,
    make_variant, new_game, observe, resolve_chance_mut,
};
use std::time::Instant;

/// Play one game, returning how many decisions it took.
fn play(names: &[&str], players: u8, seed: u64) -> usize {
    let v = make_variant(players, seed, SetupMode::Draw);
    let mut rng = SplitMix64::new(seed ^ 0xA11CE);
    let mut s = new_game(&v, &mut rng, seed, SetupMode::Draw);
    let mut agents: Vec<Box<dyn Agent>> = names
        .iter()
        .map(|n| make_agent(n, &AgentOpts::default()).expect("known agent"))
        .collect();
    let mut ctxs: Vec<AgentCtx> = (0..players)
        .map(|p| AgentCtx::new(&v, Player(p), seed ^ (0x9E37_79B9 * (p as u64 + 1))))
        .collect();

    let mut legal = Vec::new();
    let mut decisions = 0;
    loop {
        match get_pending(&s, &v) {
            Pending::Over => break,
            Pending::Chance => resolve_chance_mut(&mut s, &v, &mut rng).unwrap(),
            Pending::Decision { player, .. } => {
                legal_actions(&s, &v, &mut legal);
                let obs = observe(&s, &v, player);
                let seat = player.as_index();
                let i = agents[seat].choose(&obs, &legal, &mut ctxs[seat]);
                apply_action_mut(&mut s, &v, legal[i]).unwrap();
                decisions += 1;
            }
        }
    }
    decisions
}

fn report(label: &str, names: &[&str], games: u64) {
    let start = Instant::now();
    let mut decisions = 0;
    for seed in 0..games {
        decisions += play(names, 3, seed);
    }
    let secs = start.elapsed().as_secs_f64();
    println!(
        "{label:<28} {:>8.1} games/s   {:>7.1} us/decision  ({games} games, {:.0} decisions/game)",
        games as f64 / secs,
        secs / decisions as f64 * 1e6,
        decisions as f64 / games as f64,
    );
}

/// Win share of seat 0 over `games` fixed seeds, ties split.
///
/// Not a gauntlet result: one fixed seat, no permuted seatings and no paired
/// deals, so seat effects and deal luck are still in the number. It is here to
/// show the agents differ in the expected direction, and the R6 harness will
/// replace it with something that can carry a claim.
fn report_share(label: &str, names: &[&str], games: u64) {
    let mut wins = 0.0;
    for seed in 0..games {
        let v = make_variant(3, seed, SetupMode::Draw);
        let mut rng = SplitMix64::new(seed ^ 0xA11CE);
        let mut s = new_game(&v, &mut rng, seed, SetupMode::Draw);
        let mut agents: Vec<Box<dyn Agent>> = names
            .iter()
            .map(|n| make_agent(n, &AgentOpts::default()).unwrap())
            .collect();
        let mut ctxs: Vec<AgentCtx> = (0..3u8)
            .map(|p| AgentCtx::new(&v, Player(p), seed ^ (0x9E37_79B9 * (p as u64 + 1))))
            .collect();
        let mut legal = Vec::new();
        loop {
            match get_pending(&s, &v) {
                Pending::Over => break,
                Pending::Chance => resolve_chance_mut(&mut s, &v, &mut rng).unwrap(),
                Pending::Decision { player, .. } => {
                    legal_actions(&s, &v, &mut legal);
                    let obs = observe(&s, &v, player);
                    let seat = player.as_index();
                    let i = agents[seat].choose(&obs, &legal, &mut ctxs[seat]);
                    apply_action_mut(&mut s, &v, legal[i]).unwrap();
                }
            }
        }
        let mut power = [0u8; 3];
        for st in arcs_engine::standings(&s).iter() {
            power[st.player.as_index()] = st.power;
        }
        let best = *power.iter().max().unwrap();
        if power[0] == best {
            wins += 1.0 / power.iter().filter(|&&p| p == best).count() as f64;
        }
    }
    println!(
        "{label:<28} seat-0 win share {:.3}  ({games} games, 1/3 is the floor)",
        wins / games as f64
    );
}

#[test]
#[ignore = "benchmark"]
fn agent_win_shares() {
    report_share("random+ vs 2 random", &["random+", "random", "random"], 300);
    report_share("greedy vs 2 random", &["greedy", "random", "random"], 300);
    report_share(
        "greedy vs 2 random+",
        &["greedy", "random+", "random+"],
        300,
    );
    report_share(
        "greedy vs 2 greedy-flat",
        &["greedy", "greedy-flat", "greedy-flat"],
        300,
    );
}

#[test]
#[ignore = "benchmark"]
fn agent_throughput() {
    report("random x3", &["random", "random", "random"], 2000);
    report("random+ x3", &["random+", "random+", "random+"], 2000);
    report("greedy x3", &["greedy", "greedy", "greedy"], 200);
    report(
        "greedy-flat x3",
        &["greedy-flat", "greedy-flat", "greedy-flat"],
        200,
    );
    report("greedy vs 2 random", &["greedy", "random", "random"], 400);
}
