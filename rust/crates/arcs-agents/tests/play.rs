//! Agents playing whole games: determinism, honesty, and the one strength
//! check this milestone can make without the paired harness.
//!
//! The real measurement apparatus — permuted seatings, paired common random
//! numbers, `pairedStats` — is R6. Nothing here is a strength *claim* in the
//! `docs/GAUNTLET.md` sense; these are sanity checks that the agents work at
//! all, run over a fixed seed set.

use arcs_agents::{Agent, AgentCtx, AgentOpts, make_agent};
use arcs_engine::{
    Pending, Player, SetupMode, SplitMix64, VariantDef, apply_action_mut, get_pending,
    legal_actions, make_variant, new_game, observe, resolve_chance_mut, standings,
};

/// Play one game and return each seat's power at the end.
///
/// A miniature of the TS `playGame` (`src/sim/runner.ts`): poll the engine,
/// resolve chance from the game's own stream, and ask the seat to move. Seats
/// get their own RNG streams so one agent's sampling cannot perturb another's.
fn play_game(names: &[&str], players: u8, seed: u64, mode: SetupMode) -> Vec<u8> {
    let v: VariantDef = make_variant(players, seed, mode);
    let mut rng = SplitMix64::new(seed ^ 0xA11CE);
    let mut s = new_game(&v, &mut rng, seed, mode);

    let mut agents: Vec<Box<dyn Agent>> = names
        .iter()
        .map(|n| make_agent(n, &AgentOpts::default()).expect("known agent"))
        .collect();
    let mut ctxs: Vec<AgentCtx> = (0..players)
        .map(|p| AgentCtx::new(&v, Player(p), seed ^ (0x9E37_79B9 * (p as u64 + 1))))
        .collect();

    let mut legal = Vec::new();
    let mut guard = 0;
    loop {
        match get_pending(&s, &v) {
            Pending::Over => break,
            Pending::Chance => resolve_chance_mut(&mut s, &v, &mut rng).expect("chance resolves"),
            Pending::Decision { player, .. } => {
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

    // `standings` is ordered by rank; re-key it by seat so callers can index
    // with a seat number without silently comparing the wrong players.
    let mut by_seat = vec![0u8; players as usize];
    for st in standings(&s).iter() {
        by_seat[st.player.as_index()] = st.power;
    }
    by_seat
}

/// Share of the table's wins, ties split, as the harness will score it.
fn win_share(names: &[&str], players: u8, seeds: u64, seat: usize) -> f64 {
    let mut total = 0.0;
    for seed in 0..seeds {
        let power = play_game(names, players, seed, SetupMode::Draw);
        let best = *power.iter().max().expect("a winner");
        let winners = power.iter().filter(|&&p| p == best).count();
        if power[seat] == best {
            total += 1.0 / winners as f64;
        }
    }
    total / seeds as f64
}

/// Same seed, same game — the property every measurement in the lab rests on.
#[test]
fn games_are_reproducible_from_their_seed() {
    for seed in [1, 7, 99] {
        let a = play_game(&["greedy", "random", "random+"], 3, seed, SetupMode::Draw);
        let b = play_game(&["greedy", "random", "random+"], 3, seed, SetupMode::Draw);
        assert_eq!(a, b, "seed {seed} did not reproduce");
    }
}

/// An agent's decision is a function of (observation, legal list, its stream),
/// so replaying with a fresh context reproduces the choice.
#[test]
fn an_agent_chooses_deterministically() {
    let v = make_variant(3, 5, SetupMode::Draw);
    let mut rng = SplitMix64::new(5);
    let mut s = new_game(&v, &mut rng, 5, SetupMode::Draw);
    let mut legal = Vec::new();

    // Walk to the first real decision.
    let player = loop {
        match get_pending(&s, &v) {
            Pending::Chance => resolve_chance_mut(&mut s, &v, &mut rng).unwrap(),
            Pending::Decision { player, .. } => break player,
            Pending::Over => panic!("game over before a decision"),
        }
    };
    legal_actions(&s, &v, &mut legal);
    let obs = observe(&s, &v, player);

    for name in ["random", "random+", "greedy", "greedy-flat"] {
        let mut first = make_agent(name, &AgentOpts::default()).unwrap();
        let mut again = make_agent(name, &AgentOpts::default()).unwrap();
        let mut c1 = AgentCtx::new(&v, player, 1234);
        let mut c2 = AgentCtx::new(&v, player, 1234);
        assert_eq!(
            first.choose(&obs, &legal, &mut c1),
            again.choose(&obs, &legal, &mut c2),
            "{name} is not deterministic under a fixed stream"
        );
    }
}

/// Agents see an [`arcs_engine::Observation`], and a blanked one: an agent
/// that could read Rival hands would measure as strong for the wrong reason.
#[test]
fn agents_cannot_see_rival_hands() {
    let v = make_variant(3, 11, SetupMode::Draw);
    let mut rng = SplitMix64::new(11);
    let mut s = new_game(&v, &mut rng, 11, SetupMode::Draw);
    while matches!(get_pending(&s, &v), Pending::Chance) {
        resolve_chance_mut(&mut s, &v, &mut rng).unwrap();
    }
    let me = Player(0);
    let obs = observe(&s, &v, me);
    assert!(
        !s.player_states[1].hand.is_empty(),
        "the position should have dealt rivals a hand to hide"
    );
    for p in 1..s.players {
        // A rival's hand is emptied rather than filled with sentinels, as in
        // `observe.ts`; its size stays public and travels in `hand_sizes`.
        assert!(
            obs.state.player_states[p as usize].hand.is_empty(),
            "rival {p}'s hand is legible in the observation"
        );
        assert_eq!(
            obs.hand_sizes[p as usize],
            s.player_states[p as usize].hand.len() as u8,
            "rival {p}'s hand size should still be public"
        );
    }
    // The observer's own hand and the public record survive intact.
    assert_eq!(obs.state.player_states[0].hand, s.player_states[0].hand);
    assert_eq!(obs.state.revealed, s.revealed);
}

/// Greedy should beat the random floor decisively. This is a smoke test of the
/// evaluation, not a gauntlet result: one fixed seat, no seat permutation, no
/// paired deals, so it says "the bot works", not "the bot is this strong".
#[test]
fn greedy_beats_the_random_floor() {
    let share = win_share(&["greedy", "random", "random"], 3, 60, 0);
    assert!(
        share > 0.75,
        "greedy won {share:.2} of a 3p table against two random bots; \
         a working evaluation should be far above the 1/3 floor"
    );
}

/// `random+` never idles a turn away, and that alone should beat `random`.
/// FINDINGS records it as a meaningfully stronger floor, which is why it is
/// the rollout policy.
#[test]
fn random_plus_beats_random() {
    let share = win_share(&["random+", "random", "random"], 3, 60, 0);
    assert!(
        share > 0.40,
        "random+ won {share:.2}; it should clear the 1/3 floor"
    );
}

/// Settling cascades before scoring is what makes the 1-ply evaluation honest:
/// `docs/FINDINGS.md` records that scoring an action before its consequences
/// exist is a methodology trap. Both must still play legally.
#[test]
fn greedy_and_greedy_flat_both_play_legally() {
    for seed in 0..12 {
        let power = play_game(
            &["greedy", "greedy-flat", "random+"],
            3,
            seed,
            SetupMode::Deck,
        );
        assert_eq!(power.len(), 3);
        assert!(power.iter().any(|&p| p > 0), "seed {seed} scored nothing");
    }
}
