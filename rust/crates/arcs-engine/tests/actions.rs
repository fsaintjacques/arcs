//! Ported from `tests/rules.test.ts` "standard actions (p12-p13)", plus the
//! catapult rules (p13) the TS suite only exercises through fuzzing. Each
//! test cites the TS test name.

mod common;

use arcs_engine::state::TrophyKind;
use arcs_engine::{Action, BuildingKind, Phase, Player, SystemId};
use common::*;

// TS: "taxing a Loyal city gains its planet type"
#[test]
fn taxing_a_loyal_city_gains_its_planet_type() {
    let mut f = start_game(3, 41, 0);
    let player = turn_with(&mut f, ADMIN, 2);
    let tax = find(&actions(&f), |a| matches!(a, Action::Tax { .. }));
    let before = f.s.player(player).held_resources().len();
    apply(&mut f, tax);
    assert_eq!(f.s.player(player).held_resources().len(), before + 1);
}

// TS: "cannot tax the same city twice in a turn (p12)"
#[test]
fn cannot_tax_the_same_city_twice_in_a_turn() {
    let mut f = start_game(3, 42, 0);
    turn_with(&mut f, ADMIN, 2);
    let tax = find(&actions(&f), |a| matches!(a, Action::Tax { .. }));
    apply(&mut f, tax);
    assert!(
        !actions(&f).contains(&tax),
        "the same city was offered again"
    );
}

