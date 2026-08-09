//! Ported from `tests/rules.test.ts` "setup (p4-p5)": the `new_game`
//! opening. Each test cites the TS test name it ports.

mod common;

use arcs_engine::map::cluster_of;
use arcs_engine::setup::{SetupMode, draw_setup, setup_deck};
use arcs_engine::{BuildingKind, Phase, SplitMix64, make_variant, new_game, resolve_chance_mut};
use common::start_game;

// TS: "places 3 ships + a city, 3 ships + a starport, and 2 ships"
#[test]
fn places_the_printed_opening_pieces() {
    for players in 2..=4u8 {
        let v = make_variant(players, 0, SetupMode::Deck);
        let mut rng = SplitMix64::new(5);
        let s = new_game(&v, &mut rng, 0, SetupMode::Deck);
        for p in 0..players as usize {
            let ships: u8 = s
                .systems
                .iter()
                .map(|sys| sys.fresh[p] + sys.damaged[p])
                .sum();
            // 3 + 3 + 2 at 3-4 players; 2 players get two C systems.
            assert_eq!(ships, if players == 2 { 10 } else { 8 });
            let mut cities = 0;
            let mut ports = 0;
            for sys in &s.systems {
                for b in sys.buildings.iter() {
                    if b.player().as_index() != p {
                        continue;
                    }
                    match b.kind() {
                        BuildingKind::City => cities += 1,
                        BuildingKind::Starport => ports += 1,
                    }
                }
            }
            assert_eq!(cities, 1);
            assert_eq!(ports, 1);
            assert_eq!(s.player_states[p].ships_supply, 15 - ships);
        }
    }
}

// TS: "gives every player the 2 resources of their A and B planets (p5 step O)"
#[test]
fn gives_the_two_starting_resources() {
    let v = make_variant(3, 0, SetupMode::Deck);
    let mut rng = SplitMix64::new(5);
    let s = new_game(&v, &mut rng, 0, SetupMode::Deck);
    for p in 0..3 {
        assert_eq!(s.player_states[p].held_resources().len(), 2);
    }
}

// TS: "never starts anyone in an out-of-play cluster"
#[test]
fn never_starts_in_an_out_of_play_cluster() {
    for players in 2..=4u8 {
        for setup in 0..6u64 {
            let v = make_variant(players, setup, SetupMode::Deck);
            let mut rng = SplitMix64::new(1);
            let s = new_game(&v, &mut rng, setup, SetupMode::Deck);
            for (i, sys) in s.systems.iter().enumerate() {
                if !sys.out_of_play {
                    continue;
                }
                let total: u8 = sys.fresh.iter().sum();
                assert_eq!(total, 0, "system {i}");
                assert!(sys.buildings.is_empty(), "system {i}");
            }
        }
    }
}

// TS: "every setup card in the deck is legal" (the A/B-are-planets half;
// the structural half is covered by R0's setup tests).
#[test]
fn every_setup_card_starts_on_planets() {
    for players in 2..=4u8 {
        let v = make_variant(players, 0, SetupMode::Deck);
        for card in setup_deck(players) {
            for st in card.starts.iter() {
                // A and B take a city and a starport and yield a resource
                // at step O, so both must be planets.
                assert!(v.systems[st.a.as_index()].planet_type.is_some());
                assert!(v.systems[st.b.as_index()].planet_type.is_some());
                for &c in st.c.iter() {
                    assert!(!card.out_of_play.contains(&cluster_of(c)));
                }
            }
        }
    }
}

// TS: "gives position 1 to the player with the initiative, then clockwise
// (p5 step N)"
#[test]
fn position_one_follows_the_initiative_marker() {
    for players in 2..=4u8 {
        for seed in 0..40u64 {
            let v = make_variant(players, seed, SetupMode::Deck);
            let mut rng = SplitMix64::new(seed + 1);
            let s = new_game(&v, &mut rng, seed, SetupMode::Deck);
            let card = draw_setup(players, seed, SetupMode::Deck);
            for (position, st) in card.starts.iter().enumerate() {
                let seat = (s.initiative.0 + position as u8) % players;
                let city = s.systems[st.a.as_index()]
                    .buildings
                    .iter()
                    .find(|b| b.kind() == BuildingKind::City)
                    .unwrap_or_else(|| {
                        panic!("{players}p seed {seed}: no city at position {position}A")
                    });
                let port = s.systems[st.b.as_index()]
                    .buildings
                    .iter()
                    .find(|b| b.kind() == BuildingKind::Starport)
                    .unwrap_or_else(|| {
                        panic!("{players}p seed {seed}: no starport at position {position}B")
                    });
                assert_eq!(city.player().0, seat);
                assert_eq!(port.player().0, seat);
            }
        }
    }
}

