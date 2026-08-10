//! Round-trip and injectivity property tests for the canonical action keys
//! (encode.ts's format plus the decoder the TS side never had).
//!
//! Ports the spirit of tests/encode.test.ts "never collides where
//! JSON.stringify distinguishes": every distinct action must encode to a
//! distinct key, and every key must decode back to exactly its action.

use arcs_engine::action::{CardList, ResourceList};
use arcs_engine::{
    Action, ActionCardId, AmbitionId, BuildingKind, CardActionChoice, CourtCardId, FollowMode,
    HitTarget, Player, PreludeChoice, ResourceType, SystemId, VoxChoice, decode_action,
    encode_action,
};
use std::collections::{HashMap, HashSet};

/// An exhaustive-in-shape sample: every variant, with every parameter swept
/// through its edge values (absent / zero / mid / max) one axis at a time,
/// plus full products where the space is small.
fn samples() -> Vec<Action> {
    let mut out = Vec::new();

    for card in 0..28u8 {
        out.push(Action::Lead {
            card: ActionCardId(card),
        });
        for mode in FollowMode::ALL {
            out.push(Action::Follow {
                card: ActionCardId(card),
                mode,
            });
        }
        out.push(Action::Seize {
            card: ActionCardId(card),
        });
    }
    out.push(Action::PassInitiative);
    out.push(Action::Mulligan { take: false });
    out.push(Action::Mulligan { take: true });
    for ambition in AmbitionId::ALL {
        out.push(Action::DeclareAmbition { ambition });
    }
    for slot in 0..6u8 {
        out.push(Action::SpendResource { slot });
        for spend_as in ResourceType::ALL {
            out.push(Action::SpendResourceAs { slot, spend_as });
        }
        out.push(Action::Influence { slot });
        out.push(Action::Secure { slot });
        out.push(Action::RaidResource { slot });
    }

    let card_lists = [
        CardList::new(),
        CardList::from_slice(&[ActionCardId(1)]),
        CardList::from_slice(&[ActionCardId(2), ActionCardId(5), ActionCardId(27)]),
    ];
    for card in 0..31u8 {
        let card = CourtCardId(card);
        let mut choices = vec![PreludeChoice::Bare];
        for system in [0u8, 23] {
            choices.push(PreludeChoice::System(SystemId(system)));
        }
        for target in [0u8, 3] {
            for slot in [0u8, 5] {
                choices.push(PreludeChoice::StealResource {
                    target: Player(target),
                    slot,
                });
            }
            for stolen in [0u8, 30] {
                choices.push(PreludeChoice::StealCard {
                    target: Player(target),
                    card: CourtCardId(stolen),
                });
            }
        }
        for slot in [0u8, 5] {
            choices.push(PreludeChoice::ConvertResource { slot });
        }
        for played in [0u8, 27] {
            choices.push(PreludeChoice::Union {
                played: ActionCardId(played),
            });
        }
        for cards in card_lists {
            choices.push(PreludeChoice::Recycle { cards });
        }
        for choice in choices {
            out.push(Action::CardPrelude { card, choice });
        }
    }
    out.push(Action::BeginActions);

    for system in 0..24u8 {
        for building in 0..2u8 {
            out.push(Action::Tax {
                system: SystemId(system),
                building,
            });
            out.push(Action::BuildShip {
                system: SystemId(system),
                building,
            });
        }
        for kind in BuildingKind::ALL {
            out.push(Action::BuildBuilding {
                system: SystemId(system),
                kind,
            });
        }
        out.push(Action::Repair {
            system: SystemId(system),
            building: None,
        });
        out.push(Action::Repair {
            system: SystemId(system),
            building: Some(1),
        });
        out.push(Action::Reinforce {
            system: SystemId(system),
        });
        for to in 0..24u8 {
            for ships in [1u8, 7, 15] {
                out.push(Action::Move {
                    from: SystemId(system),
                    to: SystemId(to),
                    ships,
                });
            }
        }
        out.push(Action::Catapult {
            to: SystemId(system),
            ships: 3,
        });
    }
    out.push(Action::CatapultStop);

    let resource_lists = [
        ResourceList::new(),
        ResourceList::from_slice(&[ResourceType::Fuel]),
        ResourceList::from_slice(&[
            ResourceType::Material,
            ResourceType::Material,
            ResourceType::Psionic,
        ]),
    ];
    let mut card_action_choices = vec![CardActionChoice::Manufacture, CardActionChoice::Synthesize];
    for gain in resource_lists {
        card_action_choices.push(CardActionChoice::Pressgang { gain });
    }
    for count in [0u8, 4] {
        card_action_choices.push(CardActionChoice::Execute { count });
    }
    for slot in [0u8, 3] {
        card_action_choices.push(CardActionChoice::Abduct { slot });
    }
    for system in [0u8, 9] {
        for building in [0u8, 1] {
            for slot in [0u8, 2] {
                for give_slot in [0u8, 3] {
                    card_action_choices.push(CardActionChoice::Trade {
                        system: SystemId(system),
                        building,
                        slot,
                        give_slot,
                    });
                }
            }
        }
    }
    for card in [0u8, 11, 30] {
        for choice in &card_action_choices {
            out.push(Action::CardAction {
                card: CourtCardId(card),
                choice: *choice,
            });
        }
    }

    for system in 0..24u8 {
        for defender in 0..4u8 {
            for (a, sk, r) in [(1, 0, 0), (0, 1, 0), (0, 0, 1), (2, 3, 1), (6, 6, 6)] {
                out.push(Action::Battle {
                    system: SystemId(system),
                    defender: Player(defender),
                    assault: a,
                    skirmish: sk,
                    raid: r,
                });
            }
        }
    }
    out.push(Action::EndTurn);

    for fresh in [false, true] {
        out.push(Action::AssignSelf { fresh });
        out.push(Action::AssignHit {
            target: HitTarget::Ship { fresh },
        });
    }
    for building in 0..2u8 {
        out.push(Action::AssignHit {
            target: HitTarget::Building { building },
        });
    }
    for card in 0..31u8 {
        out.push(Action::RaidCard {
            card: CourtCardId(card),
        });
    }
    out.push(Action::RaidDone);
    for count in 0..=6u8 {
        out.push(Action::RerollSkirmish { count });
    }

    out.push(Action::PeekTarget { target: None });
    for target in 0..4u8 {
        out.push(Action::PeekTarget {
            target: Some(Player(target)),
        });
    }
    for give in [0u8, 13, 27] {
        for take in [1u8, 14, 26] {
            out.push(Action::PeekSwap {
                give: ActionCardId(give),
                take: ActionCardId(take),
            });
        }
    }
    out.push(Action::PeekSwapSkip);

    // Vox: every choice shape, swept through its own edge values.
    for cluster in 0..6u8 {
        out.push(Action::Vox(VoxChoice::Cluster(cluster)));
    }
    for ambition in AmbitionId::ALL {
        out.push(Action::Vox(VoxChoice::Declare(ambition)));
    }
    for resource in ResourceType::ALL {
        out.push(Action::Vox(VoxChoice::Outrage(resource)));
    }
    for system in [0u8, 12, 23] {
        for building in [0u8, 1] {
            for seize in [false, true] {
                out.push(Action::Vox(VoxChoice::ReturnCity {
                    system: SystemId(system),
                    building,
                    seize,
                }));
            }
        }
    }
    for target in 0..4u8 {
        for card in [0u8, 30] {
            out.push(Action::Vox(VoxChoice::Steal {
                target: Player(target),
                card: CourtCardId(card),
            }));
        }
    }
    out.push(Action::VoxSkip);

    out
}

