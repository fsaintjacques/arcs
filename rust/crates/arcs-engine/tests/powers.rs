//! Guild card abilities: the ones the engine dispatches, ported
//! case-for-case from `tests/powers.test.ts` (every TS test name is cited
//! above its port). The `observe`/`determinize` assertions of the Farseers
//! reveal test wait for R4, noted inline.

mod common;

use arcs_engine::court::{
    COURT_DECK, CourtCardKind, PowerStatus, court_card, unimplemented_powers,
};
use arcs_engine::game::{apply_action_mut, get_pending, legal_actions, resolve_chance_mut};
use arcs_engine::powers::{cartel_icons, discard_guild_card, gain_guild_card};
use arcs_engine::state::{ActionKindSet, Building, CourtSlot, TrophyKind};
use arcs_engine::{
    Action, ActionKind, AmbitionId, BuildingKind, CardActionChoice, CourtCardId, Pending, Phase,
    Player, PreludeChoice, ResourceType, SystemId, VoxChoice, ambition_count, ambition_count_with,
    survives_outrage, weapon_icons,
};
use common::*;

use ResourceType::{Fuel, Material, Relic, Weapon};

fn grant_free_secure(f: &mut Fixture) {
    let mut turn = f.s.turn.expect("a turn in progress");
    turn.free_actions
        .push(ActionKindSet::from_kinds(&[ActionKind::Secure]));
    f.s.turn = Some(turn);
}

// ---------------------------------------------------------------------------
// power status bookkeeping
// ---------------------------------------------------------------------------

// TS: "classifies every card that carries an ability" + "gives every Guild
// card with printed text an engine-readable power". The Rust POWER_STATUS is
// positional, so classification coverage is structural; the power/vox split
// still needs asserting.
#[test]
fn every_card_with_printed_text_has_an_engine_readable_ability() {
    for c in COURT_DECK.iter() {
        match c.kind {
            CourtCardKind::Guild => assert!(c.power.is_some(), "{}", c.name),
            CourtCardKind::Vox => assert!(c.vox.is_some(), "{}", c.name),
        }
    }
}

// TS: "dispatches every printed ability in the deck" + "splits the deck into
// the implemented set and its complement" + "names what is missing for
// anything not fully dispatched". The whole Court is live; if an ability is
// ever rolled back, this fails and names it rather than letting the docs
// quietly go stale.
#[test]
fn dispatches_every_printed_ability_in_the_deck() {
    let missing: Vec<&str> = unimplemented_powers().map(|c| c.name).collect();
    assert_eq!(missing, Vec::<&str>::new());
    let with_ability = COURT_DECK
        .iter()
        .filter(|c| c.power.is_some() || c.vox.is_some())
        .count();
    assert_eq!(with_ability, 31);
    for c in COURT_DECK.iter() {
        assert_eq!(
            arcs_engine::court::POWER_STATUS[c.id.as_index()],
            PowerStatus::Full,
            "{}",
            c.name
        );
    }
}

// ---------------------------------------------------------------------------
// Loyal cards
// ---------------------------------------------------------------------------

// TS: "let any resource be spent as their type"
#[test]
fn loyal_cards_let_any_resource_be_spent_as_their_type() {
    let (mut f, player) = turn_holding(301, &["Loyal Keepers"], CONSTRUCTION, 2);
    f.s.player_states[player.as_index()].resources = [None; 6];
    f.s.player_states[player.as_index()].resources[0] = Some(Fuel);

    let offered = actions(&f);
    assert!(
        offered
            .iter()
            .any(|a| matches!(a, Action::SpendResourceAs { spend_as, .. } if *spend_as == Relic))
    );

    // Spending Fuel as a Relic buys a Secure, which Fuel alone would not.
    let act = find(
        &offered,
        |a| matches!(a, Action::SpendResourceAs { spend_as, .. } if *spend_as == Relic),
    );
    apply(&mut f, act);
    apply(&mut f, Action::BeginActions);
    let turn = f.s.turn.expect("turn");
    assert!(
        turn.free_actions
            .iter()
            .any(|g| g.contains(ActionKind::Secure))
    );
}

// TS: "returns the real token to the supply, not the type it was spent as"
#[test]
fn loyal_spend_returns_the_real_token_to_the_supply() {
    let (mut f, player) = turn_holding(302, &["Loyal Keepers"], CONSTRUCTION, 2);
    f.s.player_states[player.as_index()].resources = [None; 6];
    f.s.player_states[player.as_index()].resources[0] = Some(Fuel);
    let fuel_before = f.s.supply[Fuel.as_index()];
    let relic_before = f.s.supply[Relic.as_index()];

    let act = find(
        &actions(&f),
        |a| matches!(a, Action::SpendResourceAs { spend_as, .. } if *spend_as == Relic),
    );
    apply(&mut f, act);
    apply(&mut f, Action::BeginActions);

    assert_eq!(f.s.supply[Fuel.as_index()], fuel_before + 1);
    assert_eq!(f.s.supply[Relic.as_index()], relic_before);
}

// TS: "survive the Outrage discard of their own suit"
#[test]
fn loyal_cards_survive_the_outrage_discard_of_their_own_suit() {
    // Both are Relic cards, so Outraging Relic would discard both — but
    // "If you Provoke Outrage, keep this card" exempts the Loyal one.
    assert_eq!(court_card(court("Loyal Keepers")).suit, Some(Relic));
    assert_eq!(court_card(court("Relic Fence")).suit, Some(Relic));
    assert!(survives_outrage(court("Loyal Keepers")));
    assert!(!survives_outrage(court("Relic Fence")));
}

// ---------------------------------------------------------------------------
// Gatekeepers
// ---------------------------------------------------------------------------

