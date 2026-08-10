//! A miniature of the TS `playGame` (`src/sim/runner.ts`), shared by the
//! agent integration tests.
//!
//! The real harness — permuted seatings, paired common random numbers,
//! `pairedStats` — is R6. This exists so the ported tests can do what their TS
//! originals do: run a seeded game and inspect it at every decision node
//! (`onDecision` in the TS runner).

use arcs_agents::{Agent, AgentCtx, AgentOpts, make_agent};
use arcs_engine::{
    Action, GameState, Pending, Player, SetupMode, SplitMix64, VariantDef, apply_action_mut,
    get_pending, legal_actions, make_variant, new_game, observe, resolve_chance_mut, standings,
};

/// What a decision hook wants to do next.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flow {
    Continue,
    Stop,
}

/// Play one seeded game, calling `on_decision(state, player)` at every
/// decision node *before* the chosen action is applied. Returns each seat's
/// final power (all zeros if the hook stopped the game early).
///
/// `setup_index` is separate from `seed`, as in the TS runner: the board and
/// the shuffle are independent knobs, and the ported tests fix the board while
/// varying the deal.
pub fn play_game(
    names: &[&str],
    players: u8,
    seed: u64,
    setup_index: u64,
    mode: SetupMode,
    on_decision: &mut dyn FnMut(&GameState, Player) -> Flow,
) -> Vec<u8> {
    play_game_with(
        names,
        &AgentOpts::default(),
        players,
        seed,
        setup_index,
        mode,
        on_decision,
    )
}

/// [`play_game`] with an explicit opts bag, so a test can drive an ablated
/// configuration of a registered agent.
#[allow(clippy::too_many_arguments)]
pub fn play_game_with(
    names: &[&str],
    opts: &AgentOpts,
    players: u8,
    seed: u64,
    setup_index: u64,
    mode: SetupMode,
    on_decision: &mut dyn FnMut(&GameState, Player) -> Flow,
) -> Vec<u8> {
    let v: VariantDef = make_variant(players, setup_index, mode);
    let mut rng = SplitMix64::new(seed ^ 0xA11CE);
    let mut s = new_game(&v, &mut rng, setup_index, mode);

    let mut agents: Vec<Box<dyn Agent>> = names
        .iter()
        .map(|n| make_agent(n, opts).expect("known agent"))
        .collect();
    let mut ctxs: Vec<AgentCtx> = (0..players)
        .map(|p| AgentCtx::new(&v, Player(p), seed ^ (0x9E37_79B9 * (p as u64 + 1))))
        .collect();

    let mut legal: Vec<Action> = Vec::new();
    let mut guard = 0;
    loop {
        match get_pending(&s, &v) {
            Pending::Over => break,
            Pending::Chance => resolve_chance_mut(&mut s, &v, &mut rng).expect("chance resolves"),
            Pending::Decision { player } => {
                if on_decision(&s, player) == Flow::Stop {
                    return vec![0; players as usize];
                }
                legal_actions(&s, &v, &mut legal);
                let obs = observe(&s, &v, player);
                let seat = player.as_index();
                let i = agents[seat].choose(&obs, &legal, &mut ctxs[seat]);
                assert!(
                    i < legal.len(),
                    "{} chose out of range",
                    agents[seat].name()
                );
                apply_action_mut(&mut s, &v, legal[i]).expect("a chosen action applies");
            }
        }
        guard += 1;
        assert!(guard < 200_000, "game failed to terminate");
    }

    let mut by_seat = vec![0u8; players as usize];
    for st in standings(&s).iter() {
        by_seat[st.player.as_index()] = st.power;
    }
    by_seat
}
