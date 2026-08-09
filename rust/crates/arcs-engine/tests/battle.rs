//! Ported from `tests/rules.test.ts` "battle (p14-p16)", plus raiding and
//! Outrage/Ransack coverage the TS suite reaches through powers.test.ts and
//! fuzzing. Deterministic outcomes are injected through the public
//! [`arcs_engine::apply_battle_roll_mut`] (the plan's exact-expectation
//! seam), never by stubbing the RNG.

mod common;

use arcs_engine::dice::{DIE_FACES, RollTotals};
use arcs_engine::state::{Building, TrophyKind};
use arcs_engine::{
    Action, BuildingKind, DieType, HitTarget, Phase, Player, ResourceType, SystemId,
    apply_battle_roll_mut,
};
use common::*;

struct BattleFixture {
    f: Fixture,
    player: Player,
    rival: Player,
    system: SystemId,
}

/// Put an attacker and a defender in one system, on the attacker's turn
/// (`battleSetup` in rules.test.ts).
fn battle_setup(seed: u64, defender_building: bool) -> BattleFixture {
    let mut f = start_game(3, seed, 0);
    let player = actor(&f);
    let rival = Player((player.0 + 1) % 3);
    let system = (0..f.s.systems.len())
        .find(|&i| {
            f.v.systems[i].kind == arcs_engine::map::SystemKind::Planet
                && f.s.systems[i].fresh[player.as_index()] > 0
        })
        .expect("the attacker starts on a planet");
    let system = SystemId(system as u8);
    f.s.systems[system.as_index()].fresh[player.as_index()] = 4;
    f.s.systems[system.as_index()].fresh[rival.as_index()] = 2;
    if defender_building {
        f.s.systems[system.as_index()].buildings.push(Building::new(
            rival,
            BuildingKind::Starport,
            false,
        ));
    }
    set_hand(&mut f, player, &[card_id(AGGRESSION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(AGGRESSION, 2),
        },
    );
    apply(&mut f, Action::BeginActions);
    BattleFixture {
        f,
        player,
        rival,
        system,
    }
}

// TS: "never offers more dice than attacking ships"
#[test]
fn never_offers_more_dice_than_attacking_ships() {
    let bf = battle_setup(61, false);
    let ships = bf.f.s.systems[bf.system.as_index()].fresh[bf.player.as_index()];
    let mut battles = 0;
    for a in actions(&bf.f) {
        let Action::Battle {
            assault,
            skirmish,
            raid,
            ..
        } = a
        else {
            continue;
        };
        battles += 1;
        assert!(assault + skirmish + raid <= ships);
        assert!(assault + skirmish + raid > 0);
    }
    assert!(battles > 0);
}

// TS: "withholds raid dice unless the defender has a building here or none
// anywhere (p14)"
#[test]
fn withholds_raid_dice_unless_defending_buildings_or_none_anywhere() {
    // Defender has a city elsewhere (from setup) and nothing in this system.
    let bare = battle_setup(62, false);
    assert!(
        bare.f
            .s
            .systems
            .iter()
            .any(|sys| sys.buildings.iter().any(|b| b.player() == bare.rival))
    );
    assert!(
        !actions(&bare.f)
            .iter()
            .any(|a| matches!(a, Action::Battle { raid, .. } if *raid > 0))
    );

    // With a defending building present, raid dice are legal.
    let with_building = battle_setup(62, true);
    assert!(
        actions(&with_building.f)
            .iter()
            .any(|a| matches!(a, Action::Battle { raid, .. } if *raid > 0))
    );
}

// TS: "keeps the rolled faces on the table, consistent with the totals"
#[test]
fn keeps_the_rolled_faces_consistent_with_the_totals() {
    let mut bf = battle_setup(65, true);
    let act = find(
        &actions(&bf.f),
        |a| matches!(a, Action::Battle { raid, assault, .. } if *raid > 0 && *assault > 0),
    );
    let Action::Battle {
        assault,
        skirmish,
        raid,
        ..
    } = act
    else {
        unreachable!()
    };
    apply(&mut bf.f, act);
    settle(&mut bf.f); // roll

    let b = bf.f.s.battle.expect("battle in progress");
    assert_eq!(
        b.rolled[DieType::Assault.as_index()].len(),
        assault as usize
    );
    assert_eq!(
        b.rolled[DieType::Skirmish.as_index()].len(),
        skirmish as usize
    );
    assert_eq!(b.rolled[DieType::Raid.as_index()].len(), raid as usize);

    // Recompute every counter from the faces shown.
    let sum = |die: DieType, get: fn(&arcs_engine::dice::DieFace) -> u8| -> u8 {
        b.rolled[die.as_index()]
            .iter()
            .map(|&idx| get(&DIE_FACES[die.as_index()][idx as usize]))
            .sum()
    };
    use DieType::{Assault, Raid, Skirmish};
    let hits = sum(Assault, |f| f.hits) + sum(Skirmish, |f| f.hits) + sum(Raid, |f| f.hits);
    let building_hits = sum(Assault, |f| f.building_hits) + sum(Raid, |f| f.building_hits);
    let keys = sum(Assault, |f| f.keys) + sum(Raid, |f| f.keys);
    let self_hits = sum(Assault, |f| f.self_hits) + sum(Raid, |f| f.self_hits);
    let intercept = sum(Assault, |f| f.intercept) + sum(Raid, |f| f.intercept);

    assert_eq!(b.hits, hits);
    assert_eq!(b.building_hits, building_hits);
    assert_eq!(b.keys, keys);
    // Intercept converts into self-hits once, immediately (p14).
    let defender_fresh = bf.f.s.systems[bf.system.as_index()].fresh[bf.rival.as_index()];
    assert_eq!(
        b.self_hits,
        self_hits + if intercept > 0 { defender_fresh } else { 0 }
    );
    assert_eq!(b.intercept_resolved, intercept > 0);
}

// TS: "resolves a skirmish-only battle without ever hurting the attacker"
#[test]
fn a_skirmish_only_battle_never_hurts_the_attacker() {
    let mut bf = battle_setup(63, false);
    let attacker_before = bf.f.s.systems[bf.system.as_index()].fresh[bf.player.as_index()];
    apply(
        &mut bf.f,
        Action::Battle {
            system: bf.system,
            defender: bf.rival,
            assault: 0,
            skirmish: 4,
            raid: 0,
        },
    );
    settle(&mut bf.f); // roll
    let mut guard = 0;
    while bf.f.s.battle.is_some() && guard < 32 {
        guard += 1;
        let list = actions(&bf.f);
        apply(&mut bf.f, list[0]);
        settle(&mut bf.f);
    }
    let st = &bf.f.s.systems[bf.system.as_index()];
    assert_eq!(
        st.fresh[bf.player.as_index()] + st.damaged[bf.player.as_index()],
        attacker_before
    );
}

// TS: "damages a fresh ship and destroys a damaged one into Trophies" —
// deterministic here: two injected hits damage the fresh defender, then
// destroy it.
#[test]
fn a_hit_damages_a_fresh_ship_and_destroys_a_damaged_one() {
    let mut bf = battle_setup(64, false);
    bf.f.s.systems[bf.system.as_index()].fresh[bf.rival.as_index()] = 1;
    bf.f.s.systems[bf.system.as_index()].damaged[bf.rival.as_index()] = 0;
    apply(
        &mut bf.f,
        Action::Battle {
            system: bf.system,
            defender: bf.rival,
            assault: 2,
            skirmish: 0,
            raid: 0,
        },
    );
    assert_eq!(bf.f.s.phase, Phase::BattleRoll);
    apply_battle_roll_mut(
        &mut bf.f.s,
        &bf.f.v,
        RollTotals {
            hits: 2,
            ..RollTotals::default()
        },
    )
    .unwrap();

    assert_eq!(bf.f.s.phase, Phase::BattleAssign);
    let list = actions(&bf.f);
    assert_eq!(
        list,
        vec![Action::AssignHit {
            target: HitTarget::Ship { fresh: true }
        }]
    );
    apply(&mut bf.f, list[0]);
    assert_eq!(
        bf.f.s.systems[bf.system.as_index()].damaged[bf.rival.as_index()],
        1
    );

    apply(
        &mut bf.f,
        Action::AssignHit {
            target: HitTarget::Ship { fresh: false },
        },
    );
    assert_eq!(
        bf.f.s.systems[bf.system.as_index()].damaged[bf.rival.as_index()],
        0
    );
    assert_eq!(
        bf.f.s.player(bf.player).trophies[bf.rival.as_index()][TrophyKind::Ship.as_index()],
        1
    );
    // Nothing left to assign: the battle ended.
    assert!(bf.f.s.battle.is_none());
}

// TS: "an intercept converts into one self-hit per fresh defending ship
// (p14)"
#[test]
fn an_intercept_converts_into_one_self_hit_per_fresh_defender() {
    let mut bf = battle_setup(65, false);
    bf.f.s.systems[bf.system.as_index()].fresh[bf.rival.as_index()] = 3;
    apply(
        &mut bf.f,
        Action::Battle {
            system: bf.system,
            defender: bf.rival,
            assault: 1,
            skirmish: 0,
            raid: 0,
        },
    );
    // The assault face "1 hit + intercept".
    apply_battle_roll_mut(
        &mut bf.f.s,
        &bf.f.v,
        RollTotals {
            hits: 1,
            intercept: 1,
            ..RollTotals::default()
        },
    )
    .unwrap();
    let b = bf.f.s.battle.unwrap();
    assert!(b.intercept_resolved);
    // One self-hit per fresh defending ship, and the face still deals its
    // hit.
    assert_eq!(b.self_hits, 3);
    assert_eq!(b.hits, 1);
    // Self-hits are assigned before anything else.
    let list = actions(&bf.f);
    assert!(list.iter().all(|a| matches!(a, Action::AssignSelf { .. })));
}

// Hits spill onto buildings only once no defending ships remain (p14).
#[test]
fn hits_spill_onto_buildings_once_ships_are_gone() {
    let mut bf = battle_setup(66, true);
    bf.f.s.systems[bf.system.as_index()].fresh[bf.rival.as_index()] = 0;
    apply(
        &mut bf.f,
        Action::Battle {
            system: bf.system,
            defender: bf.rival,
            assault: 2,
            skirmish: 0,
            raid: 0,
        },
    );
    apply_battle_roll_mut(
        &mut bf.f.s,
        &bf.f.v,
        RollTotals {
            hits: 2,
            ..RollTotals::default()
        },
    )
    .unwrap();
    // No defending ships: the hits are offered against the starport.
    let building = find(&actions(&bf.f), |a| {
        matches!(
            a,
            Action::AssignHit {
                target: HitTarget::Building { .. }
            }
        )
    });
    apply(&mut bf.f, building);
    let st = &bf.f.s.systems[bf.system.as_index()];
    let b = st
        .buildings
        .iter()
        .find(|b| b.player() == bf.rival)
        .expect("still standing");
    assert!(b.damaged());
    // The second hit destroys it into the attacker's trophies.
    apply(&mut bf.f, building);
    assert!(
        !bf.f.s.systems[bf.system.as_index()]
            .buildings
            .iter()
            .any(|b| b.player() == bf.rival)
    );
    assert_eq!(
        bf.f.s.player(bf.player).trophies[bf.rival.as_index()][TrophyKind::Starport.as_index()],
        1
    );
}

// --- raiding (p16-p17) -----------------------------------------------------

/// A battle already at the raid step with `keys` keys.
fn raid_setup(seed: u64, keys: u8) -> BattleFixture {
    let mut bf = battle_setup(seed, true);
    apply(
        &mut bf.f,
        Action::Battle {
            system: bf.system,
            defender: bf.rival,
            assault: 0,
            skirmish: 0,
            raid: 2,
        },
    );
    apply_battle_roll_mut(
        &mut bf.f.s,
        &bf.f.v,
        RollTotals {
            keys,
            ..RollTotals::default()
        },
    )
    .unwrap();
    bf
}

// "Keys an attacker must spend to steal the resource in each slot"
// (p16-p17): slot 0 costs 1 key, the stolen token lands in the raider's
// slots directly.
#[test]
fn raiding_steals_a_resource_by_its_slot_cost() {
    let mut bf = raid_setup(81, 2);
    let victim_resources = bf.f.s.player(bf.rival).held_resources().len();
    assert!(victim_resources > 0, "the defender starts with resources");
    let slot0 = bf.f.s.player(bf.rival).resources[0].expect("slot 0 holds a token");
    let mine_before = bf.f.s.player(bf.player).held_resources().len();

    let list = actions(&bf.f);
    assert!(list.contains(&Action::RaidResource { slot: 0 }));
    apply(&mut bf.f, Action::RaidResource { slot: 0 });

    assert_eq!(bf.f.s.player(bf.rival).resources[0], None);
    let mine: Vec<ResourceType> =
        bf.f.s
            .player(bf.player)
            .held_resources()
            .iter()
            .copied()
            .collect();
    assert_eq!(mine.len(), mine_before + 1);
    assert!(mine.contains(&slot0));
    // 2 keys - slot cost 1 = 1 key left; the raid step continues.
    assert_eq!(bf.f.s.battle.unwrap().keys, 1);
}

// Slots further right cost more keys than the raider may have (p16-p17).
#[test]
fn a_raid_cannot_afford_a_slot_costlier_than_its_keys() {
    let mut bf = raid_setup(82, 1);
    let victim = bf.f.s.player_mut(bf.rival);
    victim.resources = [None; 6];
    victim.resources[2] = Some(ResourceType::Relic); // slot 2 costs 2 keys
    let list = actions(&bf.f);
    assert!(
        !list
            .iter()
            .any(|a| matches!(a, Action::RaidResource { .. }))
    );
}

// "In battle they can steal this first and then spend keys" (p20 context):
// Guild cards are raided at their printed raid cost.
#[test]
fn raiding_steals_a_guild_card_at_its_raid_cost() {
    let mut bf = raid_setup(83, 3);
    // Sworn Guardians has raid cost 1 (id 21).
    let card = arcs_engine::CourtCardId(21);
    bf.f.s.player_mut(bf.rival).guild_cards.push(card);
    let list = actions(&bf.f);
    assert!(list.contains(&Action::RaidCard { card }));
    apply(&mut bf.f, Action::RaidCard { card });
    assert!(bf.f.s.player(bf.player).guild_cards.contains(&card));
    assert!(!bf.f.s.player(bf.rival).guild_cards.contains(&card));
    assert_eq!(bf.f.s.battle.unwrap().keys, 2);
}

// "spend keys" is optional: raidDone forfeits the rest and ends the battle.
#[test]
fn raid_done_forfeits_the_remaining_keys() {
    let mut bf = raid_setup(84, 3);
    apply(&mut bf.f, Action::RaidDone);
    assert!(bf.f.s.battle.is_none());
    assert_eq!(bf.f.s.phase, Phase::Actions);
}

// --- outrage and ransack (p16) ---------------------------------------------

// Destroying a city provokes Outrage *against the destroyer* on the
// planet's type, and Ransacks the Court card holding the most of the
// victim's agents (ties leftmost — engine ruling, game.ts header).
#[test]
fn destroying_a_city_provokes_outrage_and_ransacks_the_court() {
    let mut f = start_game(3, 91, 0);
    let player = actor(&f);
    let rival = Player((player.0 + 1) % 3);
    // The victim's city, damaged already so one building hit destroys it.
    let system = (0..f.s.systems.len())
        .find(|&i| {
            f.s.systems[i]
                .buildings
                .iter()
                .any(|b| b.player() == rival && b.kind() == BuildingKind::City)
        })
        .expect("the rival starts with a city");
    let bi = f.s.systems[system]
        .buildings
        .iter()
        .position(|b| b.player() == rival)
        .unwrap();
    f.s.systems[system].buildings.as_mut_slice()[bi].set_damaged(true);
    let planet_type = f.v.systems[system]
        .planet_type
        .expect("cities sit on planets");
    let system = SystemId(system as u8);
    f.s.systems[system.as_index()].fresh[player.as_index()] = 4;
    // Give the destroyer a token of the planet's type: Outrage discards it.
    f.s.player_mut(player).resources = [None; 6];
    f.s.player_mut(player).resources[0] = Some(planet_type);
    let supply_before = f.s.supply[planet_type.as_index()];
    // Victim agents in the Court: most on slot 1, ties broken leftmost.
    f.s.court.as_mut_slice()[0].agents[rival.as_index()] = 1;
    f.s.court.as_mut_slice()[1].agents[rival.as_index()] = 2;
    let ransacked_card = f.s.court.as_slice()[1].card;

    set_hand(&mut f, player, &[card_id(AGGRESSION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(AGGRESSION, 2),
        },
    );
    apply(&mut f, Action::BeginActions);
    apply(
        &mut f,
        Action::Battle {
            system,
            defender: rival,
            assault: 0,
            skirmish: 0,
            raid: 1,
        },
    );
    apply_battle_roll_mut(
        &mut f.s,
        &f.v,
        RollTotals {
            building_hits: 1,
            ..RollTotals::default()
        },
    )
    .unwrap();
    apply(
        &mut f,
        Action::AssignHit {
            target: HitTarget::Building { building: bi as u8 },
        },
    );

    // The city is gone, into the destroyer's trophies.
    assert_eq!(
        f.s.player(player).trophies[rival.as_index()][TrophyKind::City.as_index()],
        1
    );
    // Outrage hit the destroyer: token discarded, marker flipped, agent
    // lost.
    assert!(f.s.player(player).outrage[planet_type.as_index()]);
    assert_eq!(f.s.player(player).resources[0], None);
    assert_eq!(f.s.supply[planet_type.as_index()], supply_before + 1);
    assert_eq!(f.s.player(player).agents_supply, 9);
    // Ransack took the card with the most victim agents (slot 1), its
    // agents as Trophies, not Captives.
    assert!(f.s.court.iter().all(|slot| slot.card != ransacked_card));
    assert_eq!(
        f.s.player(player).trophies[rival.as_index()][TrophyKind::Agent.as_index()],
        2
    );
    assert_eq!(f.s.player(player).captives[rival.as_index()], 0);
    assert_eq!(f.s.court.as_slice()[0].agents[rival.as_index()], 1);
}

// A secured Vox card resolves inert in R2 (R3: pendingVox) — securing one
// must not wedge the game or leak the card.
#[test]
fn securing_a_vox_card_discards_it_and_refills_the_slot() {
    let mut f = start_game(3, 92, 0);
    let player = turn_with(&mut f, AGGRESSION, 2);
    // Force a Vox card into slot 0.
    let vox = arcs_engine::CourtCardId(27); // Outrage Spreads
    let was_at_deck = f.s.court_deck.position(&vox);
    if let Some(at) = was_at_deck {
        let displaced = f.s.court.as_slice()[0].card;
        f.s.court_deck.remove(at);
        f.s.court_deck.push(displaced);
        f.s.court.as_mut_slice()[0].card = vox;
    } else {
        f.s.court.as_mut_slice()[0].card = vox; // already in the row somewhere
    }
    f.s.court.as_mut_slice()[0].agents[player.as_index()] = 1;

    apply(&mut f, Action::Secure { slot: 0 });
    assert!(f.s.court_discard.contains(&vox));
    assert!(f.s.player(player).guild_cards.is_empty());
    assert_ne!(f.s.court.as_slice()[0].card, vox);
}