// TS: "collects 2 extra battle dice in a gate but not on a planet"
#[test]
fn gatekeepers_collects_2_extra_dice_in_a_gate_but_not_on_a_planet() {
    let count_dice = |holding: &[&str], on_gate: bool| -> u8 {
        let (mut f, player) = turn_holding(311, holding, AGGRESSION, 2);
        let rival = Player((player.0 + 1) % 3);
        let system = (0..f.s.systems.len())
            .find(|&i| {
                let is_gate = f.v.systems[i].kind == arcs_engine::map::SystemKind::Gate;
                is_gate == on_gate && !f.s.systems[i].out_of_play
            })
            .expect("a matching system");
        let system = SystemId(system as u8);
        f.s.systems[system.as_index()].fresh[player.as_index()] = 2;
        f.s.systems[system.as_index()].fresh[rival.as_index()] = 1;
        apply(&mut f, Action::BeginActions);
        actions(&f)
            .iter()
            .filter_map(|a| match a {
                Action::Battle {
                    system: sys,
                    assault,
                    ..
                } if *sys == system => Some(*assault),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    };
    assert_eq!(count_dice(&["Gatekeepers"], true), 4); // 2 ships + 2
    assert_eq!(count_dice(&[], true), 2);
    assert_eq!(count_dice(&["Gatekeepers"], false), 2); // no bonus off a gate
}

// TS: "places a ship in every in-play gate for its Prelude"
#[test]
fn gatekeepers_places_a_ship_in_every_in_play_gate() {
    let (mut f, player) = turn_holding(312, &["Gatekeepers"], CONSTRUCTION, 2);
    let gates: Vec<usize> =
        f.v.systems
            .iter()
            .filter(|d| {
                d.kind == arcs_engine::map::SystemKind::Gate
                    && !f.s.systems[d.id.as_index()].out_of_play
            })
            .map(|d| d.id.as_index())
            .collect();
    let before: Vec<u8> = gates
        .iter()
        .map(|&i| f.s.systems[i].fresh[player.as_index()])
        .collect();

    let act = find(
        &actions(&f),
        |a| matches!(a, Action::CardPrelude { card, .. } if *card == court("Gatekeepers")),
    );
    apply(&mut f, act);

    for (i, &g) in gates.iter().enumerate() {
        assert_eq!(f.s.systems[g].fresh[player.as_index()], before[i] + 1);
    }
    assert!(
        !f.s.player(player)
            .guild_cards
            .contains(&court("Gatekeepers"))
    );
}

// ---------------------------------------------------------------------------
// Secret Order
// ---------------------------------------------------------------------------

// TS: "keeps the lead card number when declaring Keeper or Empath"
#[test]
fn secret_order_keeps_the_lead_number_for_keeper_and_empath() {
    let (mut f, _) = turn_holding(321, &["Secret Order"], CONSTRUCTION, 5); // "5" = Keeper
    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Keeper,
        },
    );
    assert_eq!(f.s.round.lead_number, 5);
}

// TS: "still zeroes the card for other ambitions"
#[test]
fn secret_order_still_zeroes_the_card_for_other_ambitions() {
    let (mut f, _) = turn_holding(322, &["Secret Order"], CONSTRUCTION, 4); // "4" = Warlord
    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Warlord,
        },
    );
    assert_eq!(f.s.round.lead_number, 0);
}

// TS: "and without it, Keeper zeroes the card as normal"
#[test]
fn without_secret_order_keeper_zeroes_the_card_as_normal() {
    let (mut f, _) = turn_holding(323, &[], CONSTRUCTION, 5);
    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Keeper,
        },
    );
    assert_eq!(f.s.round.lead_number, 0);
}

// ---------------------------------------------------------------------------
// Sworn Guardians
// ---------------------------------------------------------------------------

// TS: "is the only thing a raider may take"
#[test]
fn sworn_guardians_is_the_only_thing_a_raider_may_take() {
    let (mut f, player) = turn_holding(331, &[], AGGRESSION, 2);
    let victim = Player((player.0 + 1) % 3);
    {
        let vs = &mut f.s.player_states[victim.as_index()];
        vs.guild_cards = [court("Sworn Guardians"), court("Relic Fence")]
            .into_iter()
            .collect();
        vs.resources = [None; 6];
        vs.resources[0] = Some(Fuel);
    }

    let system = (0..f.s.systems.len())
        .find(|&i| {
            f.v.systems[i].kind == arcs_engine::map::SystemKind::Planet
                && !f.s.systems[i].out_of_play
        })
        .expect("a planet");
    let system = SystemId(system as u8);
    f.s.systems[system.as_index()].fresh[player.as_index()] = 3;
    f.s.systems[system.as_index()].fresh[victim.as_index()] = 1;
    apply(&mut f, Action::BeginActions);

    f.s.battle = Some(battle_state(system, player, victim, 3));
    f.s.phase = Phase::BattleAssign;

    let raidable: Vec<Action> = actions(&f)
        .into_iter()
        .filter(|a| matches!(a, Action::RaidResource { .. } | Action::RaidCard { .. }))
        .collect();
    assert_eq!(
        raidable,
        vec![Action::RaidCard {
            card: court("Sworn Guardians")
        }]
    );
}

// ---------------------------------------------------------------------------
// new actions
// ---------------------------------------------------------------------------

// TS: "Mining Interest adds Manufacture (Build): gain 1 Material"
#[test]
fn mining_interest_adds_manufacture_gain_1_material() {
    let (mut f, player) = turn_holding(341, &["Mining Interest"], CONSTRUCTION, 2);
    apply(&mut f, Action::BeginActions);
    let act = find(&actions(&f), |a| {
        matches!(
            a,
            Action::CardAction {
                choice: CardActionChoice::Manufacture,
                ..
            }
        )
    });

    let count_material = |f: &Fixture| {
        f.s.player(player)
            .resources
            .iter()
            .flatten()
            .filter(|&&r| r == Material)
            .count()
    };
    let before = count_material(&f);
    let supply = f.s.supply[Material.as_index()];
    let pips = f.s.turn.unwrap().pips_left;
    apply(&mut f, act);

    assert_eq!(count_material(&f), before + 1);
    assert_eq!(f.s.supply[Material.as_index()], supply - 1);
    assert_eq!(f.s.turn.unwrap().pips_left, pips - 1); // paid with a Build pip
}

// TS: "is not offered from a suit that cannot buy the action it replaces"
#[test]
fn card_actions_are_not_offered_from_a_suit_that_cannot_pay() {
    let (mut f, _) = turn_holding(342, &["Mining Interest"], AGGRESSION, 2); // no Build pips
    apply(&mut f, Action::BeginActions);
    assert!(!actions(&f).iter().any(|a| matches!(
        a,
        Action::CardAction {
            choice: CardActionChoice::Manufacture,
            ..
        }
    )));
}

// ---------------------------------------------------------------------------
// Prelude abilities
// ---------------------------------------------------------------------------

// TS: "Lattice Spies seizes the initiative without burning a card"
#[test]
fn lattice_spies_seizes_the_initiative_without_burning_a_card() {
    let mut f = start_game(3, 351, 0);
    let leader = actor(&f);
    set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 2),
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    let follower = actor(&f);
    f.s.player_states[follower.as_index()].guild_cards =
        [court("Lattice Spies")].into_iter().collect();
    set_hand(&mut f, follower, &[card_id(CONSTRUCTION, 6)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(CONSTRUCTION, 6),
            mode: arcs_engine::FollowMode::Surpass,
        },
    );

    let act = find(
        &actions(&f),
        |a| matches!(a, Action::CardPrelude { card, .. } if *card == court("Lattice Spies")),
    );
    apply(&mut f, act);
    assert_eq!(f.s.round.seized_by, Some(follower));
    assert!(f.s.player(follower).hand.is_empty()); // no card burned
}

// TS: "Silver-Tongues steals a named resource from a named Rival"
#[test]
fn silver_tongues_steals_a_named_resource_from_a_named_rival() {
    let (mut f, player) = turn_holding(352, &["Silver-Tongues"], CONSTRUCTION, 2);
    let victim = Player((player.0 + 1) % 3);
    f.s.player_states[victim.as_index()].resources = [None; 6];
    f.s.player_states[victim.as_index()].resources[0] = Some(Relic);
    f.s.player_states[player.as_index()].resources = [None; 6];

    let act = find(&actions(&f), |a| {
        matches!(
            a,
            Action::CardPrelude {
                card,
                choice: PreludeChoice::StealResource { target, .. },
            } if *card == court("Silver-Tongues") && *target == victim
        )
    });
    apply(&mut f, act);
    assert!(
        f.s.player(player)
            .resources
            .iter()
            .flatten()
            .any(|&r| r == Relic)
    );
    assert_eq!(f.s.player(victim).held_resources().len(), 0);
}