// TS: "reaches every seat-to-position assignment across draws"
#[test]
fn every_rotation_is_reached() {
    for players in 2..=4u8 {
        let mut taken = vec![0u32; players as usize];
        for seed in 0..600u64 {
            let v = make_variant(players, seed, SetupMode::Deck);
            let mut rng = SplitMix64::new(seed + 1);
            let s = new_game(&v, &mut rng, seed, SetupMode::Deck);
            // The position seat 0 took.
            taken[((players - s.initiative.0) % players) as usize] += 1;
        }
        for &n in &taken {
            assert!(n > 600 / players as u32 / 2, "{players}p spread {taken:?}");
        }
    }
}

// TS: "randomises the turn order too"
#[test]
fn initiative_is_randomised() {
    for players in 2..=4u8 {
        let v = make_variant(players, 0, SetupMode::Deck);
        let mut holders = vec![0u32; players as usize];
        for seed in 0..600u64 {
            let mut rng = SplitMix64::new(seed * 7919 + 13);
            let s = new_game(&v, &mut rng, 0, SetupMode::Deck);
            holders[s.initiative.as_index()] += 1;
        }
        for &n in &holders {
            assert!(
                n > 600 / players as u32 / 2,
                "{players}p initiative {holders:?}"
            );
        }
    }
}

// TS: "seeds the phantom rival only at 2 players (p4 step K)"
#[test]
fn phantom_rival_only_at_two_players() {
    let v2 = make_variant(2, 0, SetupMode::Deck);
    let v3 = make_variant(3, 0, SetupMode::Deck);
    let mut rng = SplitMix64::new(1);
    let two = new_game(&v2, &mut rng, 0, SetupMode::Deck);
    let mut rng = SplitMix64::new(1);
    let three = new_game(&v3, &mut rng, 0, SetupMode::Deck);
    assert_eq!(two.phantom.iter().sum::<u8>(), 6);
    assert_eq!(three.phantom.iter().sum::<u8>(), 0);
}

// TS: "deals 6 cards to everyone (p5 step P)"
#[test]
fn deals_six_cards_to_everyone() {
    let f = start_game(4, 9, 0);
    for p in 0..4 {
        assert_eq!(f.s.player_states[p].hand.len(), 6);
    }
}

// Sanity for the deal chance node itself (game.ts `dealChapter`): the card
// multiset is conserved across deck + discard + hands, and a 2p deal opens
// with the mulligan decision.
#[test]
fn deal_conserves_the_deck_and_two_players_mulligan() {
    for players in 2..=4u8 {
        let v = make_variant(players, 3, SetupMode::Deck);
        let mut rng = SplitMix64::new(11);
        let mut s = new_game(&v, &mut rng, 3, SetupMode::Deck);
        let full: usize = v.action_deck.len();
        resolve_chance_mut(&mut s, &v, &mut rng).unwrap();
        let dealt: usize = (0..players as usize)
            .map(|p| s.player_states[p].hand.len())
            .sum();
        assert_eq!(dealt, players as usize * 6);
        assert_eq!(s.action_deck.len() + s.action_discard.len() + dealt, full);
        let mut all: Vec<u8> = s.action_discard.iter().map(|c| c.0).collect();
        for p in 0..players as usize {
            all.extend(s.player_states[p].hand.iter().map(|c| c.0));
        }
        all.sort_unstable();
        let mut expected: Vec<u8> = v.action_deck.iter().map(|c| c.0).collect();
        expected.sort_unstable();
        assert_eq!(all, expected);
        assert_eq!(
            s.phase,
            if players == 2 {
                Phase::Mulligan
            } else {
                Phase::Play
            }
        );
    }
}
