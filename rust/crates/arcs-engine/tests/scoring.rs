//! Ported from `tests/rules.test.ts` "ambition scoring (p18)", plus the
//! chapter-end returns (Trophies/Captives) that TS covers through
//! integration. Each test cites the TS test name.

mod common;

use arcs_engine::state::{GameState, TrophyKind};
use arcs_engine::{
    Action, AmbitionId, CourtCardId, Player, ResourceType, SetupMode, make_variant, new_game,
    score_ambition,
};
use common::*;

/// `scored` in rules.test.ts: a fresh game with the phantom cleared so the
/// tests control every count.
fn scored(players: u8, setup: impl FnOnce(&mut GameState)) -> (GameState, arcs_engine::VariantDef) {
    let v = make_variant(players, 0, SetupMode::Deck);
    let mut rng = arcs_engine::SplitMix64::new(1);
    let mut s = new_game(&v, &mut rng, 0, SetupMode::Deck);
    s.phantom = [0; AmbitionId::COUNT];
    setup(&mut s);
    (s, v)
}

fn declare_warlord(s: &mut GameState, markers: &[u8]) {
    s.declared[AmbitionId::Warlord.as_index()] = markers.iter().copied().collect();
    s.available_markers.retain(|m| !markers.contains(m));
}

// TS: "pays first and second place the marker's two values"
#[test]
fn pays_first_and_second_place_the_markers_two_values() {
    let (s, v) = scored(3, |st| {
        declare_warlord(st, &[0]); // the 5/3 marker
        st.player_states[0].trophies[1][TrophyKind::Ship.as_index()] = 2;
        st.player_states[1].trophies[0][TrophyKind::Ship.as_index()] = 1;
    });
    let r = score_ambition(&s, &v, AmbitionId::Warlord).unwrap();
    assert_eq!(r.awards[0], 5);
    assert_eq!(r.awards[1], 3);
    assert_eq!(r.awards[2], 0);
}

// TS: "drops everyone tied for first to second place"
#[test]
fn drops_everyone_tied_for_first_to_second_place() {
    let (s, v) = scored(3, |st| {
        declare_warlord(st, &[0]);
        st.player_states[0].trophies[1][TrophyKind::Ship.as_index()] = 1;
        st.player_states[1].trophies[0][TrophyKind::Ship.as_index()] = 1;
    });
    let r = score_ambition(&s, &v, AmbitionId::Warlord).unwrap();
    assert!(r.first_place.is_empty());
    assert_eq!(r.awards[0], 3);
    assert_eq!(r.awards[1], 3);
}

// TS: "pays nothing when a tie for second place forms"
#[test]
fn pays_nothing_when_a_tie_for_second_place_forms() {
    let (s, v) = scored(3, |st| {
        declare_warlord(st, &[0]);
        st.player_states[0].trophies[1][TrophyKind::Ship.as_index()] = 2;
        st.player_states[1].trophies[0][TrophyKind::Ship.as_index()] = 1;
        st.player_states[2].trophies[0][TrophyKind::Ship.as_index()] = 1;
    });
    let r = score_ambition(&s, &v, AmbitionId::Warlord).unwrap();
    assert_eq!(r.awards[0], 5);
    assert_eq!(r.awards[1], 0);
    assert_eq!(r.awards[2], 0);
}

// TS: "pays nobody when nobody has any of the counted thing (p18
// Qualifying)"
#[test]
fn pays_nobody_when_nobody_qualifies() {
    let (s, v) = scored(3, |st| {
        declare_warlord(st, &[0]);
    });
    let r = score_ambition(&s, &v, AmbitionId::Warlord).unwrap();
    assert!(r.awards.iter().all(|&a| a == 0));
}

// TS: "sums the values of every marker in a box"
#[test]
fn sums_the_values_of_every_marker_in_a_box() {
    let (s, v) = scored(3, |st| {
        // 5/3 and 2/0 -> 7/3, matching the p18 example.
        st.declared[AmbitionId::Tycoon.as_index()] = arcs_engine::InlineVec::from_slice(&[0, 2]);
        st.available_markers.retain(|m| *m == 1);
        // Setup hands out starting resources; clear them so the counts are
        // the ones this test states.
        for ps in st.player_states.iter_mut() {
            ps.resources = [None; 6];
        }
        st.player_states[0].resources[0] = Some(ResourceType::Material);
        st.player_states[0].resources[1] = Some(ResourceType::Fuel);
        st.player_states[1].resources[0] = Some(ResourceType::Material);
    });
    let r = score_ambition(&s, &v, AmbitionId::Tycoon).unwrap();
    assert_eq!(r.first, 7);
    assert_eq!(r.second, 3);
    assert_eq!(r.awards[0], 7);
    assert_eq!(r.awards[1], 3);
}