// TS: "Relic Fence trades a resource for a Relic and stays in play, once
// per turn"
#[test]
fn relic_fence_converts_once_per_turn_and_stays_in_play() {
    let (mut f, player) = turn_holding(353, &["Relic Fence"], CONSTRUCTION, 2);
    {
        let p = &mut f.s.player_states[player.as_index()];
        p.resources = [None; 6];
        p.resources[0] = Some(Fuel);
        p.resources[1] = Some(Material);
    }

    let act = find(
        &actions(&f),
        |a| matches!(a, Action::CardPrelude { card, .. } if *card == court("Relic Fence")),
    );
    apply(&mut f, act);
    let p = f.s.player(player);
    assert!(p.resources.iter().flatten().any(|&r| r == Relic));
    assert!(p.guild_cards.contains(&court("Relic Fence"))); // kept, not discarded
    // Once per turn.
    assert!(
        !actions(&f).iter().any(
            |a| matches!(a, Action::CardPrelude { card, .. } if *card == court("Relic Fence"))
        )
    );
}

// TS: "Shipping Interest fills every empty slot with Fuel"
#[test]
fn shipping_interest_fills_every_empty_slot_with_fuel() {
    let (mut f, player) = turn_holding(354, &["Shipping Interest"], CONSTRUCTION, 2);
    {
        let p = &mut f.s.player_states[player.as_index()];
        p.resources = [None; 6];
        p.resources[0] = Some(Material);
    }

    let act = find(
        &actions(&f),
        |a| matches!(a, Action::CardPrelude { card, .. } if *card == court("Shipping Interest")),
    );
    apply(&mut f, act);
    let p = f.s.player(player);
    let fuel = p.resources.iter().flatten().filter(|&&r| r == Fuel).count();
    assert!(fuel > 0);
    // Every open slot is filled unless the supply ran out first.
    let open = p.open_resource_slots();
    let empty = p
        .resources
        .iter()
        .take(open)
        .filter(|r| r.is_none())
        .count();
    assert!(empty == 0 || f.s.supply[Fuel.as_index()] == 0);
}

// TS: "\"place 3 ships\" only offers systems you control"
#[test]
fn place_3_ships_only_offers_systems_you_control() {
    let (f, player) = turn_holding(355, &["Loyal Marines"], CONSTRUCTION, 2);
    let offered: Vec<SystemId> = actions(&f)
        .iter()
        .filter_map(|a| match a {
            Action::CardPrelude {
                card,
                choice: PreludeChoice::System(system),
            } if *card == court("Loyal Marines") => Some(*system),
            _ => None,
        })
        .collect();
    assert!(!offered.is_empty());
    for system in offered {
        assert_eq!(f.s.control_of(system), Some(player));
    }
}

// TS: "cannot be used on a card secured in the same Prelude (p20)"
#[test]
fn cannot_use_a_prelude_on_a_card_secured_in_the_same_prelude() {
    let (mut f, player) = turn_holding(356, &["Silver-Tongues"], CONSTRUCTION, 2);
    let victim = Player((player.0 + 1) % 3);
    f.s.player_states[victim.as_index()].resources[0] = Some(Relic);
    let mut turn = f.s.turn.unwrap();
    turn.secured_this_prelude.push(court("Silver-Tongues"));
    f.s.turn = Some(turn);
    assert!(
        !actions(&f)
            .iter()
            .any(|a| matches!(a, Action::CardPrelude { .. }))
    );
}

// ---------------------------------------------------------------------------
// the Cartels
// ---------------------------------------------------------------------------

// TS: "takes its type's whole supply onto the card when acquired"
#[test]
fn a_cartel_takes_its_types_whole_supply_onto_the_card() {
    let (mut f, player) = turn_holding(371, &[], CONSTRUCTION, 2);
    let before = f.s.supply[Material.as_index()];
    assert!(before > 0);
    gain_guild_card(&mut f.s, player, court("Material Cartel"));

    assert_eq!(f.s.supply[Material.as_index()], 0);
    assert_eq!(f.s.cartel[Material.as_index()], before);
}

// TS: "counts toward Tycoon but cannot be spent"
#[test]
fn a_cartel_counts_toward_tycoon_but_cannot_be_spent() {
    let (mut f, player) = turn_holding(372, &[], CONSTRUCTION, 2);
    f.s.player_states[player.as_index()].resources = [None; 6];
    gain_guild_card(&mut f.s, player, court("Material Cartel"));

    // The stockpile is on the card, not in a slot, so there is nothing to
    // spend.
    let p = f.s.player(player);
    assert_eq!(p.held_resources().len(), 0);
    assert!(
        !actions(&f)
            .iter()
            .any(|a| matches!(a, Action::SpendResource { .. }))
    );
    // But it counts: the card's own suit icon plus every token on it.
    assert_eq!(
        ambition_count_with(
            f.s.player(player),
            AmbitionId::Tycoon,
            &cartel_icons(&f.s, player)
        ),
        1 + f.s.cartel[Material.as_index()]
    );
}

// TS: "routes returned tokens onto the card instead of the supply"
#[test]
fn a_cartel_routes_returned_tokens_onto_the_card() {
    let (mut f, player) = turn_holding(373, &[], CONSTRUCTION, 2);
    gain_guild_card(&mut f.s, player, court("Fuel Cartel"));
    let held = f.s.cartel[Fuel.as_index()];
    f.s.return_to_supply(Fuel);

    assert_eq!(f.s.supply[Fuel.as_index()], 0);
    assert_eq!(f.s.cartel[Fuel.as_index()], held + 1);
}

// TS: "releases the stockpile when the card leaves play"
#[test]
fn a_cartel_releases_the_stockpile_when_the_card_leaves_play() {
    let (mut f, player) = turn_holding(374, &[], CONSTRUCTION, 2);
    gain_guild_card(&mut f.s, player, court("Material Cartel"));
    let held = f.s.cartel[Material.as_index()];
    discard_guild_card(&mut f.s, player, court("Material Cartel"));

    assert_eq!(f.s.cartel[Material.as_index()], 0);
    assert_eq!(f.s.supply[Material.as_index()], held);
}