#[test]
fn encode_decode_round_trips() {
    let all = samples();
    // The sweep used to be a full product over the optional-field bags —
    // ~30k samples, most of them field combinations no card could ever ask
    // for. The choice enums make those unrepresentable, so what is left is
    // the legal shapes: fewer samples, same coverage of the encoding.
    assert!(all.len() > 3_000, "sample sweep shrank to {}", all.len());
    for a in &all {
        let key = encode_action(*a);
        let back = decode_action(&key);
        assert_eq!(back, Some(*a), "key {key:?}");
    }
}

#[test]
fn keys_are_injective() {
    let all = samples();
    let distinct: HashSet<Action> = all.iter().copied().collect();
    let mut by_key: HashMap<String, Action> = HashMap::new();
    for a in distinct {
        if let Some(prev) = by_key.insert(encode_action(a), a) {
            panic!(
                "collision: {prev:?} and {a:?} both encode to {}",
                encode_action(a)
            );
        }
    }
}

#[test]
fn junk_keys_do_not_decode() {
    for junk in [
        "",
        ":",
        "zz:1",
        "ld:",
        "ld:x",
        "fo:3",
        "fo:3:q",
        "mu:2",
        "da:emperor",
        "cp:1",
        "vx:",
        // A Vox bag naming no choice: `vs` is how a player declines.
        "vx:::::::",
        // Choices whose own parameters are missing.
        "ca:11:Pressgang::::::",
        "ca:11:Execute::::::",
        "ca:22:Trade::::::",
        "bt:1:2:3/4",
        "bt:1:2:3/4/5/6",
        "pi:0",
        "ah:x:1",
        "rp:3:m",
        "sa:0:gold",
    ] {
        assert_eq!(decode_action(junk), None, "junk {junk:?} decoded");
    }
}

/// The nested choice enums must stay small: `Action` is copied into every
/// search node, and the flat parameter bags were what pushed it to 24 bytes.
#[test]
fn action_stays_small() {
    assert!(
        size_of::<Action>() <= 16,
        "Action is {} bytes",
        size_of::<Action>()
    );
    println!("Action = {} bytes", size_of::<Action>());
}
