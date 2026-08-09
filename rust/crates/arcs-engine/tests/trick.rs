//! Ported from `tests/rules.test.ts`: the lead/follow trick, ambition
//! declarations, initiative, Prelude resources, and the chapter skeleton.
//! Each test cites the TS test name it ports. Board actions and battle land
//! in R2; their TS tests are not ported here.

mod common;

use arcs_engine::ambitions::AMBITION_MARKERS;
use arcs_engine::cards::action_card;
use arcs_engine::game::standings;
use arcs_engine::{Action, ActionKind, AmbitionId, FollowMode, Phase, Player, ResourceType, Suit};
use common::*;

fn count_leads(list: &[Action]) -> usize {
    list.iter()
        .filter(|a| matches!(a, Action::Lead { .. }))
        .count()
}

fn offers_declare(list: &[Action]) -> bool {
    list.iter()
        .any(|a| matches!(a, Action::DeclareAmbition { .. }))
}

// ---------------------------------------------------------------------------
// lead and follow (p8, p10)
// ---------------------------------------------------------------------------

// TS: "offers the leader every card plus passing the initiative"
#[test]
fn offers_the_leader_every_card_plus_passing() {
    let f = start_game(3, 11, 0);
    let list = actions(&f);
    assert_eq!(count_leads(&list), 6);
    assert!(list.contains(&Action::PassInitiative));
}

// TS: "lets followers Surpass only with the lead suit and a higher number"
#[test]
fn surpass_needs_lead_suit_and_higher_number() {
    let mut f = start_game(3, 11, 0);
    let leader = actor(&f);
    set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 4)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 4),
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    let follower = actor(&f);
    set_hand(
        &mut f,
        follower,
        &[
            card_id(CONSTRUCTION, 5), // surpass or copy
            card_id(CONSTRUCTION, 3), // copy only — same suit, lower number
            card_id(AGGRESSION, 6),   // pivot or copy
        ],
    );
    let list = actions(&f);
    assert_eq!(modes_for(&list, card_id(CONSTRUCTION, 5)), vec!['c', 's']);
    assert_eq!(modes_for(&list, card_id(CONSTRUCTION, 3)), vec!['c']);
    assert_eq!(modes_for(&list, card_id(AGGRESSION, 6)), vec!['c', 'p']);
}

// TS: "gives Surpass its own pips, but Copy and Pivot exactly one action"
#[test]
fn surpass_keeps_pips_copy_and_pivot_get_one() {
    let check = |mode: FollowMode, card, expected: u8| {
        let mut f = start_game(3, 11, 0);
        let leader = actor(&f);
        set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 4)]);
        apply(
            &mut f,
            Action::Lead {
                card: card_id(CONSTRUCTION, 4),
            },
        );
        apply(&mut f, Action::BeginActions);
        end_all_turns(&mut f);
        let follower = actor(&f);
        set_hand(&mut f, follower, &[card]);
        apply(&mut f, Action::Follow { card, mode });
        assert_eq!(f.s.turn.unwrap().pips_left, expected);
    };
    check(FollowMode::Surpass, card_id(CONSTRUCTION, 2), 4); // a "2" has 4 pips
    check(FollowMode::Copy, card_id(CONSTRUCTION, 2), 1);
    check(FollowMode::Pivot, card_id(MOBILIZATION, 2), 1);
}

// TS: "gives a Copy the lead suit's actions, and a Pivot its own card's"
#[test]
fn copy_uses_the_lead_suit_actions() {
    let mut f = start_game(3, 13, 0);
    let leader = actor(&f);
    set_hand(&mut f, leader, &[card_id(ADMIN, 4)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(ADMIN, 4),
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    let follower = actor(&f);
    set_hand(&mut f, follower, &[card_id(MOBILIZATION, 5)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(MOBILIZATION, 5),
            mode: FollowMode::Copy,
        },
    );
    let pip_actions = f.s.turn.unwrap().pip_actions;
    assert_eq!(
        pip_actions.iter().collect::<Vec<_>>(),
        vec![ActionKind::Tax, ActionKind::Repair, ActionKind::Influence]
    );
}

// TS: "a Pivot does not change the lead suit"
#[test]
fn pivot_does_not_change_the_lead_suit() {
    let mut f = start_game(3, 17, 0);
    let leader = actor(&f);
    set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 4)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 4),
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);
    let follower = actor(&f);
    set_hand(&mut f, follower, &[card_id(AGGRESSION, 6)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(AGGRESSION, 6),
            mode: FollowMode::Pivot,
        },
    );
    assert_eq!(
        action_card(f.s.round.lead.unwrap().card).suit,
        Suit::Construction
    );
}