// TS: "makes Rivals discard that type after scoring, onto the card"
#[test]
fn a_cartel_makes_rivals_discard_its_type_after_scoring() {
    let mut f = start_game(3, 375, 0);
    let holder = Player(0);
    let rival = Player(1);
    gain_guild_card(&mut f.s, holder, court("Fuel Cartel"));
    let held = f.s.cartel[Fuel.as_index()];
    // Clear every board so the only Fuel in play is the two placed here.
    for p in f.s.player_states.iter_mut() {
        p.resources = [None; 6];
    }
    f.s.player_states[rival.as_index()].resources[0] = Some(Fuel);
    f.s.player_states[rival.as_index()].resources[1] = Some(Relic);
    f.s.player_states[holder.as_index()].resources[0] = Some(Fuel);

    // Run a chapter to its end.
    for p in f.s.player_states.iter_mut() {
        p.hand.clear();
    }
    f.s.phase = Phase::Play;
    settle_to_chapter_end(&mut f);

    let rr = f.s.player(rival).resources;
    assert!(!rr.iter().flatten().any(|&r| r == Fuel));
    assert!(rr.iter().flatten().any(|&r| r == Relic)); // only its own type
    assert!(
        f.s.player(holder)
            .resources
            .iter()
            .flatten()
            .any(|&r| r == Fuel)
    ); // the holder keeps theirs
    assert_eq!(f.s.cartel[Fuel.as_index()], held + 1);
}

// ---------------------------------------------------------------------------
// the Unions
// ---------------------------------------------------------------------------

// TS: "attaches to a face-up played card of its suit and draws it at round
// end"
#[test]
fn a_union_attaches_to_a_played_card_and_draws_it_at_round_end() {
    let mut f = start_game(3, 381, 0);
    let leader = actor(&f);
    f.s.player_states[leader.as_index()].guild_cards =
        [court("Construction Union")].into_iter().collect();
    set_hand(
        &mut f,
        leader,
        &[card_id(CONSTRUCTION, 2), card_id(AGGRESSION, 3)],
    );
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 2),
        },
    );

    let attach = find(
        &actions(&f),
        |a| matches!(a, Action::CardPrelude { card, .. } if *card == court("Construction Union")),
    );
    apply(&mut f, attach);

    // The card is off the play area and on the played card, not discarded.
    assert!(
        !f.s.player(leader)
            .guild_cards
            .contains(&court("Construction Union"))
    );
    assert!(!f.s.court_discard.contains(&court("Construction Union")));
    assert_eq!(f.s.unions.len(), 1);
    assert_eq!(f.s.unions.as_slice()[0].target, card_id(CONSTRUCTION, 2));

    // Play out the round; the lead card comes back to hand instead of the
    // discard.
    apply(&mut f, Action::BeginActions);
    settle_round(&mut f);

    assert!(f.s.player(leader).hand.contains(&card_id(CONSTRUCTION, 2)));
    assert!(!f.s.action_discard.contains(&card_id(CONSTRUCTION, 2)));
    assert!(f.s.court_discard.contains(&court("Construction Union")));
    assert!(f.s.unions.is_empty());
}

// TS: "only attaches to its own suit, and never to a face-down play"
#[test]
fn a_union_only_attaches_to_its_own_suit_and_never_face_down() {
    let mut f = start_game(3, 382, 0);
    let leader = actor(&f);
    f.s.player_states[leader.as_index()].guild_cards = [court("Admin Union")].into_iter().collect(); // administration
    set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 2),
        },
    );
    assert!(
        !actions(&f)
            .iter()
            .any(|a| matches!(a, Action::CardPrelude { .. }))
    );

    // A Copy play is face down, so a Union cannot see it either.
    let mut g = start_game(3, 383, 0);
    let first = actor(&g);
    set_hand(&mut g, first, &[card_id(ADMIN, 2)]);
    apply(
        &mut g,
        Action::Lead {
            card: card_id(ADMIN, 2),
        },
    );
    apply(&mut g, Action::BeginActions);
    end_all_turns(&mut g);
    let second = actor(&g);
    g.s.player_states[second.as_index()].guild_cards = [court("Admin Union")].into_iter().collect();
    set_hand(&mut g, second, &[card_id(ADMIN, 3)]);
    apply(
        &mut g,
        Action::Follow {
            card: card_id(ADMIN, 3),
            mode: arcs_engine::FollowMode::Copy,
        },
    );
    let targets: Vec<_> = actions(&g)
        .iter()
        .filter_map(|a| match a {
            Action::CardPrelude {
                choice: PreludeChoice::Union { played },
                ..
            } => Some(*played),
            _ => None,
        })
        .collect();
    // Only the face-up lead is offered, not their own face-down copy. (The
    // TS action names the play by index — [0]; the Rust action names the
    // played card itself.)
    assert_eq!(targets, vec![card_id(ADMIN, 2)]);
}

// ---------------------------------------------------------------------------
// Prison Wardens
// ---------------------------------------------------------------------------

// TS: "Pressgang returns Captives for a freely chosen resource each"
#[test]
fn pressgang_returns_captives_for_freely_chosen_resources() {
    let (mut f, player) = turn_holding(391, &["Prison Wardens"], CONSTRUCTION, 2);
    {
        let p = &mut f.s.player_states[player.as_index()];
        p.captives = [0, 1, 1, 0];
        p.resources = [None; 6];
    }
    let agents_before = f.s.player(Player(1)).agents_supply + f.s.player(Player(2)).agents_supply;
    apply(&mut f, Action::BeginActions);

    let offers: Vec<Action> = actions(&f)
        .into_iter()
        .filter(|a| {
            matches!(
                a,
                Action::CardAction {
                    choice: CardActionChoice::Pressgang { .. },
                    ..
                }
            )
        })
        .collect();
    // Both "one captive" and "two captives" options, and mixed resource
    // picks.
    let gain_of = |a: &Action| match a {
        Action::CardAction {
            choice: CardActionChoice::Pressgang { gain },
            ..
        } => *gain,
        _ => unreachable!(),
    };
    assert!(offers.iter().any(|a| gain_of(a).len() == 1));
    assert!(offers.iter().any(|a| gain_of(a).len() == 2));
    assert!(offers.iter().any(|a| {
        let g = gain_of(a);
        g.len() == 2 && g.as_slice()[0] != g.as_slice()[1]
    }));

    let act = find(&offers, |a| gain_of(a).as_slice() == [Fuel, Relic]);
    apply(&mut f, act);

    let p = f.s.player(player);
    assert_eq!(p.captive_count(), 0);
    assert!(p.resources.iter().flatten().any(|&r| r == Fuel));
    assert!(p.resources.iter().flatten().any(|&r| r == Relic));
    // The agents went home to their owners.
    assert_eq!(
        f.s.player(Player(1)).agents_supply + f.s.player(Player(2)).agents_supply,
        agents_before + 2
    );
}

// TS: "Pressgang is capped by empty resource slots"
#[test]
fn pressgang_is_capped_by_empty_resource_slots() {
    let (mut f, player) = turn_holding(392, &["Prison Wardens"], CONSTRUCTION, 2);
    {
        let p = &mut f.s.player_states[player.as_index()];
        p.captives = [0, 3, 0, 0];
        p.resources = [Some(Fuel); 6]; // no room at all
    }
    apply(&mut f, Action::BeginActions);
    assert!(!actions(&f).iter().any(|a| matches!(
        a,
        Action::CardAction {
            choice: CardActionChoice::Pressgang { .. },
            ..
        }
    )));
}

