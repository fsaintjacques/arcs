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

/// Time one seat's `choose` calls over `games` serial games.
///
/// The gauntlet's **≤30 ms/decision budget is a hard gate**, and it is a gate
/// on *one agent's thinking time*, not on the table's throughput — so this
/// measures a single seat and nothing else. It runs serially and single
/// threaded on purpose: `docs/FINDINGS.md` records thinking time sampled
/// inside saturated workers reporting an agent 90x slower than it actually
/// was, which is the exact mistake this function exists to avoid.
fn report_thinking(name: &str, opponents: [&str; 2], games: u64) {
    let names = [name, opponents[0], opponents[1]];
    let players = 3u8;
    let mut elapsed = std::time::Duration::ZERO;
    let mut decisions = 0u64;

    for seed in 0..games {
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
        loop {
            match get_pending(&s, &v) {
                Pending::Over => break,
                Pending::Chance => resolve_chance_mut(&mut s, &v, &mut rng).unwrap(),
                Pending::Decision { player, .. } => {
                    legal_actions(&s, &v, &mut legal);
                    let obs = observe(&s, &v, player);
                    let seat = player.as_index();
                    let i = if seat == 0 {
                        let start = Instant::now();
                        let i = agents[0].choose(&obs, &legal, &mut ctxs[0]);
                        elapsed += start.elapsed();
                        decisions += 1;
                        i
                    } else {
                        agents[seat].choose(&obs, &legal, &mut ctxs[seat])
                    };
                    apply_action_mut(&mut s, &v, legal[i]).unwrap();
                }
            }
        }
    }

    let ms = elapsed.as_secs_f64() * 1e3 / decisions as f64;
    println!(
        "{name:<20} {ms:>8.2} ms/decision  ({decisions} decisions over {games} games, \
         serial; gauntlet gate is 30.00)",
    );
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

/// Seat-0 win shares for the search tier. **Not a gauntlet claim**: one fixed
/// seating, no permuted seats, no paired deals, so seat effects and deal luck
/// are still in every number. It says the search agents beat the evaluation
/// tier in the expected direction; the R6 harness will say by how much.
#[test]
#[ignore = "benchmark"]
fn search_agent_win_shares() {
    report_share("mcts2 vs 2 greedy", &["mcts2", "greedy", "greedy"], 300);
    report_share("mcts vs 2 greedy", &["mcts", "greedy", "greedy"], 300);
    report_share("mcts-c vs 2 greedy", &["mcts-c", "greedy", "greedy"], 300);
    report_share("mcts2 vs 2 mcts", &["mcts2", "mcts", "mcts"], 200);
}

/// Thinking time per decision against the gauntlet's 30 ms gate.
#[test]
#[ignore = "benchmark"]
fn search_agent_thinking_time() {
    for name in [
        "greedy",
        "mc",
        "mcts-fast",
        "mcts",
        "mcts-c",
        "mcts2",
        "anchor-mcts300-v0",
        "anchor-mcts-c-v1",
        "anchor-mcts2-v2",
    ] {
        report_thinking(name, ["greedy", "greedy"], 20);
    }
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