// ---------------------------------------------------------------------------
// declaring an ambition (p9)
// ---------------------------------------------------------------------------

fn lead_and_hold(seed: u64, card: arcs_engine::ActionCardId) -> Fixture {
    let mut f = start_game(3, seed, 0);
    let leader = actor(&f);
    set_hand(&mut f, leader, &[card]);
    apply(&mut f, Action::Lead { card });
    f
}

// TS: "takes the highest available marker and zeroes the lead card"
#[test]
fn declaring_takes_the_highest_marker_and_zeroes_the_lead() {
    let mut f = lead_and_hold(21, card_id(CONSTRUCTION, 4)); // "4" = Warlord
    assert!(offers_declare(&actions(&f)));
    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Warlord,
        },
    );

    let declared = &f.s.declared[AmbitionId::Warlord.as_index()];
    assert_eq!(declared.len(), 1);
    let marker = declared.as_slice()[0] as usize;
    assert_eq!(AMBITION_MARKERS[marker].blue.first, 5); // the highest of 5/3/2
    assert_eq!(f.s.available_markers.len(), 2);
    assert_eq!(f.s.round.lead_number, 0);
}

// TS: "does not change the card's pips"
#[test]
fn declaring_does_not_change_the_pips() {
    let mut f = lead_and_hold(22, card_id(CONSTRUCTION, 4));
    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Warlord,
        },
    );
    assert_eq!(f.s.turn.unwrap().pips_left, 3); // a "4" has 3 pips
}

// TS: "only offers the ambition printed on the card"
#[test]
fn only_offers_the_printed_ambition() {
    let f = lead_and_hold(23, card_id(CONSTRUCTION, 5)); // "5" = Keeper
    let offered: Vec<AmbitionId> = actions(&f)
        .iter()
        .filter_map(|a| match a {
            Action::DeclareAmbition { ambition } => Some(*ambition),
            _ => None,
        })
        .collect();
    assert_eq!(offered, vec![AmbitionId::Keeper]);
}

// TS: "lets a '7' declare anything and a '1' declare nothing (4 players)"
#[test]
fn seven_declares_anything_one_declares_nothing() {
    let mut seven = start_game(4, 24, 0);
    let leader = actor(&seven);
    set_hand(&mut seven, leader, &[card_id(CONSTRUCTION, 7)]);
    apply(
        &mut seven,
        Action::Lead {
            card: card_id(CONSTRUCTION, 7),
        },
    );
    let declares = actions(&seven)
        .iter()
        .filter(|a| matches!(a, Action::DeclareAmbition { .. }))
        .count();
    assert_eq!(declares, 5);

    let mut one = start_game(4, 24, 0);
    let leader = actor(&one);
    set_hand(&mut one, leader, &[card_id(CONSTRUCTION, 1)]);
    apply(
        &mut one,
        Action::Lead {
            card: card_id(CONSTRUCTION, 1),
        },
    );
    assert!(!offers_declare(&actions(&one)));
}

// TS: "cannot declare twice with the same card"
#[test]
fn cannot_declare_twice_on_one_card() {
    let mut f = lead_and_hold(25, card_id(CONSTRUCTION, 4));
    apply(
        &mut f,
        Action::DeclareAmbition {
            ambition: AmbitionId::Warlord,
        },
    );
    assert!(!offers_declare(&actions(&f)));
}

// TS: "cannot declare once all 3 markers are placed"
#[test]
fn cannot_declare_without_available_markers() {
    let mut f = lead_and_hold(26, card_id(CONSTRUCTION, 4));
    f.s.available_markers.clear();
    assert!(!offers_declare(&actions(&f)));
}

// TS: "is only offered to the player leading"
#[test]
fn only_the_leader_declares() {
    let mut f = start_game(3, 27, 0);
    let leader = actor(&f);
    set_hand(&mut f, leader, &[card_id(CONSTRUCTION, 4)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 4),
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);
    let follower = actor(&f);
    set_hand(&mut f, follower, &[card_id(CONSTRUCTION, 6)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(CONSTRUCTION, 6),
            mode: FollowMode::Surpass,
        },
    );
    assert!(!offers_declare(&actions(&f)));
}