// TS: "Execute moves Captives to Trophies, swapping Tyrant count for
// Warlord"
#[test]
fn execute_moves_captives_to_trophies_tyrant_becomes_warlord() {
    let (mut f, player) = turn_holding(393, &["Prison Wardens"], ADMIN, 2); // Influence pips
    f.s.player_states[player.as_index()].captives = [0, 1, 1, 0];
    apply(&mut f, Action::BeginActions);

    let act = find(&actions(&f), |a| {
        matches!(
            a,
            Action::CardAction {
                choice: CardActionChoice::Execute { count: 2 },
                ..
            }
        )
    });
    apply(&mut f, act);

    let p = f.s.player(player);
    assert_eq!(p.captive_count(), 0);
    assert_eq!(p.trophy_count(), 2);
    let agent_trophies: u8 = p
        .trophies
        .iter()
        .map(|by_owner| by_owner[TrophyKind::Agent.as_index()])
        .sum();
    assert_eq!(agent_trophies, 2);
    assert_eq!(ambition_count(p, AmbitionId::Tyrant), 0);
    assert_eq!(ambition_count(p, AmbitionId::Warlord), 2);
}

// TS: "an executed agent goes home when Trophies are returned"
#[test]
fn an_executed_agent_goes_home_when_trophies_are_returned() {
    let mut f = start_game(3, 394, 0);
    let before = f.s.player(Player(1)).agents_supply;
    f.s.player_states[0].trophies[1][TrophyKind::Agent.as_index()] = 1;
    // The TS fixture pushes marker 0 into the box without pulling it from
    // the available list; the bounded Rust list needs the state consistent.
    let at = f.s.available_markers.position(&0).unwrap();
    f.s.available_markers.remove(at);
    f.s.declared[AmbitionId::Warlord.as_index()].push(0);
    for p in f.s.player_states.iter_mut() {
        p.hand.clear();
    }
    f.s.phase = Phase::Play;
    settle_to_chapter_end(&mut f);

    assert_eq!(f.s.player(Player(1)).agents_supply, before + 1);
}

// ---------------------------------------------------------------------------
// Court Enforcers
// ---------------------------------------------------------------------------

