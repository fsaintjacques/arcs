//! The imperfect-information boundary, ported from the `observation` block of
//! `tests/engine.test.ts`.

use arcs_engine::cards::ACTION_CARD_COUNT;
use arcs_engine::game::{apply_action_mut, get_pending, legal_actions, resolve_chance_mut};
use arcs_engine::observe::{DeterminizeOptions, HIDDEN_CARD, determinize, observe, sample_world};
use arcs_engine::state::PlayedCard;
use arcs_engine::{
    Action, ActionCardId, GameState, Pending, Phase, PlayMode, Player, Rng, SetupMode, SplitMix64,
    VariantDef, make_variant, new_game,
};
use std::collections::HashSet;

/// A game a few random decisions in, so there is a trick in progress
/// (`midGame` in engine.test.ts).
fn mid_game(players: u8, seed: u64) -> (VariantDef, GameState, SplitMix64) {
    let v = make_variant(players, 1, SetupMode::Deck);
    let mut rng = SplitMix64::new(seed);
    let mut s = new_game(&v, &mut rng, 1, SetupMode::Deck);
    let mut acts = Vec::new();
    for _ in 0..6 {
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
    (v, s, rng)
}

#[test]
fn hides_rival_hands_and_every_deck_order() {
    let (v, s, _) = mid_game(3, 12);
    let obs = observe(&s, &v, Player(0));
    assert_eq!(obs.state.player_states[0].hand, s.player_states[0].hand);
    for p in 1..s.players as usize {
        assert!(obs.state.player_states[p].hand.is_empty());
    }
    assert!(obs.state.action_deck.is_empty());
    assert!(obs.state.action_discard.is_empty());
    assert!(obs.state.court_deck.is_empty());
}

#[test]
fn reports_rival_hand_sizes_even_though_the_cards_are_hidden() {
    let (v, s, _) = mid_game(3, 12);
    let obs = observe(&s, &v, Player(0));
    for p in 0..s.players as usize {
        assert_eq!(obs.hand_sizes[p], s.player_states[p].hand.len() as u8);
    }
}

#[test]
fn hides_rivals_face_down_plays_but_not_the_observers_own() {
    let (v, mut s, _) = mid_game(4, 33);
    s.round.played.clear();
    s.round.played.push(PlayedCard {
        player: Player(1),
        card: ActionCardId(5),
        mode: PlayMode::Copy,
        face_down: true,
    });
    s.round.played.push(PlayedCard {
        player: Player(0),
        card: ActionCardId(6),
        mode: PlayMode::Copy,
        face_down: true,
    });
    let obs = observe(&s, &v, Player(0));
    let mine = obs
        .state
        .round
        .played
        .iter()
        .find(|c| c.player == Player(0))
        .unwrap();
    let theirs = obs
        .state
        .round
        .played
        .iter()
        .find(|c| c.player == Player(1))
        .unwrap();
    assert_eq!(mine.card, ActionCardId(6));
    assert_eq!(theirs.card, HIDDEN_CARD);
    assert_eq!(obs.face_down_counts[0], 1);
    assert_eq!(obs.face_down_counts[1], 1);
}

#[test]
fn determinize_rebuilds_a_legal_playable_world() {
    let (v, s, _) = mid_game(3, 12);
    let obs = observe(&s, &v, Player(0));
    let mut acts = Vec::new();
    for i in 0..10u64 {
        let mut rng = SplitMix64::new(100 + i);
        let world = determinize(&obs, &v, &mut rng, DeterminizeOptions::default());
        assert_eq!(world.player_states[0].hand, s.player_states[0].hand);
        let mut all = Vec::new();
        for p in 0..world.players as usize {
            assert_eq!(
                world.player_states[p].hand.len(),
                obs.hand_sizes[p] as usize
            );
            all.extend(world.player_states[p].hand.iter().copied());
        }
        // No card exists twice across hands.
        let unique: HashSet<ActionCardId> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len());
        // No face-down play is still a placeholder.
        for c in world.round.played.iter() {
            assert_ne!(c.card, HIDDEN_CARD);
        }
        // And the sampled world can actually be played.
        match get_pending(&world, &v) {
            Pending::Over | Pending::Chance => {}
            Pending::Decision { .. } => {
                legal_actions(&world, &v, &mut acts);
                assert!(!acts.is_empty());
            }
        }
    }
}