// TS: "counts Guild card suits toward ambitions, but never Weapon cards
// (p17)"
#[test]
fn counts_guild_card_suits_but_never_weapon_cards() {
    use arcs_engine::court::COURT_DECK;
    let relic = COURT_DECK
        .iter()
        .find(|c| c.suit == Some(ResourceType::Relic))
        .unwrap();
    let weapon = COURT_DECK
        .iter()
        .find(|c| c.suit == Some(ResourceType::Weapon))
        .unwrap();
    let (s, _v) = scored(3, |st| {
        st.player_states[0].resources = [None; 6]; // count the cards alone
        st.player_states[0].guild_cards =
            arcs_engine::InlineVec::from_slice(&[relic.id, weapon.id]);
    });
    assert_eq!(
        arcs_engine::ambition_count(&s.player_states[0], AmbitionId::Keeper),
        1
    );
    assert_eq!(
        arcs_engine::ambition_count(&s.player_states[0], AmbitionId::Tycoon),
        0
    );
}

// "+2 / +5 for an outright first place, by uncovered city slots (p18)" —
// bonusCityPower in ambitions.ts, asserted through a scored box.
#[test]
fn an_outright_first_place_earns_the_uncovered_city_bonuses() {
    for (cities_used, bonus) in [(2u8, 0u8), (3, 2), (5, 5)] {
        let (s, v) = scored(3, |st| {
            declare_warlord(st, &[0]);
            st.player_states[0].trophies[1][TrophyKind::Ship.as_index()] = 2;
            st.player_states[0].cities_used = cities_used;
        });
        let r = score_ambition(&s, &v, AmbitionId::Warlord).unwrap();
        assert_eq!(r.awards[0], 5 + bonus, "citiesUsed {cities_used}");
    }
    // A tie pays second place with no bonus.
    let (s, v) = scored(3, |st| {
        declare_warlord(st, &[0]);
        st.player_states[0].trophies[1][TrophyKind::Ship.as_index()] = 1;
        st.player_states[1].trophies[0][TrophyKind::Ship.as_index()] = 1;
        st.player_states[0].cities_used = 5;
    });
    let r = score_ambition(&s, &v, AmbitionId::Warlord).unwrap();
    assert_eq!(r.awards[0], 3);
}

// The 2-player phantom rival competes for placements (p19): tying it drops
// the real player to second.
#[test]
fn the_two_player_phantom_competes_for_first_place() {
    let (s, v) = scored(2, |st| {
        declare_warlord(st, &[0]);
        st.phantom[AmbitionId::Warlord.as_index()] = 2;
        st.player_states[0].trophies[1][TrophyKind::Ship.as_index()] = 2;
    });
    let r = score_ambition(&s, &v, AmbitionId::Warlord).unwrap();
    assert!(r.first_place.is_empty());
    assert_eq!(r.awards[0], 3); // second-place value only
    // Outscoring the phantom restores the outright win.
    let (s, v) = scored(2, |st| {
        declare_warlord(st, &[0]);
        st.phantom[AmbitionId::Warlord.as_index()] = 2;
        st.player_states[0].trophies[1][TrophyKind::Ship.as_index()] = 3;
    });
    let r = score_ambition(&s, &v, AmbitionId::Warlord).unwrap();
    assert_eq!(r.first_place.as_slice(), &[Player(0)]);
    assert_eq!(r.awards[0], 5);
    // And the phantom blocks second place for anyone below it.
    let (s, v) = scored(2, |st| {
        declare_warlord(st, &[0]);
        st.phantom[AmbitionId::Warlord.as_index()] = 2;
        st.player_states[0].trophies[1][TrophyKind::Ship.as_index()] = 3;
        st.player_states[1].trophies[0][TrophyKind::Ship.as_index()] = 2;
    });
    let r = score_ambition(&s, &v, AmbitionId::Warlord).unwrap();
    assert_eq!(r.awards[1], 0, "tied with the phantom for second");
}

// --- chapter-end returns (p19) ---------------------------------------------

/// End the chapter by passing with empty hands, as the TS chapter tests do.
fn end_chapter_now(f: &mut Fixture) {
    for p in 0..f.s.players as usize {
        let hand = f.s.player_states[p].hand;
        for &c in hand.iter() {
            f.s.action_discard.push(c);
        }
        f.s.player_states[p].hand.clear();
    }
    apply(f, Action::PassInitiative);
}