// TS: "Abducts from a Court card holding fewer Rival agents than your
// Weapon icons"
#[test]
fn abduct_takes_agents_from_a_card_with_fewer_than_your_weapon_icons() {
    let (mut f, player) = turn_holding(401, &["Court Enforcers"], AGGRESSION, 2); // Battle pips
    let rival = Player((player.0 + 1) % 3);
    {
        let p = &mut f.s.player_states[player.as_index()];
        p.resources = [None; 6];
        p.resources[0] = Some(Weapon);
        p.resources[1] = Some(Weapon);
    }
    // Court Enforcers is itself a Weapon card, so that is 3 icons.
    assert_eq!(weapon_icons(f.s.player(player)), 3);

    f.s.court.as_mut_slice()[0].agents[rival.as_index()] = 2; // fewer than 3 — abductable
    f.s.court.as_mut_slice()[1].agents[rival.as_index()] = 3; // not fewer than 3
    apply(&mut f, Action::BeginActions);

    let offers: Vec<Action> = actions(&f)
        .into_iter()
        .filter(|a| {
            matches!(
                a,
                Action::CardAction {
                    choice: CardActionChoice::Abduct { .. },
                    ..
                }
            )
        })
        .collect();
    let slots: Vec<u8> = offers
        .iter()
        .map(|a| match a {
            Action::CardAction {
                choice: CardActionChoice::Abduct { slot },
                ..
            } => *slot,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(slots, vec![0]);

    apply(&mut f, offers[0]);
    assert_eq!(f.s.player(player).captives[rival.as_index()], 2);
    assert_eq!(f.s.court.as_slice()[0].agents[rival.as_index()], 0);
}

// ---------------------------------------------------------------------------
// Elder Broker
// ---------------------------------------------------------------------------

/// A planet the player controls, holding a Rival city — the Trade setup
/// shared by both Elder Broker tests.
fn trade_setup(seed: u64) -> (Fixture, Player, Player, usize, ResourceType) {
    let (mut f, player) = turn_holding(seed, &["Elder Broker"], ADMIN, 2); // Tax pips
    let rival = Player((player.0 + 1) % 3);
    let system = (0..f.s.systems.len())
        .find(|&i| {
            f.v.systems[i].kind == arcs_engine::map::SystemKind::Planet
                && f.v.systems[i].planet_type.is_some()
                && !f.s.systems[i].out_of_play
        })
        .expect("a typed planet");
    let planet_type = f.v.systems[system].planet_type.unwrap();
    // Clear whatever setup put here: Trade needs the player to *control* the
    // system, which any third player's starting ships would deny.
    f.s.systems[system].fresh = [0; 4];
    f.s.systems[system].damaged = [0; 4];
    f.s.systems[system].fresh[player.as_index()] = 3;
    f.s.systems[system].buildings = [Building::new(rival, BuildingKind::City, false)]
        .into_iter()
        .collect();
    (f, player, rival, system, planet_type)
}

// TS: "Trades a resource of the city's type for one the Rival lacks"
#[test]
fn trade_swaps_the_city_type_for_one_the_rival_lacks() {
    let (mut f, player, rival, _, planet_type) = trade_setup(411);
    f.s.player_states[rival.as_index()].resources = [None; 6];
    f.s.player_states[rival.as_index()].resources[0] = Some(planet_type);
    // A type they do not hold. With 5 types there is always another one.
    let give = ResourceType::ALL
        .into_iter()
        .find(|&r| r != planet_type)
        .unwrap();
    f.s.player_states[player.as_index()].resources = [None; 6];
    f.s.player_states[player.as_index()].resources[0] = Some(give);

    apply(&mut f, Action::BeginActions);
    let act = find(&actions(&f), |a| {
        matches!(
            a,
            Action::CardAction {
                choice: CardActionChoice::Trade { .. },
                ..
            }
        )
    });
    apply(&mut f, act);

    let mine = f.s.player(player).resources;
    let theirs = f.s.player(rival).resources;
    assert!(mine.iter().flatten().any(|&r| r == planet_type));
    assert!(theirs.iter().flatten().any(|&r| r == give));
    assert!(!theirs.iter().flatten().any(|&r| r == planet_type));
}

// TS: "is not offered when the Rival already holds every type you could
// give"
#[test]
fn trade_is_not_offered_when_the_rival_holds_everything_you_could_give() {
    let (mut f, player, rival, _, planet_type) = trade_setup(412);
    f.s.player_states[rival.as_index()].resources = [None; 6];
    f.s.player_states[rival.as_index()].resources[0] = Some(planet_type);
    f.s.player_states[player.as_index()].resources = [None; 6];
    // The only thing I hold is what they hold.
    f.s.player_states[player.as_index()].resources[0] = Some(planet_type);
    apply(&mut f, Action::BeginActions);
    assert!(!actions(&f).iter().any(|a| matches!(
        a,
        Action::CardAction {
            choice: CardActionChoice::Trade { .. },
            ..
        }
    )));
}

// ---------------------------------------------------------------------------
// Skirmishers
// ---------------------------------------------------------------------------

// TS: "offers a reroll of the blanks, up to your Weapon icons"
#[test]
fn skirmishers_offers_a_reroll_of_blanks_up_to_weapon_icons() {
    let (mut f, player, rival, system) = battle_with(421, &["Skirmishers"]);
    {
        let p = &mut f.s.player_states[player.as_index()];
        p.resources = [None; 6];
        p.resources[0] = Some(Weapon); // + the Skirmishers card itself = 2 icons
    }

    apply(
        &mut f,
        Action::Battle {
            system,
            defender: rival,
            assault: 0,
            skirmish: 4,
            raid: 0,
        },
    );
    // A roll where every skirmish die is blank: face indices 3-5 are blanks.
    resolve_chance_mut(&mut f.s, &f.v, &mut ConstRng(u64::MAX)).unwrap();

    assert_eq!(f.s.phase, Phase::BattleReroll);
    assert_eq!(f.s.battle.unwrap().skirmish_blanks, 4);
    let counts: Vec<u8> = actions(&f)
        .iter()
        .map(|a| match a {
            Action::RerollSkirmish { count } => *count,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(counts, vec![0, 1, 2]); // capped at 2 Weapon icons, not 4 blanks
}

// TS: "a reroll converts blanks into hits without touching the rest of the
// roll"
#[test]
fn a_reroll_converts_blanks_into_hits() {
    let (mut f, player, rival, system) = battle_with(422, &["Skirmishers"]);
    f.s.player_states[player.as_index()].resources = [None; 6];
    f.s.player_states[player.as_index()].resources[0] = Some(Weapon);

    apply(
        &mut f,
        Action::Battle {
            system,
            defender: rival,
            assault: 0,
            skirmish: 3,
            raid: 0,
        },
    );
    resolve_chance_mut(&mut f.s, &f.v, &mut ConstRng(u64::MAX)).unwrap(); // all blank
    assert_eq!(f.s.battle.unwrap().hits, 0);

    apply(&mut f, Action::RerollSkirmish { count: 2 });
    assert_eq!(f.s.phase, Phase::BattleRoll);
    resolve_chance_mut(&mut f.s, &f.v, &mut ConstRng(0)).unwrap(); // face index 0 — a hit

    let b = f.s.battle.unwrap();
    assert_eq!(b.hits, 2);
    assert!(b.reroll_done);
}

// TS: "declining goes straight to assignment, and the reroll is once per
// battle"
#[test]
fn declining_the_reroll_is_final() {
    let (mut f, player, rival, system) = battle_with(423, &["Skirmishers"]);
    f.s.player_states[player.as_index()].resources = [None; 6];
    f.s.player_states[player.as_index()].resources[0] = Some(Weapon);

    apply(
        &mut f,
        Action::Battle {
            system,
            defender: rival,
            assault: 0,
            skirmish: 2,
            raid: 0,
        },
    );
    resolve_chance_mut(&mut f.s, &f.v, &mut ConstRng(u64::MAX)).unwrap();
    apply(&mut f, Action::RerollSkirmish { count: 0 });
    assert!(f.s.battle.map(|b| b.reroll_done).unwrap_or(true));
    assert_ne!(f.s.phase, Phase::BattleReroll);
}

// TS: "is not offered without the card"
#[test]
fn the_reroll_is_not_offered_without_the_card() {
    let (mut f, _, rival, system) = battle_with(424, &[]);
    apply(
        &mut f,
        Action::Battle {
            system,
            defender: rival,
            assault: 0,
            skirmish: 2,
            raid: 0,
        },
    );
    resolve_chance_mut(&mut f.s, &f.v, &mut ConstRng(u64::MAX)).unwrap();
    assert_ne!(f.s.phase, Phase::BattleReroll);
}

// ---------------------------------------------------------------------------
// Galactic Bards
// ---------------------------------------------------------------------------

// TS: "lets a Surpass declare, without placing the zero marker"
#[test]
fn galactic_bards_lets_a_surpass_declare_without_the_zero_marker() {
    let mut f = start_game(3, 431, 0);
    let leader = actor(&f);
    set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 2),
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    let follower = actor(&f);
    f.s.player_states[follower.as_index()].guild_cards =
        [court("Galactic Bards")].into_iter().collect();
    set_hand(&mut f, follower, &[card_id(CONSTRUCTION, 5)]); // "5" = Keeper
    apply(
        &mut f,
        Action::Follow {
            card: card_id(CONSTRUCTION, 5),
            mode: arcs_engine::FollowMode::Surpass,
        },
    );

    let offered: Vec<AmbitionId> = actions(&f)
        .iter()
        .filter_map(|a| match a {
            Action::DeclareAmbition { ambition } => Some(*ambition),
            _ => None,
        })
        .collect();
    assert_eq!(offered, vec![AmbitionId::Keeper]);

    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Keeper,
        },
    );
    assert_eq!(f.s.declared[AmbitionId::Keeper.as_index()].len(), 1);
    assert_eq!(f.s.round.lead_number, 2); // no zero marker
}

// TS: "does nothing once an ambition is already declared this round"
#[test]
fn galactic_bards_does_nothing_once_an_ambition_is_declared() {
    let mut f = start_game(3, 432, 0);
    let leader = actor(&f);
    set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 4)]); // "4" = Warlord
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 4),
        },
    );
    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Warlord,
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    let follower = actor(&f);
    f.s.player_states[follower.as_index()].guild_cards =
        [court("Galactic Bards")].into_iter().collect();
    set_hand(&mut f, follower, &[card_id(CONSTRUCTION, 5)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(CONSTRUCTION, 5),
            mode: arcs_engine::FollowMode::Surpass,
        },
    );
    assert!(
        !actions(&f)
            .iter()
            .any(|a| matches!(a, Action::DeclareAmbition { .. }))
    );
}

// TS: "without the card, a Surpass cannot declare at all"
#[test]
fn without_the_bards_a_surpass_cannot_declare() {
    let mut f = start_game(3, 433, 0);
    let leader = actor(&f);
    set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 2),
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);
    let follower = actor(&f);
    set_hand(&mut f, follower, &[card_id(CONSTRUCTION, 5)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(CONSTRUCTION, 5),
            mode: arcs_engine::FollowMode::Surpass,
        },
    );
    assert!(
        !actions(&f)
            .iter()
            .any(|a| matches!(a, Action::DeclareAmbition { .. }))
    );
}

// ---------------------------------------------------------------------------
// Farseers
// ---------------------------------------------------------------------------