#[test]
fn samples_genuinely_different_worlds() {
    let (v, s, _) = mid_game(4, 44);
    let obs = observe(&s, &v, Player(0));
    let mut seen = HashSet::new();
    for i in 0..8u64 {
        let mut rng = SplitMix64::new(500 + i);
        let world = determinize(&obs, &v, &mut rng, DeterminizeOptions::default());
        seen.insert(world.player_states[1].hand.as_slice().to_vec());
    }
    assert!(seen.len() > 1);
}

#[test]
fn never_deals_back_a_card_it_watched_being_played() {
    // Regression: `revealed` did not exist, so only the *current* round's
    // plays were accounted for. Cards played face up in earlier rounds of the
    // same chapter were forgotten and redealt into hands — in 65% of sampled
    // worlds, 1.56 impossible cards apiece.
    let v = make_variant(3, 1, SetupMode::Deck);
    let mut rng = SplitMix64::new(99);
    let mut s = new_game(&v, &mut rng, 1, SetupMode::Deck);
    resolve_chance_mut(&mut s, &v, &mut rng).unwrap();

    let mut seen: HashSet<ActionCardId> = HashSet::new();
    let mut acts = Vec::new();
    let mut checked = false;
    for i in 0..400u64 {
        match get_pending(&s, &v) {
            Pending::Over => break,
            Pending::Chance => {
                if s.phase == Phase::Deal {
                    seen.clear();
                }
                resolve_chance_mut(&mut s, &v, &mut rng).unwrap();
                continue;
            }
            Pending::Decision { .. } => {}
        }
        let before = s.round.played.len();
        legal_actions(&s, &v, &mut acts);
        let a = acts[rng.gen_range(acts.len())];
        apply_action_mut(&mut s, &v, a).unwrap();
        for pc in s.round.played.iter().skip(before) {
            if !pc.face_down {
                seen.insert(pc.card);
            }
        }

        if seen.len() >= 3 {
            checked = true;
            let obs = observe(&s, &v, Player(0));
            // The observer's public memory holds every card they watched
            // played.
            for &card in s.revealed.iter() {
                assert!(seen.contains(&card));
            }
            for w in 0..6u64 {
                let mut wrng = SplitMix64::new(i * 131 + w);
                let world = determinize(&obs, &v, &mut wrng, DeterminizeOptions::default());
                for p in 0..3usize {
                    for &c in world.player_states[p].hand.iter() {
                        assert!(
                            !s.revealed.contains(&c),
                            "card {c:?} was played and discarded"
                        );
                    }
                }
            }
        }
    }
    assert!(checked, "never reached three watched cards");
}

#[test]
fn never_invents_a_card_the_observer_can_already_account_for() {
    let (v, s, _) = mid_game(3, 55);
    let obs = observe(&s, &v, Player(0));
    let mine: HashSet<ActionCardId> = s.player_states[0].hand.iter().copied().collect();
    for i in 0..10u64 {
        let mut rng = SplitMix64::new(900 + i);
        let world = determinize(&obs, &v, &mut rng, DeterminizeOptions::default());
        for p in 1..world.players as usize {
            for card in world.player_states[p].hand.iter() {
                assert!(!mine.contains(card));
            }
        }
    }
}

/// Every card in the variant's deck sits in exactly one place in a sampled
/// world — the Rust conservation invariant behind the TS legality tests.
#[test]
fn a_sampled_world_conserves_the_action_deck() {
    for seed in 0..40u64 {
        let (v, s, _) = mid_game(3, seed);
        if s.phase == Phase::Over {
            continue;
        }
        let mut rng = SplitMix64::new(seed ^ 0xABC);
        let world = sample_world(&s, &v, Player(0), &mut rng);
        let mut count = [0u8; ACTION_CARD_COUNT];
        for p in 0..world.players as usize {
            for &c in world.player_states[p].hand.iter() {
                count[c.as_index()] += 1;
            }
        }
        for &c in world.action_deck.iter().chain(world.action_discard.iter()) {
            count[c.as_index()] += 1;
        }
        for c in world.round.played.iter() {
            count[c.card.as_index()] += 1;
        }
        for &id in v.action_deck.iter() {
            assert_eq!(
                count[id.as_index()],
                1,
                "card {id:?} misplaced (seed {seed})"
            );
        }
    }
}