// ---------------------------------------------------------------------------
// initiative (p10-p11)
// ---------------------------------------------------------------------------

struct Play {
    card: arcs_engine::ActionCardId,
    mode: Option<FollowMode>,
}

/// Play a full round where each player plays one named card
/// (`playRound` in rules.test.ts).
fn play_round(f: &mut Fixture, plays: &[Play]) {
    for (i, play) in plays.iter().enumerate() {
        let player = actor(f);
        set_hand(f, player, &[play.card]);
        if i == 0 {
            apply(f, Action::Lead { card: play.card });
        } else {
            apply(
                f,
                Action::Follow {
                    card: play.card,
                    mode: play.mode.unwrap(),
                },
            );
        }
        // Skip the whole turn.
        apply(f, Action::BeginActions);
        end_all_turns(f);
        settle(f);
    }
}

// TS: "goes to the highest Surpass when nobody seized"
#[test]
fn initiative_goes_to_the_highest_surpass() {
    let mut f = start_game(3, 31, 0);
    let start = f.s.initiative;
    let second = Player((start.0 + 1) % 3);
    let third = Player((start.0 + 2) % 3);
    play_round(
        &mut f,
        &[
            Play {
                card: card_id(CONSTRUCTION, 2),
                mode: None,
            },
            Play {
                card: card_id(CONSTRUCTION, 4),
                mode: Some(FollowMode::Surpass),
            },
            Play {
                card: card_id(CONSTRUCTION, 6),
                mode: Some(FollowMode::Surpass),
            },
        ],
    );
    assert_eq!(f.s.initiative, third);
    assert_ne!(second, third);
}

// TS: "does not move when nobody Surpasses"
#[test]
fn initiative_stays_when_nobody_surpasses() {
    let mut f = start_game(3, 32, 0);
    let start = f.s.initiative;
    play_round(
        &mut f,
        &[
            Play {
                card: card_id(CONSTRUCTION, 2),
                mode: None,
            },
            Play {
                card: card_id(AGGRESSION, 6),
                mode: Some(FollowMode::Pivot),
            },
            Play {
                card: card_id(MOBILIZATION, 5),
                mode: Some(FollowMode::Copy),
            },
        ],
    );
    assert_eq!(f.s.initiative, start);
}