// TS: "peeks at one chosen hand on declaring, and may swap a card"
#[test]
fn farseers_peeks_one_chosen_hand_and_may_swap() {
    let mut f = start_game(3, 441, 0);
    let leader = actor(&f);
    let rival = Player((leader.0 + 1) % 3);
    f.s.player_states[leader.as_index()].guild_cards = [court("Farseers")].into_iter().collect();
    set_hand(
        &mut f,
        leader,
        &[card_id(CONSTRUCTION, 4), card_id(ADMIN, 2)],
    );
    set_hand(&mut f, rival, &[card_id(AGGRESSION, 6)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 4),
        },
    );
    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Warlord,
        },
    );

    // Step 1: choose whose hand to look at.
    assert_eq!(f.s.phase, Phase::PeekTarget);
    assert_eq!(actor(&f), leader);
    apply(
        &mut f,
        Action::PeekTarget {
            target: Some(rival),
        },
    );

    // Step 2: swap, now that the hand is visible.
    assert_eq!(f.s.phase, Phase::PeekSwap);
    apply(
        &mut f,
        Action::PeekSwap {
            give: card_id(ADMIN, 2),
            take: card_id(AGGRESSION, 6),
        },
    );

    assert!(f.s.player(leader).hand.contains(&card_id(AGGRESSION, 6)));
    assert!(f.s.player(rival).hand.contains(&card_id(ADMIN, 2)));
    assert_eq!(f.s.phase, Phase::Prelude); // back to the Prelude
    assert!(f.s.peek.is_none());
}

// TS: "reveals only the peeked hand, and only while the swap is open" —
// the `observe`/`determinize` halves of that test land with R4; what the
// engine owns today is the peek state those functions will read.
#[test]
fn the_peek_commits_to_one_target_before_any_hand_is_seen() {
    let mut f = start_game(3, 442, 0);
    let leader = actor(&f);
    let seen = Player((leader.0 + 1) % 3);
    f.s.player_states[leader.as_index()].guild_cards = [court("Farseers")].into_iter().collect();
    set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 4)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 4),
        },
    );
    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Warlord,
        },
    );

    // Before the target is named, the peek records no one.
    assert_eq!(f.s.peek.unwrap().target, None);
    apply(&mut f, Action::PeekTarget { target: Some(seen) });
    // Only the chosen target is recorded — the seam observe() (R4) reads.
    assert_eq!(f.s.peek.unwrap().target, Some(seen));
    assert_eq!(f.s.phase, Phase::PeekSwap);
}

// TS: "can decline to look at all"
#[test]
fn farseers_can_decline_to_look_at_all() {
    let mut f = start_game(3, 443, 0);
    let leader = actor(&f);
    f.s.player_states[leader.as_index()].guild_cards = [court("Farseers")].into_iter().collect();
    set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 4)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 4),
        },
    );
    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Warlord,
        },
    );
    apply(&mut f, Action::PeekTarget { target: None });
    assert_eq!(f.s.phase, Phase::Prelude);
    assert!(f.s.peek.is_none());
}

// TS: "its Prelude swaps n hand cards for n + 1 from the discard bottom"
#[test]
fn farseers_prelude_swaps_n_cards_for_n_plus_1_from_the_discard_bottom() {
    let (mut f, player) = turn_holding(444, &["Farseers"], CONSTRUCTION, 2);
    set_hand(&mut f, player, &[card_id(ADMIN, 2), card_id(ADMIN, 3)]);
    f.s.action_discard = [
        card_id(AGGRESSION, 4),
        card_id(AGGRESSION, 5),
        card_id(AGGRESSION, 6),
    ]
    .into_iter()
    .collect();

    let act = find(&actions(&f), |a| {
        matches!(
            a,
            Action::CardPrelude { card, choice: PreludeChoice::Recycle { cards } }
                if *card == court("Farseers") && cards.len() == 1
        )
    });
    let Action::CardPrelude {
        choice: PreludeChoice::Recycle { cards },
        ..
    } = act
    else {
        unreachable!()
    };
    let discarded = cards.as_slice()[0];
    apply(&mut f, act);

    // One card given up, two drawn from the bottom of the discard.
    let p = f.s.player(player);
    assert_eq!(p.hand.len(), 2 - 1 + 2);
    assert!(p.hand.contains(&card_id(AGGRESSION, 4)));
    assert!(p.hand.contains(&card_id(AGGRESSION, 5)));
    assert!(!p.hand.contains(&discarded));
    assert!(f.s.action_discard.contains(&discarded));
    assert!(!p.guild_cards.contains(&court("Farseers")));
}

// ---------------------------------------------------------------------------
// Vox cards, when secured
// ---------------------------------------------------------------------------

/// Put a Vox card in the Court with the player holding a majority on it
/// (`voxReady` in powers.test.ts).
fn vox_ready(seed: u64, name: &str) -> (Fixture, Player) {
    let (mut f, player) = turn_holding(seed, &[], ADMIN, 2); // Influence + Tax pips
    f.s.court.as_mut_slice()[0] = CourtSlot {
        card: court(name),
        agents: [0; 4],
    };
    f.s.court.as_mut_slice()[0].agents[player.as_index()] = 1;
    apply(&mut f, Action::BeginActions);
    // Secure needs a Relic-bought action; grant it directly.
    grant_free_secure(&mut f);
    apply(&mut f, Action::Secure { slot: 0 });
    (f, player)
}

// TS: "Mass Uprising places a ship in every system of a chosen cluster"
#[test]
fn mass_uprising_places_a_ship_in_every_system_of_a_cluster() {
    let (mut f, player) = vox_ready(451, "Mass Uprising");
    assert_eq!(
        f.s.pending_vox.map(|p| p.card),
        Some(court("Mass Uprising"))
    );
    // It is mandatory, so declining is not offered while a cluster is legal.
    assert!(!actions(&f).iter().any(|a| matches!(a, Action::VoxSkip)));

    let act = find(&actions(&f), |a| matches!(a, Action::Vox(_)));
    let Action::Vox(VoxChoice::Cluster(cluster)) = act else {
        panic!("expected a cluster choice")
    };
    let before: Vec<u8> =
        f.s.systems
            .iter()
            .map(|sys| sys.fresh[player.as_index()])
            .collect();
    apply(&mut f, act);

    for (i, sys) in f.s.systems.iter().enumerate() {
        let in_cluster =
            !sys.out_of_play && arcs_engine::map::cluster_of(SystemId(i as u8)) == cluster;
        let expected = if in_cluster { before[i] + 1 } else { before[i] };
        assert_eq!(sys.fresh[player.as_index()], expected, "system {i}");
    }
    assert!(f.s.pending_vox.is_none());
    assert!(f.s.court_discard.contains(&court("Mass Uprising")));
}

// TS: "Populist Demands declares any ambition"
#[test]
fn populist_demands_declares_any_ambition() {
    let (mut f, _) = vox_ready(452, "Populist Demands");
    let offered: Vec<AmbitionId> = actions(&f)
        .iter()
        .filter_map(|a| match a {
            Action::Vox(VoxChoice::Declare(ambition)) => Some(*ambition),
            _ => None,
        })
        .collect();
    assert_eq!(
        offered,
        vec![
            AmbitionId::Tycoon,
            AmbitionId::Tyrant,
            AmbitionId::Warlord,
            AmbitionId::Keeper,
            AmbitionId::Empath
        ]
    );

    apply(&mut f, Action::Vox(VoxChoice::Declare(AmbitionId::Empath)));
    assert_eq!(f.s.declared[AmbitionId::Empath.as_index()].len(), 1);
    assert!(f.s.pending_vox.is_none());
}