// "The pieces go home when Warlord scores" (p19): trophies return to their
// owners' supplies — including cities back onto the player board.
#[test]
fn warlord_scoring_returns_trophies_to_their_owners() {
    let mut f = start_game(3, 73, 0);
    declare_warlord(&mut f.s, &[0]);
    // Player 0 holds two of player 1's ships and one of their agents;
    // supplies were debited when they were captured.
    f.s.player_states[0].trophies[1][TrophyKind::Ship.as_index()] = 2;
    f.s.player_states[0].trophies[1][TrophyKind::Agent.as_index()] = 1;
    let sys = (0..24)
        .find(|&i| f.s.systems[i].fresh[1] >= 2)
        .expect("player 1 has ships on the map");
    f.s.systems[sys].fresh[1] -= 2;
    f.s.player_states[1].agents_supply -= 1;
    let ships_before = f.s.player_states[1].ships_supply;
    let agents_before = f.s.player_states[1].agents_supply;

    end_chapter_now(&mut f);

    assert_eq!(
        f.s.player_states[0].trophies[1][TrophyKind::Ship.as_index()],
        0
    );
    assert_eq!(f.s.player_states[1].ships_supply, ships_before + 2);
    assert_eq!(f.s.player_states[1].agents_supply, agents_before + 1);
    // Warlord paid its 5/3: outright first with 3 trophies.
    assert_eq!(f.s.player_states[0].power, 5);
}

// Captives return when Tyrant scores (p19).
#[test]
fn tyrant_scoring_returns_captives_to_their_owners() {
    let mut f = start_game(3, 74, 0);
    f.s.declared[AmbitionId::Tyrant.as_index()] = arcs_engine::InlineVec::from_slice(&[0]);
    f.s.available_markers.retain(|m| *m != 0);
    f.s.player_states[0].captives[1] = 2;
    f.s.player_states[1].agents_supply -= 2;
    let agents_before = f.s.player_states[1].agents_supply;

    end_chapter_now(&mut f);

    assert_eq!(f.s.player_states[0].captives[1], 0);
    assert_eq!(f.s.player_states[1].agents_supply, agents_before + 2);
    assert_eq!(f.s.player_states[0].power, 5);
}

// Trophies and captives stay put when their ambition did not score.
#[test]
fn unscored_ambitions_keep_trophies_and_captives() {
    let mut f = start_game(3, 75, 0);
    // Tycoon scores; Warlord and Tyrant were never declared.
    f.s.declared[AmbitionId::Tycoon.as_index()] = arcs_engine::InlineVec::from_slice(&[0]);
    f.s.available_markers.retain(|m| *m != 0);
    f.s.player_states[0].trophies[1][TrophyKind::Ship.as_index()] = 2;
    f.s.player_states[0].captives[1] = 1;

    end_chapter_now(&mut f);

    assert_eq!(
        f.s.player_states[0].trophies[1][TrophyKind::Ship.as_index()],
        2
    );
    assert_eq!(f.s.player_states[0].captives[1], 1);
}

// A returning city covers resource slots again; surplus tokens go back to
// the supply (p7, p17 "you must discard resources you cannot hold").
#[test]
fn a_returned_city_covers_slots_and_discards_the_surplus() {
    let mut f = start_game(3, 76, 0);
    declare_warlord(&mut f.s, &[0]);
    // Player 1 built a city (4 open slots), then lost it to player 0.
    f.s.player_states[1].cities_used = 1;
    f.s.player_states[0].trophies[1][TrophyKind::City.as_index()] = 1;
    f.s.player_states[1].resources = [None; 6];
    for slot in 0..4 {
        f.s.player_states[1].resources[slot] = Some(ResourceType::Fuel);
    }
    let fuel_before = f.s.supply[ResourceType::Fuel.as_index()];

    end_chapter_now(&mut f);

    assert_eq!(f.s.player_states[1].cities_used, 0);
    assert_eq!(f.s.player_states[1].held_resources().len(), 3);
    assert_eq!(f.s.supply[ResourceType::Fuel.as_index()], fuel_before + 1);
}

// Guild cards survive chapter end and keep counting next chapter.
#[test]
fn guild_cards_persist_across_chapters() {
    let mut f = start_game(3, 77, 0);
    let card = CourtCardId(20); // Loyal Keepers, Relic suit
    f.s.player_states[0].guild_cards.push(card);
    let at = f.s.court_deck.position(&card);
    if let Some(at) = at {
        f.s.court_deck.remove(at); // keep the court multiset consistent
    }
    end_chapter_now(&mut f);
    assert!(f.s.player_states[0].guild_cards.contains(&card));
}
