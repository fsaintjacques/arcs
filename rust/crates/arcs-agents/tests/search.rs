//! The search agents are deterministic, budget-respecting, and stronger than
//! their parts. Ported from `tests/mcts2.test.ts`, extended to cover the two
//! frozen ISMCTS anchors the gauntlet ladder depends on.

mod common;

use std::time::Instant;

use arcs_agents::{Agent, AgentCtx, AgentOpts, Mcts2, Mcts2Opts, make_agent};
use arcs_engine::{
    Action, GameState, Observation, Pending, Player, SetupMode, VariantDef, get_pending,
    legal_actions, make_variant, observe,
};

use common::Flow;

/// A wide mid-game node, captured the way `midGameNode` does in
/// `tests/mcts2.test.ts`: play a `random+` game and take a decision node deep
/// enough to have real structure and wide enough to have a real choice.
///
/// Deviation from TS: the original requires decision #120 itself to offer more
/// than 4 actions and throws otherwise. This takes the *first* node at or after
/// #120 that does, which is the same node whenever the TS version would not
/// have thrown, and does not make the fixture depend on a seed's exact shape.
struct MidGameNode {
    variant: VariantDef,
    obs: Observation,
    legal: Vec<Action>,
    player: Player,
}

fn mid_game_node(seed: u64) -> MidGameNode {
    let v = make_variant(3, 1, SetupMode::Draw);
    let mut captured: Option<(GameState, Player)> = None;
    let mut count = 0usize;

    common::play_game(
        &["random+", "random+", "random+"],
        3,
        seed,
        1,
        SetupMode::Draw,
        &mut |state, player| {
            count += 1;
            if count < 120 {
                return Flow::Continue;
            }
            let mut legal = Vec::new();
            legal_actions(state, &v, &mut legal);
            if legal.len() > 4 {
                // `state` is the live, mutating game — keep a copy.
                captured = Some((*state, player));
                return Flow::Stop;
            }
            Flow::Continue
        },
    );

    let (state, player) = captured.expect("no wide mid-game node captured");
    let mut legal = Vec::new();
    legal_actions(&state, &v, &mut legal);
    MidGameNode {
        obs: observe(&state, &v, player),
        variant: v,
        legal,
        player,
    }
}

// Ported from tests/mcts2.test.ts "same observation, same seed, same choice".
#[test]
fn mcts2_makes_the_same_choice_from_the_same_seed() {
    let node = mid_game_node(17);
    let opts = Mcts2Opts {
        iterations: 60,
        ..Mcts2Opts::default()
    };
    let mut a = Mcts2::new("mcts2", opts);
    let mut b = Mcts2::new("mcts2", opts);
    let mut ctx_a = AgentCtx::new(&node.variant, node.player, 5);
    let mut ctx_b = AgentCtx::new(&node.variant, node.player, 5);
    assert_eq!(
        a.choose(&node.obs, &node.legal, &mut ctx_a),
        b.choose(&node.obs, &node.legal, &mut ctx_b),
    );
}

// Ported from tests/mcts2.test.ts "respects a wall-clock budget within a small
// factor".
#[test]
fn mcts2_respects_a_wall_clock_budget() {
    let node = mid_game_node(18);
    let mut agent = Mcts2::new(
        "mcts2-play",
        Mcts2Opts {
            iterations: 1_000_000,
            time_ms: Some(25),
            ..Mcts2Opts::default()
        },
    );
    let mut ctx = AgentCtx::new(&node.variant, node.player, 6);
    let start = Instant::now();
    agent.choose(&node.obs, &node.legal, &mut ctx);
    let ms = start.elapsed().as_secs_f64() * 1e3;
    // Generous ceiling so CI never flakes; the point is that it stops, not
    // that it runs the million iterations.
    assert!(
        ms < 250.0,
        "mcts2-play took {ms:.0} ms against a 25 ms budget"
    );
}

/// The deadline is only checked every 8 iterations, so a zero budget still
/// pays for the first batch — but it must not run the iteration cap.
#[test]
fn a_zero_budget_stops_almost_immediately() {
    let node = mid_game_node(19);
    let mut agent = Mcts2::new(
        "mcts2-play",
        Mcts2Opts {
            iterations: 1_000_000,
            time_ms: Some(0),
            ..Mcts2Opts::default()
        },
    );
    let mut ctx = AgentCtx::new(&node.variant, node.player, 7);
    let start = Instant::now();
    let i = agent.choose(&node.obs, &node.legal, &mut ctx);
    assert!(i < node.legal.len(), "it still returns a legal index");
    assert!(start.elapsed().as_millis() < 250);
}