// TS: "Outrage Spreads outrages every player, including the securer"
#[test]
fn outrage_spreads_outrages_every_player_including_the_securer() {
    let (mut f, player) = vox_ready(453, "Outrage Spreads");
    for p in f.s.player_states.iter_mut() {
        p.resources = [None; 6];
        p.resources[0] = Some(Relic);
    }
    let act = Action::Vox(VoxChoice::Outrage(Relic));
    apply(&mut f, act);

    for p in 0..3 {
        let ps = f.s.player(Player(p));
        assert!(!ps.resources.iter().flatten().any(|&r| r == Relic));
        assert!(ps.outrage[Relic.as_index()]);
    }
    assert!(f.s.player(player).outrage[Relic.as_index()]);
}

// TS: "Song of Freedom returns a city and may seize, then goes back in the
// deck"
#[test]
fn song_of_freedom_returns_a_city_and_goes_back_in_the_deck() {
    let (mut f, player) = vox_ready(454, "Song of Freedom");
    let controlled_city = (0..f.s.systems.len()).find(|&i| {
        f.s.systems[i]
            .buildings
            .iter()
            .any(|b| b.kind() == BuildingKind::City)
            && f.s.control_of(SystemId(i as u8)) == Some(player)
    });
    // Give the player control of a city's system so one is offered.
    if controlled_city.is_none() {
        let any = (0..f.s.systems.len())
            .find(|&i| {
                f.s.systems[i]
                    .buildings
                    .iter()
                    .any(|b| b.kind() == BuildingKind::City)
            })
            .expect("a city on the map");
        f.s.systems[any].fresh[player.as_index()] = 9;
    }
    let act = find(&actions(&f), |a| matches!(a, Action::Vox(_)));
    let Action::Vox(VoxChoice::ReturnCity {
        system, building, ..
    }) = act
    else {
        panic!("expected a city choice")
    };
    let owner = f.s.systems[system.as_index()].buildings.as_slice()[building as usize].player();
    let used_before = f.s.player(owner).cities_used;
    apply(&mut f, act);

    assert_eq!(f.s.player(owner).cities_used, used_before - 1);
    assert!(f.s.court_deck.contains(&court("Song of Freedom")));
    assert!(!f.s.court_discard.contains(&court("Song of Freedom")));
}

// TS: "Guild Struggle steals a card and recycles the Guild discards"
#[test]
fn guild_struggle_steals_a_card_and_recycles_the_guild_discards() {
    let (mut f, player) = vox_ready(455, "Guild Struggle");
    let rival = Player((player.0 + 1) % 3);
    f.s.player_states[rival.as_index()].guild_cards = [court("Relic Fence")].into_iter().collect();
    f.s.court_discard = [court("Mining Interest"), court("Call to Action")]
        .into_iter()
        .collect();

    apply(
        &mut f,
        Action::Vox(VoxChoice::Steal {
            target: rival,
            card: court("Relic Fence"),
        }),
    );

    assert!(
        f.s.player(player)
            .guild_cards
            .contains(&court("Relic Fence"))
    );
    assert!(f.s.player(rival).guild_cards.is_empty());
    // Guild cards went back into the deck; the Vox one stayed discarded.
    assert!(f.s.court_deck.contains(&court("Mining Interest")));
    assert!(!f.s.court_discard.contains(&court("Mining Interest")));
    assert!(f.s.court_discard.contains(&court("Call to Action")));
    assert!(f.s.court_discard.contains(&court("Guild Struggle")));
}

// TS: "Call to Action needs no decision and just draws a card"
#[test]
fn call_to_action_needs_no_decision_and_draws_a_card() {
    let (mut f, player) = turn_holding(456, &[], ADMIN, 2);
    f.s.court.as_mut_slice()[0] = CourtSlot {
        card: court("Call to Action"),
        agents: [0; 4],
    };
    f.s.court.as_mut_slice()[0].agents[player.as_index()] = 1;
    apply(&mut f, Action::BeginActions);
    f.s.action_discard = [card_id(AGGRESSION, 4), card_id(AGGRESSION, 5)]
        .into_iter()
        .collect();
    let hand_before = f.s.player(player).hand.len();

    grant_free_secure(&mut f);
    apply(&mut f, Action::Secure { slot: 0 });

    assert!(f.s.pending_vox.is_none()); // resolved inline
    assert_eq!(f.s.player(player).hand.len(), hand_before + 1);
    assert!(f.s.player(player).hand.contains(&card_id(AGGRESSION, 4)));
}

// TS: "a pending Vox card holds the turn open until it is answered"
#[test]
fn a_pending_vox_card_holds_the_turn_open() {
    let (mut f, player) = vox_ready(457, "Outrage Spreads");
    // The securing action was the last thing the turn could pay for, but the
    // turn cannot end while the Vox effect is outstanding.
    assert!(f.s.pending_vox.is_some());
    assert_eq!(actor(&f), player);
    for a in actions(&f) {
        assert!(matches!(a, Action::Vox(_) | Action::VoxSkip));
    }

    apply(&mut f, Action::VoxSkip);
    assert!(f.s.pending_vox.is_none());
}

// ---------------------------------------------------------------------------
// determinized search safety
// ---------------------------------------------------------------------------

// TS: "a rollout never mutates the real turn state" — the TS regression was
// `cardPreludesUsed` shared by reference across clones; the Copy state makes
// aliasing impossible, and this pins it.
#[test]
fn a_rollout_never_mutates_the_real_turn_state() {
    let (mut f, player) = turn_holding(361, &["Relic Fence"], CONSTRUCTION, 2);
    f.s.player_states[player.as_index()].resources[0] = Some(Fuel);
    let before = actions(&f).len();

    let mut copy = f.s;
    {
        let turn = copy.turn.as_mut().unwrap();
        turn.card_preludes_used.push(court("Relic Fence"));
        turn.secured_this_prelude.push(CourtCardId(1));
        turn.prelude_spent[Fuel.as_index()] += 1;
    }

    let turn = f.s.turn.unwrap();
    assert!(turn.card_preludes_used.is_empty());
    assert!(turn.secured_this_prelude.is_empty());
    assert_eq!(turn.prelude_spent, [0; 5]);
    assert_eq!(actions(&f).len(), before);
}

// The pending-Vox interception in apply: anything that is not a Vox answer
// is rejected while one is outstanding (exercised implicitly by the TS
// suite; explicit here because the Rust API returns Result).
#[test]
fn only_vox_answers_are_accepted_while_one_is_pending() {
    let (mut f, _) = vox_ready(458, "Outrage Spreads");
    assert!(matches!(get_pending(&f.s, &f.v), Pending::Decision { .. }));
    let err = apply_action_mut(&mut f.s, &f.v, Action::EndTurn);
    assert!(err.is_err());
    let mut acts = Vec::new();
    legal_actions(&f.s, &f.v, &mut acts);
    assert!(!acts.is_empty());
    apply(&mut f, Action::VoxSkip);
}