// TS: "a seize beats every Surpass"
#[test]
fn a_seize_beats_every_surpass() {
    let mut f = start_game(3, 33, 0);
    let start = f.s.initiative;
    let second = Player((start.0 + 1) % 3);

    set_hand(&mut f, start, &[card_id(CONSTRUCTION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 2),
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    // Second player seizes by burning a spare card.
    set_hand(&mut f, second, &[card_id(AGGRESSION, 3), card_id(ADMIN, 2)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(AGGRESSION, 3),
            mode: FollowMode::Pivot,
        },
    );
    apply(
        &mut f,
        Action::Seize {
            card: card_id(ADMIN, 2),
        },
    );
    assert!(f.s.initiative_seized);
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    // Third player Surpasses with a 6 — too late.
    let third = actor(&f);
    set_hand(&mut f, third, &[card_id(CONSTRUCTION, 6)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(CONSTRUCTION, 6),
            mode: FollowMode::Surpass,
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);
    settle(&mut f);

    assert_eq!(f.s.initiative, second);
}

// TS: "only one player may seize per round, and never the holder"
#[test]
fn one_seize_per_round_and_never_the_holder() {
    let mut f = start_game(3, 34, 0);
    let start = f.s.initiative;
    set_hand(
        &mut f,
        start,
        &[card_id(CONSTRUCTION, 2), card_id(ADMIN, 3)],
    );
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 2),
        },
    );
    assert!(
        !actions(&f)
            .iter()
            .any(|a| matches!(a, Action::Seize { .. }))
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    let second = actor(&f);
    set_hand(&mut f, second, &[card_id(AGGRESSION, 3), card_id(ADMIN, 2)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(AGGRESSION, 3),
            mode: FollowMode::Pivot,
        },
    );
    apply(
        &mut f,
        Action::Seize {
            card: card_id(ADMIN, 2),
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    let third = actor(&f);
    set_hand(&mut f, third, &[card_id(AGGRESSION, 4), card_id(ADMIN, 5)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(AGGRESSION, 4),
            mode: FollowMode::Pivot,
        },
    );
    assert!(
        !actions(&f)
            .iter()
            .any(|a| matches!(a, Action::Seize { .. }))
    );
}

// TS: "Surpassing with a '7' seizes the initiative (4 players)"
#[test]
fn surpassing_with_a_seven_seizes() {
    let mut f = start_game(4, 35, 0);
    let start = f.s.initiative;
    let second = Player((start.0 + 1) % 4);
    set_hand(&mut f, start, &[card_id(CONSTRUCTION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 2),
        },
    );
    apply(&mut f, Action::BeginActions);
    end_all_turns(&mut f);

    set_hand(&mut f, second, &[card_id(CONSTRUCTION, 7)]);
    apply(
        &mut f,
        Action::Follow {
            card: card_id(CONSTRUCTION, 7),
            mode: FollowMode::Surpass,
        },
    );
    assert_eq!(f.s.round.seized_by, Some(second));
    assert_eq!(f.s.turn.unwrap().pips_left, 1); // still gets the 7's single pip
}

// TS: "passing the initiative hands it on and ends the round at once"
#[test]
fn passing_hands_initiative_on_and_ends_the_round() {
    let mut f = start_game(3, 36, 0);
    let start = f.s.initiative;
    let rounds_before = f.s.stats.rounds;
    apply(&mut f, Action::PassInitiative);
    assert_eq!(f.s.initiative, Player((start.0 + 1) % 3));
    assert_eq!(f.s.stats.rounds, rounds_before + 1);
    assert_eq!(actor(&f), Player((start.0 + 1) % 3));
    assert_eq!(f.s.round.turn_index, 0);
}

// ---------------------------------------------------------------------------
// Prelude resources (p17, p20)
// ---------------------------------------------------------------------------

// TS: "a Fuel buys a Move even from a Construction card". R1 adaptation:
// board actions are not enumerated yet, so the granted free Move is
// asserted on the turn state instead of in the legal list.
#[test]
fn a_fuel_grants_a_free_move_from_any_card() {
    let mut f = start_game(3, 51, 0);
    let player = actor(&f);
    f.s.player_states[player.as_index()].resources[0] = Some(ResourceType::Fuel);
    set_hand(&mut f, player, &[card_id(CONSTRUCTION, 5)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 5),
        },
    );
    apply(&mut f, Action::SpendResource { slot: 0 });
    apply(&mut f, Action::BeginActions);
    let turn = f.s.turn.unwrap();
    assert_eq!(turn.free_actions.len(), 1);
    assert!(turn.free_actions.as_slice()[0].contains(ActionKind::Move));
    assert!(!turn.pip_actions.contains(ActionKind::Move)); // Construction pips buy no Move
}

// TS: "a Weapon lets the card's own pips buy Battle actions"
#[test]
fn a_weapon_makes_pips_buy_battle() {
    let mut f = start_game(3, 52, 0);
    let player = actor(&f);
    f.s.player_states[player.as_index()].resources[0] = Some(ResourceType::Weapon);
    set_hand(&mut f, player, &[card_id(CONSTRUCTION, 2)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 2),
        },
    );
    apply(&mut f, Action::SpendResource { slot: 0 });
    assert!(f.s.turn.unwrap().weapon_spent);
    apply(&mut f, Action::BeginActions);
    assert_eq!(f.s.turn.unwrap().pips_left, 4);
}

// TS: "Outraged resources cannot be spent in the Prelude (p16)"
#[test]
fn outraged_resources_cannot_be_spent() {
    let mut f = start_game(3, 53, 0);
    let player = actor(&f);
    let p = &mut f.s.player_states[player.as_index()];
    p.resources = [None; 6];
    p.resources[0] = Some(ResourceType::Fuel);
    p.outrage[ResourceType::Fuel.as_index()] = true;
    set_hand(&mut f, player, &[card_id(CONSTRUCTION, 5)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 5),
        },
    );
    assert!(
        !actions(&f)
            .iter()
            .any(|a| matches!(a, Action::SpendResource { .. }))
    );
}

// TS: "declaring and seizing close once a resource is spent (p20)"
#[test]
fn declaring_closes_once_a_resource_is_spent() {
    let mut f = start_game(3, 54, 0);
    let player = actor(&f);
    f.s.player_states[player.as_index()].resources[0] = Some(ResourceType::Fuel);
    set_hand(&mut f, player, &[card_id(CONSTRUCTION, 4)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 4),
        },
    );
    assert!(offers_declare(&actions(&f)));
    apply(&mut f, Action::SpendResource { slot: 0 });
    assert!(!offers_declare(&actions(&f)));
}