/// Every registered search agent plays whole games without producing an
/// illegal or out-of-range choice. `play_game` asserts both.
#[test]
fn the_search_agents_play_legally() {
    for name in ["mc", "mcts-fast", "mcts2"] {
        let power = common::play_game(
            &[name, "random+", "greedy"],
            3,
            4,
            4,
            SetupMode::Deck,
            &mut |_, _| Flow::Continue,
        );
        assert_eq!(power.len(), 3);
        assert!(power.iter().any(|&p| p > 0), "{name} scored nothing");
    }
}

/// Each ablation switch plays a whole game on its own. `rollout_leaf: false`
/// is the one that matters here: it is the only configuration that reaches
/// `frontier_value`'s exact-battle branch, where an unrolled battle is valued
/// as the probability-weighted eval over `battle_distribution`'s top mass
/// instead of one sampled roll — so the test also asserts the game it played
/// actually contained battles.
#[test]
fn every_ablation_switch_plays_a_whole_game() {
    let ablations: [(&str, AgentOpts); 3] = [
        (
            "eval leaves",
            AgentOpts {
                rollout_leaf: Some(false),
                ..AgentOpts::default()
            },
        ),
        (
            "uniform priors",
            AgentOpts {
                priors: Some(false),
                ..AgentOpts::default()
            },
        ),
        (
            "no world pool",
            AgentOpts {
                worlds: Some(0),
                ..AgentOpts::default()
            },
        ),
    ];
    for (label, ablation) in ablations {
        // A short budget: this is coverage of a code path, not a strength
        // measurement, and 440 iterations per decision is a minute of
        // debug-build time per ablation.
        let opts = AgentOpts {
            iterations: Some(24),
            ..ablation
        };
        let mut battles = 0u16;
        let power = common::play_game_with(
            &["mcts2", "random+", "random+"],
            &opts,
            3,
            6,
            6,
            SetupMode::Draw,
            &mut |state, _| {
                battles = battles.max(state.stats.battles);
                Flow::Continue
            },
        );
        assert_eq!(power.len(), 3, "{label} did not finish a game");
        assert!(battles > 0, "{label} never saw a battle");
    }
}

/// Both ISMCTS anchors and the PUCT anchor play legally too: the R6 gauntlet
/// cannot run if a ladder rung cannot finish a game.
#[test]
fn every_anchor_finishes_a_game() {
    for name in [
        "anchor-greedy-v0",
        "anchor-mcts300-v0",
        "anchor-mcts-c-v1",
        "anchor-mcts2-v2",
    ] {
        let power = common::play_game(
            &[name, "random+", "random+"],
            3,
            2,
            2,
            SetupMode::Draw,
            &mut |_, _| Flow::Continue,
        );
        assert_eq!(power.len(), 3, "{name} did not finish");
    }
}

/// A node with exactly one legal action is answered without searching — the
/// TS `if (actions.length === 1) return actions[0]` short-circuit, which is
/// what keeps a forced cascade from costing a full iteration budget.
#[test]
fn a_forced_move_costs_no_search() {
    let v = make_variant(3, 5, SetupMode::Draw);
    let mut rng = arcs_engine::SplitMix64::new(5);
    let mut s = arcs_engine::new_game(&v, &mut rng, 5, SetupMode::Draw);
    let player = loop {
        match get_pending(&s, &v) {
            Pending::Chance => arcs_engine::resolve_chance_mut(&mut s, &v, &mut rng).unwrap(),
            Pending::Decision { player } => break player,
            Pending::Over => panic!("game over before a decision"),
        }
    };
    let mut legal = Vec::new();
    legal_actions(&s, &v, &mut legal);
    let obs = observe(&s, &v, player);
    let only = &legal[..1];

    let mut agent = make_agent(
        "mcts2",
        &AgentOpts {
            iterations: Some(1_000_000),
            ..AgentOpts::default()
        },
    )
    .unwrap();
    let mut ctx = AgentCtx::new(&v, player, 1);
    let start = Instant::now();
    assert_eq!(agent.choose(&obs, only, &mut ctx), 0);
    assert!(
        start.elapsed().as_millis() < 100,
        "a forced move should not run the iteration budget"
    );
}