// TS: "taxing a Rival city you control captures one of their agents"
#[test]
fn taxing_a_rival_city_you_control_captures_an_agent() {
    let mut f = start_game(3, 43, 0);
    let player = actor(&f);
    let victim = Player((player.0 + 1) % 3);
    // Find the victim's city and park a controlling fleet on it.
    let system = (0..f.s.systems.len())
        .find(|&i| {
            f.s.systems[i]
                .buildings
                .iter()
                .any(|b| b.player() == victim && b.kind() == BuildingKind::City)
        })
        .expect("victim has a city from setup");
    f.s.systems[system].fresh[player.as_index()] = 9;
    set_hand(&mut f, player, &[card_id(ADMIN, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(ADMIN, 2),
        },
    );
    apply(&mut f, Action::BeginActions);

    assert_eq!(f.s.control_of(SystemId(system as u8)), Some(player));
    let building = f.s.systems[system]
        .buildings
        .iter()
        .position(|b| b.player() == victim)
        .unwrap();
    let agents_before = f.s.player(victim).agents_supply;
    apply(
        &mut f,
        Action::Tax {
            system: SystemId(system as u8),
            building: building as u8,
        },
    );
    assert_eq!(f.s.player(victim).agents_supply, agents_before - 1);
    assert_eq!(f.s.player(player).captives[victim.as_index()], 1);
}

// TS: "building in a system someone else controls places the piece damaged
// (p12)"
#[test]
fn building_in_a_rival_controlled_system_places_the_piece_damaged() {
    let mut f = start_game(3, 44, 0);
    let player = actor(&f);
    let rival = Player((player.0 + 1) % 3);
    // Any in-play planet with a free building slot; put a ship of the
    // player's there so they may build, and enough rival ships that the
    // rival controls it.
    let system = (0..f.s.systems.len())
        .find(|&i| {
            let def = &f.v.systems[i];
            def.kind == arcs_engine::map::SystemKind::Planet
                && !def.adjacent.is_empty()
                && f.s.systems[i].buildings.len() < def.building_slots as usize
        })
        .expect("a planet with a free slot");
    f.s.systems[system].fresh[player.as_index()] += 1;
    f.s.systems[system].fresh[rival.as_index()] = 9; // rival now controls it
    set_hand(&mut f, player, &[card_id(CONSTRUCTION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 2),
        },
    );
    apply(&mut f, Action::BeginActions);

    let before = f.s.systems[system].buildings.len();
    let build = find(
        &actions(&f),
        |a| matches!(a, Action::BuildBuilding { system: sys, .. } if sys.as_index() == system),
    );
    apply(&mut f, build);
    let placed = f.s.systems[system].buildings.as_slice()[before];
    assert_eq!(placed.player(), player);
    assert!(placed.damaged());
}

// TS: "a starport builds only one ship per turn (p12)"
#[test]
fn a_starport_builds_only_one_ship_per_turn() {
    let mut f = start_game(3, 45, 0);
    turn_with(&mut f, CONSTRUCTION, 2); // 4 pips
    let first = find(&actions(&f), |a| matches!(a, Action::BuildShip { .. }));
    apply(&mut f, first);
    assert!(
        !actions(&f).contains(&first),
        "the same starport was offered again"
    );
}

// TS: "influence puts an agent in the Court, secure needs a strict majority
// (p13)"
#[test]
fn influence_places_an_agent_and_secure_needs_a_strict_majority() {
    let mut f = start_game(3, 46, 0);
    // Aggression buys Secure.
    let player = turn_with(&mut f, AGGRESSION, 2);
    f.s.court.as_mut_slice()[0].agents[player.as_index()] = 1;
    f.s.player_mut(player).agents_supply -= 1;

    // A rival matching the count blocks the Secure.
    let rival = Player((player.0 + 1) % 3);
    f.s.court.as_mut_slice()[0].agents[rival.as_index()] = 1;
    assert!(
        !actions(&f)
            .iter()
            .any(|a| matches!(a, Action::Secure { slot: 0 }))
    );
    f.s.court.as_mut_slice()[0].agents[rival.as_index()] = 0;
    assert!(
        actions(&f)
            .iter()
            .any(|a| matches!(a, Action::Secure { slot: 0 }))
    );
}

// TS: "securing captures Rival agents and refills the slot"
#[test]
fn securing_captures_rival_agents_and_refills_the_slot() {
    let mut f = start_game(3, 47, 0);
    let player = turn_with(&mut f, AGGRESSION, 2);
    let rival = Player((player.0 + 1) % 3);
    let card = f.s.court.as_slice()[0].card;
    f.s.court.as_mut_slice()[0].agents[player.as_index()] = 2;
    f.s.court.as_mut_slice()[0].agents[rival.as_index()] = 1;

    apply(&mut f, Action::Secure { slot: 0 });
    assert_eq!(f.s.player(player).captives[rival.as_index()], 1);
    assert_ne!(f.s.court.as_slice()[0].card, card);
    assert!(f.s.court.as_slice()[0].agents.iter().all(|&n| n == 0));
}

// TS: "repair stands a damaged ship back up (p13)"
#[test]
fn repair_stands_a_damaged_ship_back_up() {
    let mut f = start_game(3, 48, 0);
    let player = actor(&f);
    let system = (0..f.s.systems.len())
        .find(|&i| f.s.systems[i].fresh[player.as_index()] > 0)
        .unwrap();
    f.s.systems[system].fresh[player.as_index()] -= 1;
    f.s.systems[system].damaged[player.as_index()] += 1;
    set_hand(&mut f, player, &[card_id(ADMIN, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(ADMIN, 2),
        },
    );
    apply(&mut f, Action::BeginActions);

    apply(
        &mut f,
        Action::Repair {
            system: SystemId(system as u8),
            building: None,
        },
    );
    assert_eq!(f.s.systems[system].damaged[player.as_index()], 0);
}

// TS: "move only reaches adjacent in-play systems"
#[test]
fn move_only_reaches_adjacent_in_play_systems() {
    let mut f = start_game(3, 49, 0);
    turn_with(&mut f, MOBILIZATION, 2);
    let mut moves = 0;
    for a in actions(&f) {
        let Action::Move { from, to, .. } = a else {
            continue;
        };
        moves += 1;
        assert!(f.v.systems[from.as_index()].adjacent.contains(&to));
        assert!(!f.s.systems[to.as_index()].out_of_play);
    }
    assert!(moves > 0);
}

// TS: "a Fuel buys a Move even from a Construction card" — the full version
// (the R1 suite could only check the grant, not the enumerated moves).
#[test]
fn a_fuel_buys_a_move_even_from_a_construction_card() {
    let mut f = start_game(3, 51, 0);
    let player = actor(&f);
    f.s.player_mut(player).resources[0] = Some(arcs_engine::ResourceType::Fuel);
    set_hand(&mut f, player, &[card_id(CONSTRUCTION, 5)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 5),
        },
    );
    apply(&mut f, Action::SpendResource { slot: 0 });
    apply(&mut f, Action::BeginActions);
    assert!(actions(&f).iter().any(|a| matches!(a, Action::Move { .. })));
}

// --- catapult (p13) --------------------------------------------------------

/// A player, their starport planet and the adjacent gate, cleared of rivals
/// and on the player's turn with Move actions.
fn catapult_setup(seed: u64) -> (Fixture, Player, SystemId, SystemId) {
    let mut f = start_game(3, seed, 0);
    let player = actor(&f);
    let from = (0..f.s.systems.len())
        .find(|&i| {
            f.s.systems[i]
                .buildings
                .iter()
                .any(|b| b.player() == player && b.kind() == BuildingKind::Starport)
        })
        .expect("the player starts with a starport");
    let from = SystemId(from as u8);
    // The cluster's gate is adjacent to every planet in it.
    let gate = arcs_engine::map::gate_id(arcs_engine::map::cluster_of(from));
    assert!(f.v.systems[from.as_index()].adjacent.contains(&gate));
    // Clear the gate so nobody controls it.
    for p in 0..3 {
        f.s.systems[gate.as_index()].fresh[p] = 0;
        f.s.systems[gate.as_index()].damaged[p] = 0;
    }
    set_hand(&mut f, player, &[card_id(MOBILIZATION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(MOBILIZATION, 2),
        },
    );
    apply(&mut f, Action::BeginActions);
    (f, player, from, gate)
}

// "Move any number of Loyal ships from a system with a Loyal starport ...
// they may keep moving" (p13): leaving a Loyal starport onto an
// uncontrolled gate opens the catapult decision.
#[test]
fn a_starport_move_onto_an_uncontrolled_gate_opens_a_catapult() {
    let (mut f, player, from, gate) = catapult_setup(61);
    apply(
        &mut f,
        Action::Move {
            from,
            to: gate,
            ships: 2,
        },
    );
    assert_eq!(f.s.phase, Phase::Catapult);
    let list = actions(&f);
    assert!(list.contains(&Action::CatapultStop));
    // Engine ruling (game.ts header): a catapult never revisits a system.
    for a in &list {
        if let Action::Catapult { to, .. } = a {
            assert_ne!(*to, from, "offered a revisit of the origin");
            assert_ne!(*to, gate, "offered a revisit of the current stop");
        }
    }
    // Stopping ends the move; the turn continues with its remaining pips.
    apply(&mut f, Action::CatapultStop);
    assert_eq!(f.s.phase, Phase::Actions);
    assert_eq!(f.s.systems[gate.as_index()].fresh[player.as_index()], 2);
    assert!(f.s.turn.is_some());
}

// "until they move to a gate controlled by anyone else" (p13): check for
// control before moving in, so the arriving ships cannot clear the blockade
// themselves.
#[test]
fn a_rival_controlled_gate_stops_the_catapult() {
    let (mut f, player, from, gate) = catapult_setup(62);
    let rival = Player((player.0 + 1) % 3);
    f.s.systems[gate.as_index()].fresh[rival.as_index()] = 5;
    apply(
        &mut f,
        Action::Move {
            from,
            to: gate,
            ships: 2,
        },
    );
    assert_eq!(f.s.phase, Phase::Actions);
    assert_eq!(f.s.systems[gate.as_index()].fresh[player.as_index()], 2);
}

// "you can't Catapult from Rival starports you control" — and not from
// systems without a starport at all.
#[test]
fn moving_without_a_loyal_starport_never_catapults() {
    let mut f = start_game(3, 63, 0);
    let player = actor(&f);
    // The city start (position A) has ships but no starport.
    let from = (0..f.s.systems.len())
        .find(|&i| {
            f.s.systems[i].fresh[player.as_index()] > 0
                && !f.s.systems[i]
                    .buildings
                    .iter()
                    .any(|b| b.kind() == BuildingKind::Starport)
        })
        .expect("a starport-less system with ships");
    let from = SystemId(from as u8);
    let gate = arcs_engine::map::gate_id(arcs_engine::map::cluster_of(from));
    for p in 0..3 {
        f.s.systems[gate.as_index()].fresh[p] = 0;
        f.s.systems[gate.as_index()].damaged[p] = 0;
    }
    set_hand(&mut f, player, &[card_id(MOBILIZATION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(MOBILIZATION, 2),
        },
    );
    apply(&mut f, Action::BeginActions);
    apply(
        &mut f,
        Action::Move {
            from,
            to: gate,
            ships: 1,
        },
    );
    assert_eq!(f.s.phase, Phase::Actions);
}

// --- reinforce (p22) -------------------------------------------------------

// "Rarely, a player will have no starports or ships on the map ... they
// place 3 fresh ships in any gate at the end of their turn."
#[test]
fn a_wiped_out_player_reinforces_a_gate_at_end_of_turn() {
    let mut f = start_game(3, 71, 0);
    let player = actor(&f);
    set_hand(&mut f, player, &[card_id(ADMIN, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(ADMIN, 2),
        },
    );
    apply(&mut f, Action::BeginActions);

    // Wipe the player off the map mid-turn (their ships were destroyed).
    for i in 0..f.s.systems.len() {
        let lost =
            f.s.systems[i].fresh[player.as_index()] + f.s.systems[i].damaged[player.as_index()];
        f.s.systems[i].fresh[player.as_index()] = 0;
        f.s.systems[i].damaged[player.as_index()] = 0;
        f.s.player_mut(player).ships_supply += lost;
        f.s.systems[i]
            .buildings
            .retain(|b| !(b.player() == player && b.kind() == BuildingKind::Starport));
    }
    f.s.player_mut(player).starports_supply = 5;

    apply(&mut f, Action::EndTurn);
    assert_eq!(f.s.phase, Phase::Reinforce);
    assert_eq!(actor(&f), player);
    let list = actions(&f);
    assert!(!list.is_empty());
    // Every offer is an in-play gate.
    for a in &list {
        let Action::Reinforce { system } = a else {
            panic!("non-reinforce action offered")
        };
        assert!(arcs_engine::map::is_gate(*system));
        assert!(!f.s.systems[system.as_index()].out_of_play);
    }
    let Action::Reinforce { system } = list[0] else {
        unreachable!()
    };
    let supply_before = f.s.player(player).ships_supply;
    apply(&mut f, list[0]);
    assert_eq!(f.s.systems[system.as_index()].fresh[player.as_index()], 3);
    assert_eq!(f.s.player(player).ships_supply, supply_before - 3);
    assert_eq!(f.s.phase, Phase::Play);
}

// --- trophies keep counting (p18) ------------------------------------------

// The count-matrix form of trophies must count for Warlord exactly like the
// TS list (plan risk 2: nothing reads trophy order).
#[test]
fn trophy_counts_add_up_across_kinds() {
    let mut f = start_game(3, 72, 0);
    let p = f.s.player_mut(Player(0));
    p.trophies[1][TrophyKind::Ship.as_index()] = 2;
    p.trophies[2][TrophyKind::City.as_index()] = 1;
    p.trophies[1][TrophyKind::Agent.as_index()] = 3;
    assert_eq!(p.trophy_count(), 6);
    assert_eq!(
        arcs_engine::ambition_count(f.s.player(Player(0)), arcs_engine::AmbitionId::Warlord),
        6
    );
}
