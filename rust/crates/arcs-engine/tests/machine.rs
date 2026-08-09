//! Ported from `tests/engine.test.ts`: the decision-process contract —
//! non-empty decision nodes, termination, determinism, conservation, and
//! the public-memory bookkeeping. Each test cites the TS test name.
//!
//! R1 games have no board actions, so a "random policy" here walks the
//! trick, Prelude and turn skeleton only; the full-width versions of these
//! tests return with R2.

mod common;

use arcs_engine::game::{apply_action_mut, get_pending, legal_actions, resolve_chance_mut};
use arcs_engine::{
    Action, FollowMode, GameState, Pending, Phase, Rng, SetupMode, SplitMix64, VariantDef,
    make_variant, new_game,
};
use common::*;

/// Drive a game to the end with a random policy, returning the number of
/// decisions taken (`driveTo` in engine.test.ts).
fn drive_to_end(v: &VariantDef, s: &mut GameState, rng: &mut SplitMix64) -> u32 {
    let mut decisions = 0;
    let mut acts = Vec::new();
    for _ in 0..100_000 {
        match get_pending(s, v) {
            Pending::Over => break,
            Pending::Chance => {
                resolve_chance_mut(s, v, rng).unwrap();
            }
            Pending::Decision { .. } => {
                legal_actions(s, v, &mut acts);
                assert!(!acts.is_empty(), "no legal actions in phase {:?}", s.phase);
                let a = acts[rng.gen_range(acts.len())];
                apply_action_mut(s, v, a).unwrap();
                decisions += 1;
            }
        }
    }
    decisions
}

// TS: "always offers at least one action at a decision node, and terminates"
#[test]
fn always_offers_an_action_and_terminates() {
    for players in 2..=4u8 {
        for seed in 0..12u64 {
            let v = make_variant(players, seed, SetupMode::Deck);
            let mut rng = SplitMix64::new(seed + 1);
            let mut s = new_game(&v, &mut rng, seed, SetupMode::Deck);
            let decisions = drive_to_end(&v, &mut s, &mut rng);
            assert_eq!(s.phase, Phase::Over);
            assert!(decisions > 20, "only {decisions} decisions");
        }
    }
}

// TS: "is reproducible from a seed" — adapted: the TS test runs agents via
// the sim runner (R6); the property under test is that the same seed
// yields the identical game, which `GameState: Eq` states directly.
#[test]
fn reproducible_from_a_seed() {
    let run = || {
        let v = make_variant(3, 2, SetupMode::Deck);
        let mut rng = SplitMix64::new(4242);
        let mut s = new_game(&v, &mut rng, 2, SetupMode::Deck);
        drive_to_end(&v, &mut s, &mut rng);
        s
    };
    let a = run();
    let b = run();
    assert_eq!(a, b);
    assert_eq!(a.phase, Phase::Over);
}

// The Copy contract that replaces TS "applyAction is pure: the input state
// is untouched" and "cloneState deep-copies every mutable branch": a plain
// copy is a full snapshot, and applying to the copy leaves the original
// untouched.
#[test]
fn copy_is_a_full_snapshot() {
    let mut f = start_game(3, 7, 2);
    let snapshot = f.s;
    let list = actions(&f);
    apply(&mut f, list[0]);
    assert_ne!(f.s, snapshot);
    // The snapshot still plays from where it was.
    let mut restored = Fixture {
        v: f.v,
        s: snapshot,
        rng: SplitMix64::new(1),
    };
    assert!(!actions(&restored).is_empty());
    apply(&mut restored, list[0]);
}

// TS: "conserves pieces: ships on the map plus supply plus Trophies equals
// 15" (no battles exist yet, so the trophy term is zero — R2 restores it).
#[test]
fn conserves_ships() {
    let v = make_variant(3, 0, SetupMode::Deck);
    let mut rng = SplitMix64::new(11);
    let mut s = new_game(&v, &mut rng, 0, SetupMode::Deck);
    drive_to_end(&v, &mut s, &mut rng);
    for p in 0..3 {
        let on_map: u8 = s
            .systems
            .iter()
            .map(|sys| sys.fresh[p] + sys.damaged[p])
            .sum();
        assert_eq!(on_map + s.player_states[p].ships_supply, 15);
    }
}