/// Determinizing is a pure function of the observation and the RNG stream.
#[test]
fn determinize_is_deterministic_for_a_seed() {
    let (v, s, _) = mid_game(4, 7);
    let obs = observe(&s, &v, Player(2));
    let mut a = SplitMix64::new(4242);
    let mut b = SplitMix64::new(4242);
    assert_eq!(
        determinize(&obs, &v, &mut a, DeterminizeOptions::default()),
        determinize(&obs, &v, &mut b, DeterminizeOptions::default())
    );
}

/// The Court deck is rebuilt from the cards not visibly in play.
#[test]
fn court_deck_is_reshuffled_from_the_cards_not_in_play() {
    let (v, s, _) = mid_game(3, 5);
    let obs = observe(&s, &v, Player(0));
    let mut rng = SplitMix64::new(31);
    let world = determinize(&obs, &v, &mut rng, DeterminizeOptions::default());
    let mut placed: HashSet<u8> = HashSet::new();
    for slot in world.court.iter() {
        placed.insert(slot.card.0);
    }
    for &c in world.court_discard.iter() {
        placed.insert(c.0);
    }
    for p in 0..world.players as usize {
        for &c in world.player_states[p].guild_cards.iter() {
            placed.insert(c.0);
        }
    }
    for &c in world.court_deck.iter() {
        assert!(!placed.contains(&c.0), "court card {c:?} is in two places");
        placed.insert(c.0);
    }
    assert_eq!(placed.len(), v.court_deck.len());
}

/// A hidden play is dealt a card the observer has not already placed.
#[test]
fn face_down_plays_are_filled_from_the_unseen_pool() {
    let (v, mut s, _) = mid_game(3, 21);
    if s.phase == Phase::Over {
        return;
    }
    s.round.played.push(PlayedCard {
        player: Player(1),
        card: s.player_states[1].hand.as_slice()[0],
        mode: PlayMode::Copy,
        face_down: true,
    });
    let obs = observe(&s, &v, Player(0));
    let mine: HashSet<ActionCardId> = s.player_states[0].hand.iter().copied().collect();
    for i in 0..10u64 {
        let mut rng = SplitMix64::new(i);
        let world = determinize(&obs, &v, &mut rng, DeterminizeOptions::default());
        let hidden = world.round.played.as_slice().last().unwrap().card;
        assert_ne!(hidden, HIDDEN_CARD);
        assert!(!mine.contains(&hidden));
        assert!(!s.revealed.contains(&hidden));
    }
}

/// `observe` never leaks a Rival hand, even at a Farseers peek belonging to
/// somebody else.
#[test]
fn a_peek_only_reveals_the_hand_to_the_peeking_player() {
    let (v, mut s, _) = mid_game(3, 63);
    s.peek = Some(arcs_engine::state::Peek {
        player: Player(1),
        target: Some(Player(2)),
        resume: Phase::Prelude,
    });
    // The peeking player sees the named hand...
    let peeker = observe(&s, &v, Player(1));
    assert_eq!(
        peeker.state.player_states[2].hand, s.player_states[2].hand,
        "the peeking player sees the hand Farseers named"
    );
    // ...and nobody else does.
    let other = observe(&s, &v, Player(0));
    assert!(other.state.player_states[2].hand.is_empty());
    // The peeked cards are already placed, so they are never redealt.
    let mut rng = SplitMix64::new(9);
    let world = determinize(&peeker, &v, &mut rng, DeterminizeOptions::default());
    assert_eq!(world.player_states[2].hand, s.player_states[2].hand);
}

