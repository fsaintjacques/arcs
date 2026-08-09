//! Round-trip and injectivity property tests for the canonical action keys
//! (encode.ts's format plus the decoder the TS side never had).
//!
//! Ports the spirit of tests/encode.test.ts "never collides where
//! JSON.stringify distinguishes": every distinct action must encode to a
//! distinct key, and every key must decode back to exactly its action.

use arcs_engine::action::{CardList, ResourceList};
use arcs_engine::{
    Action, ActionCardId, AmbitionId, BuildingKind, CardActionName, CourtCardId, FollowMode,
    HitTarget, Player, ResourceType, SystemId, decode_action, encode_action,
};
use std::collections::{HashMap, HashSet};

fn opt_u8(values: &[u8]) -> Vec<Option<u8>> {
    let mut out = vec![None];
    out.extend(values.iter().map(|&v| Some(v)));
    out
}

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

    let card_lists: Vec<Option<CardList>> = vec![
        None,
        Some(CardList::new()),
        Some(CardList::from_slice(&[ActionCardId(1)])),
        Some(CardList::from_slice(&[
            ActionCardId(2),
            ActionCardId(5),
            ActionCardId(27),
        ])),
    ];
    for card in 0..31u8 {
        for system in opt_u8(&[0, 23]) {
            for slot in opt_u8(&[0, 5]) {
                for target in opt_u8(&[0, 3]) {
                    for take_card in opt_u8(&[0, 30]) {
                        for played in opt_u8(&[0, 27]) {
                            for cards in &card_lists {
                                out.push(Action::CardPrelude {
                                    card: CourtCardId(card),
                                    system: system.map(SystemId),
                                    slot,
                                    target: target.map(Player),
                                    take_card: take_card.map(CourtCardId),
                                    played: played.map(ActionCardId),
                                    cards: *cards,
                                });
                            }
                        }
                    }
                }
            }
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

    let resource_lists: Vec<Option<ResourceList>> = vec![
        None,
        Some(ResourceList::new()),
        Some(ResourceList::from_slice(&[ResourceType::Fuel])),
        Some(ResourceList::from_slice(&[
            ResourceType::Material,
            ResourceType::Material,
            ResourceType::Psionic,
        ])),
    ];
    for name in CardActionName::ALL {
        for gain in &resource_lists {
            for count in opt_u8(&[0, 4]) {
                for slot in opt_u8(&[2]) {
                    for system in opt_u8(&[9]) {
                        for building in opt_u8(&[1]) {
                            for give_slot in opt_u8(&[3]) {
                                out.push(Action::CardAction {
                                    card: CourtCardId(11),
                                    name,
                                    gain: *gain,
                                    count,
                                    slot,
                                    system: system.map(SystemId),
                                    building,
                                    give_slot,
                                });
                            }
                        }
                    }
                }
            }
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

    // Vox: one axis at a time around an all-absent base, plus a dense corner.
    out.push(vox(|_| {}));
    for cluster in 0..6u8 {
        out.push(vox(|p| p.cluster = Some(cluster)));
    }
    for ambition in AmbitionId::ALL {
        out.push(vox(|p| p.ambition = Some(ambition)));
    }
    for resource in ResourceType::ALL {
        out.push(vox(|p| p.resource = Some(resource)));
    }
    for system in [0u8, 12, 23] {
        for building in opt_u8(&[0, 1]) {
            for seize in [None, Some(false), Some(true)] {
                out.push(vox(|p| {
                    p.system = Some(SystemId(system));
                    p.building = building;
                    p.seize = seize;
                }));
            }
        }
    }
    for target in 0..4u8 {
        for card in [0u8, 30] {
            out.push(vox(|p| {
                p.target = Some(Player(target));
                p.card = Some(CourtCardId(card));
            }));
        }
    }
    out.push(Action::VoxSkip);

    out
}

#[derive(Default)]
struct VoxParts {
    cluster: Option<u8>,
    ambition: Option<AmbitionId>,
    resource: Option<ResourceType>,
    system: Option<SystemId>,
    building: Option<u8>,
    seize: Option<bool>,
    target: Option<Player>,
    card: Option<CourtCardId>,
}

/// Build a Vox action from an all-absent base with overrides.
fn vox(build: impl FnOnce(&mut VoxParts)) -> Action {
    let mut p = VoxParts::default();
    build(&mut p);
    Action::Vox {
        cluster: p.cluster,
        ambition: p.ambition,
        resource: p.resource,
        system: p.system,
        building: p.building,
        seize: p.seize,
        target: p.target,
        card: p.card,
    }
}

#[test]
fn encode_decode_round_trips() {
    let all = samples();
    assert!(all.len() > 10_000, "sample sweep shrank to {}", all.len());
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