// TS: "never leaves a piece in an out-of-play cluster"
#[test]
fn never_leaves_a_piece_out_of_play() {
    let v = make_variant(2, 3, SetupMode::Deck);
    let mut rng = SplitMix64::new(12);
    let mut s = new_game(&v, &mut rng, 3, SetupMode::Deck);
    drive_to_end(&v, &mut s, &mut rng);
    for sys in &s.systems {
        if !sys.out_of_play {
            continue;
        }
        assert_eq!(sys.fresh.iter().sum::<u8>(), 0);
        assert_eq!(sys.damaged.iter().sum::<u8>(), 0);
        assert!(sys.buildings.is_empty());
    }
}

// TS: "forgets the public record when a new chapter is dealt"
#[test]
fn deal_forgets_the_public_record() {
    let mut f = start_game(3, 7, 0);
    f.s.revealed.push(arcs_engine::ActionCardId(0));
    f.s.revealed.push(arcs_engine::ActionCardId(1));
    f.s.declines.push(arcs_engine::state::Decline {
        player: arcs_engine::Player(1),
        suit: arcs_engine::Suit::Aggression,
        number: 3,
    });
    f.s.phase = Phase::Deal;
    resolve_chance_mut(&mut f.s, &f.v, &mut f.rng).unwrap();
    assert!(f.s.revealed.is_empty());
    assert!(f.s.declines.is_empty());
}

// TS: "records a decline whenever a follower does not Surpass"
#[test]
fn records_a_decline_on_copy_and_pivot() {
    let mut f = start_game(3, 3, 0);
    let leader = actor(&f);
    let lead = f.s.player_states[leader.as_index()].hand.as_slice()[0];
    apply(&mut f, Action::Lead { card: lead });
    let lead_def = arcs_engine::cards::action_card(lead);
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    let follower = actor(&f);
    let own = f.s.player_states[follower.as_index()].hand.as_slice()[0];
    apply(
        &mut f,
        Action::Follow {
            card: own,
            mode: FollowMode::Copy,
        },
    );

    assert_eq!(f.s.declines.len(), 1);
    let d = f.s.declines.as_slice()[0];
    assert_eq!(d.player, follower);
    assert_eq!(d.suit, lead_def.suit);
    assert_eq!(d.number, lead_def.number);
}

// Follow-up invariants specific to the R1 skeleton: a full random game
// never wedges a phase the milestone does not implement.
#[test]
fn r1_games_stay_inside_r1_phases() {
    for seed in 0..8u64 {
        let v = make_variant(3, seed, SetupMode::Deck);
        let mut rng = SplitMix64::new(seed + 500);
        let mut s = new_game(&v, &mut rng, seed, SetupMode::Deck);
        let mut acts = Vec::new();
        for _ in 0..100_000 {
            assert!(
                matches!(
                    s.phase,
                    Phase::Deal
                        | Phase::Mulligan
                        | Phase::Play
                        | Phase::Prelude
                        | Phase::Actions
                        | Phase::Reinforce
                        | Phase::Over
                ),
                "reached {:?}",
                s.phase
            );
            match get_pending(&s, &v) {
                Pending::Over => break,
                Pending::Chance => {
                    resolve_chance_mut(&mut s, &v, &mut rng).unwrap();
                }
                Pending::Decision { .. } => {
                    legal_actions(&s, &v, &mut acts);
                    let a = acts[rng.gen_range(acts.len())];
                    apply_action_mut(&mut s, &v, a).unwrap();
                }
            }
        }
        assert_eq!(s.phase, Phase::Over);
    }
}