/// A determinized world is playable to the end of the game.
#[test]
fn a_sampled_world_plays_out() {
    let (v, s, mut rng) = mid_game(3, 77);
    if s.phase == Phase::Over {
        return;
    }
    let mut world = sample_world(&s, &v, Player(0), &mut rng);
    let mut acts = Vec::new();
    for _ in 0..200_000 {
        match get_pending(&world, &v) {
            Pending::Over => return,
            Pending::Chance => resolve_chance_mut(&mut world, &v, &mut rng).unwrap(),
            Pending::Decision { .. } => {
                legal_actions(&world, &v, &mut acts);
                assert!(!acts.is_empty());
                let a: Action = acts[rng.gen_range(acts.len())];
                apply_action_mut(&mut world, &v, a).unwrap();
            }
        }
    }
    panic!("sampled world did not terminate");
}

/// A state deep enough that face-up cards have been discarded, so `revealed`
/// is not empty. No TS counterpart: the TS suite never determinized a position
/// this far into a chapter.
fn deep_game(players: u8, seed: u64) -> (VariantDef, GameState, SplitMix64) {
    let v = make_variant(players, 1, SetupMode::Deck);
    let mut rng = SplitMix64::new(seed);
    let mut s = new_game(&v, &mut rng, 1, SetupMode::Deck);
    let mut acts = Vec::new();
    for _ in 0..200_000 {
        if !s.revealed.is_empty() {
            break;
        }
        match get_pending(&s, &v) {
            Pending::Over => break,
            Pending::Chance => resolve_chance_mut(&mut s, &v, &mut rng).unwrap(),
            Pending::Decision { .. } => {
                legal_actions(&s, &v, &mut acts);
                let a = acts[rng.gen_range(acts.len())];
                apply_action_mut(&mut s, &v, a).unwrap();
            }
        }
    }
    (v, s, rng)
}

/// A determinized world holds every action card in the variant, exactly once.
///
/// This is the invariant `observe.ts` quietly breaks: it removes the publicly
/// discarded `revealed` cards from the deal pool and never puts them back, so
/// a TS world is short by that many cards. Nothing at 1 ply notices, but a
/// search that rolls out into the next chapter deals off a short deck.
#[test]
fn determinize_conserves_every_action_card() {
    let mut want = [0u8; ACTION_CARD_COUNT];
    for seed in [3u64, 21, 44, 90] {
        let (v, s, mut rng) = deep_game(3, seed);
        if s.phase == Phase::Over {
            continue;
        }
        assert!(!s.revealed.is_empty(), "seed {seed} revealed nothing");
        want.fill(0);
        for &c in v.action_deck.iter() {
            want[c.as_index()] += 1;
        }
        for player in (0..s.players).map(Player) {
            let obs = observe(&s, &v, player);
            let world = determinize(&obs, &v, &mut rng, DeterminizeOptions::default());
            let mut count = [0u8; ACTION_CARD_COUNT];
            for p in 0..world.players as usize {
                for &c in world.player_states[p].hand.iter() {
                    count[c.as_index()] += 1;
                }
            }
            for c in world.round.played.iter() {
                count[c.card.as_index()] += 1;
            }
            for &c in world.action_deck.iter() {
                count[c.as_index()] += 1;
            }
            for &c in world.action_discard.iter() {
                count[c.as_index()] += 1;
            }
            assert_eq!(
                count, want,
                "seed {seed}, seat {player:?}: the sampled world is not a permutation of the deck"
            );
        }
    }
}

/// The same position sampled and then played to the end. A short deck only
/// bites at the next chapter deal, which is several rounds away — which is
/// why only a searching agent ever met it.
#[test]
fn a_world_sampled_late_in_a_chapter_plays_out() {
    let (v, s, mut rng) = deep_game(3, 21);
    if s.phase == Phase::Over {
        return;
    }
    let mut world = sample_world(&s, &v, Player(0), &mut rng);
    let mut acts = Vec::new();
    for _ in 0..200_000 {
        match get_pending(&world, &v) {
            Pending::Over => return,
            Pending::Chance => resolve_chance_mut(&mut world, &v, &mut rng).unwrap(),
            Pending::Decision { .. } => {
                legal_actions(&world, &v, &mut acts);
                assert!(!acts.is_empty());
                let a: Action = acts[rng.gen_range(acts.len())];
                apply_action_mut(&mut world, &v, a).unwrap();
            }
        }
    }
    panic!("sampled world did not terminate");
}