// TS: "spent Prelude resources go back to the supply when the Prelude ends"
#[test]
fn spent_resources_return_to_the_supply_at_prelude_end() {
    let mut f = start_game(3, 55, 0);
    let player = actor(&f);
    f.s.player_states[player.as_index()].resources[0] = Some(ResourceType::Fuel);
    let supply_before = f.s.supply[ResourceType::Fuel.as_index()];
    set_hand(&mut f, player, &[card_id(CONSTRUCTION, 5)]);
    apply(
        &mut f,
        Action::Lead {
            card: card_id(CONSTRUCTION, 5),
        },
    );
    apply(&mut f, Action::SpendResource { slot: 0 });
    assert_eq!(f.s.supply[ResourceType::Fuel.as_index()], supply_before);
    apply(&mut f, Action::BeginActions);
    assert_eq!(f.s.supply[ResourceType::Fuel.as_index()], supply_before + 1);
}

// ---------------------------------------------------------------------------
// chapters and game end (p19)
// ---------------------------------------------------------------------------

// TS: "returns the markers and flips the lowest unflipped one each chapter"
#[test]
fn chapter_end_returns_markers_and_flips_the_lowest() {
    let mut f = start_game(3, 71, 0);
    for p in 0..3 {
        f.s.player_states[p].hand.clear();
    }
    // Force the chapter to end by passing with empty hands.
    apply(&mut f, Action::PassInitiative);
    assert_eq!(f.s.flipped.iter().filter(|&&x| x).count(), 1);
    // The 2/0 marker (index 2, lowest) flips first.
    assert!(f.s.flipped[2]);
}

// TS: "ends the game once someone is at the Power threshold"
#[test]
fn ends_at_the_power_threshold() {
    let mut f = start_game(3, 72, 0);
    f.s.player_states[0].power = 30;
    for p in 0..3 {
        f.s.player_states[p].hand.clear();
    }
    apply(&mut f, Action::PassInitiative);
    assert_eq!(f.s.phase, Phase::Over);
    assert_eq!(standings(&f.s).as_slice()[0].player, Player(0));
}

// TS: "ends after chapter 5 even below the threshold"
#[test]
fn ends_after_chapter_five() {
    let mut f = start_game(3, 73, 0);
    f.s.chapter = 5;
    for p in 0..3 {
        f.s.player_states[p].hand.clear();
    }
    apply(&mut f, Action::PassInitiative);
    assert_eq!(f.s.phase, Phase::Over);
}

// TS: "breaks a final tie toward the earliest player in turn order (p19)"
#[test]
fn final_ties_break_toward_turn_order() {
    let mut f = start_game(3, 74, 0);
    f.s.initiative = Player(1);
    for p in 0..3 {
        f.s.player_states[p].power = 10;
    }
    assert_eq!(standings(&f.s).as_slice()[0].player, Player(1));
}

// Not in the TS suite as its own case (the 2p mulligan is exercised by
// `startGame`); pinned here against game.ts `applyActionMut`'s `mulligan`
// arm: taking the mulligan swaps the hand for the first 6 discard cards.
#[test]
fn mulligan_take_swaps_the_hand() {
    let v = arcs_engine::make_variant(2, 0, arcs_engine::SetupMode::Deck);
    let mut rng = arcs_engine::SplitMix64::new(7);
    let mut s = arcs_engine::new_game(&v, &mut rng, 0, arcs_engine::SetupMode::Deck);
    arcs_engine::resolve_chance_mut(&mut s, &v, &mut rng).unwrap();
    assert_eq!(s.phase, Phase::Mulligan);

    // The mulligan belongs to the player without initiative (p19).
    let muller = Player((s.initiative.0 + 1) % 2);
    let old_hand = s.player_states[muller.as_index()].hand;
    let expected: Vec<_> = s.action_discard.iter().copied().take(6).collect();
    arcs_engine::apply_action_mut(&mut s, &v, Action::Mulligan { take: true }).unwrap();
    let new_hand = s.player_states[muller.as_index()].hand;
    assert_eq!(new_hand.as_slice(), expected.as_slice());
    assert_ne!(new_hand, old_hand);
    // The old hand went to the discard; nothing was created or destroyed.
    for &c in old_hand.iter() {
        assert!(s.action_discard.contains(&c));
    }
    assert_eq!(s.phase, Phase::Play);
}
