//! The Court deck: 25 Guild cards + 6 Vox cards (rulebook p3, p4 step H,
//! p17), mirroring `src/engine/court.ts`.
//!
//! A Guild card has a suit matching one of the 5 resources — it adds to
//! Tycoon / Keeper / Empath exactly like a resource token, and Weapon cards
//! add to no ambition (p17) — plus a raid cost. Vox cards resolve immediately
//! when secured and are discarded.
//!
//! Printed numbers run 01-25 for Guild and 26-31 for Vox, and `id` is
//! `number - 1`. `power` is the engine-readable form of a Guild ability; the
//! verbatim rules text stays on the TS side (UI-only).

use crate::types::{ActionKind, CourtCardId, ResourceType, Suit};

// ---------------------------------------------------------------------------
// Card powers
//
// These types describe const data (`&'static` slices into COURT_DECK), so
// under the `serde` feature they derive Serialize only — game state refers
// to cards by `CourtCardId`, never by embedded defs, so nothing ever needs
// to deserialize one.
// ---------------------------------------------------------------------------

/// A Prelude ability that discards the card to do something (p20). Every one
/// of these reads "Prelude: You may discard this to ..." (except
/// `ConvertResource`, which keeps the card).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum PreludeAbility {
    /// Place `count` fresh ships in one system you control.
    PlaceShips { count: u8 },
    /// Place 1 fresh ship in every in-play gate.
    ShipInEveryGate,
    /// Gain `resource` up to your number of empty slots; steal if the supply
    /// is out.
    FillSlots { resource: ResourceType },
    /// Steal 1 `resource` from a Rival.
    StealResource { resource: ResourceType },
    /// Steal any one Guild card or resource from a Rival.
    StealAny,
    /// Gain one of each listed resource from the supply.
    GainResources { resources: &'static [ResourceType] },
    /// Seize the initiative without paying a card.
    SeizeInitiative,
    /// Once per turn, without discarding: trade 1 resource for 1 `gain`.
    ConvertResource { gain: ResourceType },
    /// A Union: attach to a face-up played card of `suit` to draw it later.
    AttachUnion { suit: Suit },
    /// Farseers: discard this and any hand cards, redraw from the discard
    /// bottom.
    RecycleHand,
}

/// What a Guild card's new action does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum NewActionEffect {
    GainResource { resource: ResourceType },
    Pressgang,
    Execute,
    Abduct,
    Trade,
}

/// A new action replacing a standard one, written `Name (Standard): ...`
/// (p20). `replaces` is always one of tax / build / influence / battle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct NewAction {
    pub name: &'static str,
    /// The standard action whose pip pays for it.
    pub replaces: ActionKind,
    pub effect: NewActionEffect,
}

/// Which declarations Secret Order / Galactic Bards exempt from the zero
/// marker.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum NoZeroScope {
    KeeperEmpath,
    SurpassPivot,
}

/// Persistent effects the engine consults at specific points.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Passive {
    /// "You may spend any resources as X", and Outrage never discards this
    /// card.
    Loyal { spend_as: ResourceType },
    /// Holds its resource type's supply; Rivals discard that type after
    /// scoring.
    Cartel { resource: ResourceType },
    /// Attach to a played card of `suit`; draw that card at the end of the
    /// round.
    Union { suit: Suit },
    /// Collect `count` extra battle dice when the battle system is a gate.
    GateDice { count: u8 },
    /// Reroll skirmish dice up to your Weapon icon count.
    RerollSkirmish,
    /// Rivals cannot steal your resources or other Guild cards.
    TheftImmune,
    /// Declaring these ambitions does not place the zero marker.
    NoZeroMarker { ambitions: NoZeroScope },
    /// On declaring an ambition, look at a Rival hand and may swap 1 card.
    PeekAndSwap,
}

/// The engine-readable form of a Guild card's printed ability.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CardPower {
    pub prelude: Option<PreludeAbility>,
    pub new_actions: &'static [NewAction],
    pub passives: &'static [Passive],
}

impl CardPower {
    pub const NONE: CardPower = CardPower {
        prelude: None,
        new_actions: &[],
        passives: &[],
    };
}

/// What a Vox card does when secured (they always discard afterwards; Song
/// of Freedom shuffles back into the Court deck instead).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum VoxEffect {
    ShipInEachSystemOfCluster,
    DeclareAnyAmbition,
    OutrageAll,
    ReturnCityMaySeize,
    StealGuildCardAndRecycle,
    DrawFromDiscardBottom,
}

impl VoxEffect {
    /// Song of Freedom shuffles back into the Court deck rather than
    /// discarding.
    #[inline]
    pub const fn shuffle_back_in(self) -> bool {
        matches!(self, VoxEffect::ReturnCityMaySeize)
    }
}

// ---------------------------------------------------------------------------
// The deck
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum CourtCardKind {
    Guild,
    Vox,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CourtCardDef {
    pub id: CourtCardId,
    /// The number printed on the card: 01-25 Guild, 26-31 Vox.
    pub number: u8,
    pub name: &'static str,
    pub kind: CourtCardKind,
    /// Guild cards only: counts toward ambitions exactly like a resource
    /// token.
    pub suit: Option<ResourceType>,
    /// Keys an attacker must spend to steal this card in a raid.
    pub raid_cost: u8,
    /// Guild cards only: the engine-readable ability.
    pub power: Option<CardPower>,
    /// Vox cards only: what resolves when the card is secured.
    pub vox: Option<VoxEffect>,
}

impl CourtCardDef {
    /// Vox cards resolve and are discarded rather than kept.
    #[inline]
    pub const fn discard_on_secure(&self) -> bool {
        self.vox.is_some()
    }
}

pub const GUILD_CARD_COUNT: usize = 25;
pub const VOX_CARD_COUNT: usize = 6;
pub const COURT_CARD_COUNT: usize = GUILD_CARD_COUNT + VOX_CARD_COUNT;

const fn guild(
    id: u8,
    name: &'static str,
    suit: ResourceType,
    raid_cost: u8,
    power: CardPower,
) -> CourtCardDef {
    CourtCardDef {
        id: CourtCardId(id),
        number: id + 1,
        name,
        kind: CourtCardKind::Guild,
        suit: Some(suit),
        raid_cost,
        power: Some(power),
        vox: None,
    }
}

const fn vox(id: u8, name: &'static str, effect: VoxEffect) -> CourtCardDef {
    CourtCardDef {
        id: CourtCardId(id),
        number: id + 1,
        name,
        kind: CourtCardKind::Vox,
        suit: None,
        raid_cost: 0,
        power: None,
        vox: Some(effect),
    }
}

/// The "Loyal X" shape (cards 01/07/15/19/21): raid cost 3, the loyal
/// passive, and (Loyal Marines only) a place-ships Prelude.
const fn loyal(
    id: u8,
    name: &'static str,
    suit: ResourceType,
    prelude: Option<PreludeAbility>,
    passives: &'static [Passive],
) -> CourtCardDef {
    guild(
        id,
        name,
        suit,
        3,
        CardPower {
            prelude,
            new_actions: &[],
            passives,
        },
    )
}

const PLACE_3_SHIPS: Option<PreludeAbility> = Some(PreludeAbility::PlaceShips { count: 3 });

/// All 31 Court cards in printed order, `COURT_DECK[id]` having that id.
pub static COURT_DECK: [CourtCardDef; COURT_CARD_COUNT] = {
    use ResourceType::{Fuel as F, Material as M, Psionic as P, Relic as R, Weapon as W};
    [
        loyal(
            0,
            "Loyal Engineers",
            M,
            None,
            &[Passive::Loyal { spend_as: M }],
        ),
        guild(
            1,
            "Mining Interest",
            M,
            2,
            CardPower {
                prelude: Some(PreludeAbility::FillSlots { resource: M }),
                new_actions: &[NewAction {
                    name: "Manufacture",
                    replaces: ActionKind::Build,
                    effect: NewActionEffect::GainResource { resource: M },
                }],
                passives: &[],
            },
        ),
        guild(
            2,
            "Material Cartel",
            M,
            2,
            CardPower {
                prelude: Some(PreludeAbility::StealResource { resource: M }),
                new_actions: &[],
                passives: &[Passive::Cartel { resource: M }],
            },
        ),
        guild(
            3,
            "Admin Union",
            M,
            2,
            CardPower {
                prelude: Some(PreludeAbility::AttachUnion {
                    suit: Suit::Administration,
                }),
                new_actions: &[],
                passives: &[Passive::Union {
                    suit: Suit::Administration,
                }],
            },
        ),
        guild(
            4,
            "Construction Union",
            M,
            2,
            CardPower {
                prelude: Some(PreludeAbility::AttachUnion {
                    suit: Suit::Construction,
                }),
                new_actions: &[],
                passives: &[Passive::Union {
                    suit: Suit::Construction,
                }],
            },
        ),
        guild(
            5,
            "Fuel Cartel",
            F,
            2,
            CardPower {
                prelude: Some(PreludeAbility::StealResource { resource: F }),
                new_actions: &[],
                passives: &[Passive::Cartel { resource: F }],
            },
        ),
        loyal(
            6,
            "Loyal Pilots",
            F,
            None,
            &[Passive::Loyal { spend_as: F }],
        ),
        guild(
            7,
            "Gatekeepers",
            F,
            2,
            CardPower {
                prelude: Some(PreludeAbility::ShipInEveryGate),
                new_actions: &[],
                passives: &[Passive::GateDice { count: 2 }],
            },
        ),
        guild(
            8,
            "Shipping Interest",
            F,
            2,
            CardPower {
                prelude: Some(PreludeAbility::FillSlots { resource: F }),
                new_actions: &[NewAction {
                    name: "Synthesize",
                    replaces: ActionKind::Build,
                    effect: NewActionEffect::GainResource { resource: F },
                }],
                passives: &[],
            },
        ),
        guild(
            9,
            "Spacing Union",
            F,
            2,
            CardPower {
                prelude: Some(PreludeAbility::AttachUnion {
                    suit: Suit::Mobilization,
                }),
                new_actions: &[],
                passives: &[Passive::Union {
                    suit: Suit::Mobilization,
                }],
            },
        ),
        guild(
            10,
            "Arms Union",
            W,
            2,
            CardPower {
                prelude: Some(PreludeAbility::AttachUnion {
                    suit: Suit::Aggression,
                }),
                new_actions: &[],
                passives: &[Passive::Union {
                    suit: Suit::Aggression,
                }],
            },
        ),
        guild(
            11,
            "Prison Wardens",
            W,
            2,
            CardPower {
                prelude: PLACE_3_SHIPS,
                new_actions: &[
                    NewAction {
                        name: "Pressgang",
                        replaces: ActionKind::Build,
                        effect: NewActionEffect::Pressgang,
                    },
                    NewAction {
                        name: "Execute",
                        replaces: ActionKind::Influence,
                        effect: NewActionEffect::Execute,
                    },
                ],
                passives: &[],
            },
        ),
        guild(
            12,
            "Skirmishers",
            W,
            2,
            CardPower {
                prelude: PLACE_3_SHIPS,
                new_actions: &[],
                passives: &[Passive::RerollSkirmish],
            },
        ),
        guild(
            13,
            "Court Enforcers",
            W,
            2,
            CardPower {
                prelude: PLACE_3_SHIPS,
                new_actions: &[NewAction {
                    name: "Abduct",
                    replaces: ActionKind::Battle,
                    effect: NewActionEffect::Abduct,
                }],
                passives: &[],
            },
        ),
        loyal(
            14,
            "Loyal Marines",
            W,
            PLACE_3_SHIPS,
            &[Passive::Loyal { spend_as: W }],
        ),
        guild(
            15,
            "Lattice Spies",
            P,
            2,
            CardPower {
                prelude: Some(PreludeAbility::SeizeInitiative),
                new_actions: &[],
                passives: &[],
            },
        ),
        guild(
            16,
            "Farseers",
            P,
            2,
            CardPower {
                prelude: Some(PreludeAbility::RecycleHand),
                new_actions: &[],
                passives: &[Passive::PeekAndSwap],
            },
        ),
        guild(
            17,
            "Secret Order",
            P,
            2,
            CardPower {
                prelude: None,
                new_actions: &[],
                passives: &[Passive::NoZeroMarker {
                    ambitions: NoZeroScope::KeeperEmpath,
                }],
            },
        ),
        loyal(
            18,
            "Loyal Empaths",
            P,
            None,
            &[Passive::Loyal { spend_as: P }],
        ),
        guild(
            19,
            "Silver-Tongues",
            P,
            2,
            CardPower {
                prelude: Some(PreludeAbility::StealAny),
                new_actions: &[],
                passives: &[],
            },
        ),
        loyal(
            20,
            "Loyal Keepers",
            R,
            None,
            &[Passive::Loyal { spend_as: R }],
        ),
        guild(
            21,
            "Sworn Guardians",
            R,
            1,
            CardPower {
                prelude: None,
                new_actions: &[],
                passives: &[Passive::TheftImmune],
            },
        ),
        guild(
            22,
            "Elder Broker",
            R,
            2,
            CardPower {
                prelude: Some(PreludeAbility::GainResources {
                    resources: &[M, F, W],
                }),
                new_actions: &[NewAction {
                    name: "Trade",
                    replaces: ActionKind::Tax,
                    effect: NewActionEffect::Trade,
                }],
                passives: &[],
            },
        ),
        guild(
            23,
            "Relic Fence",
            R,
            2,
            CardPower {
                prelude: Some(PreludeAbility::ConvertResource { gain: R }),
                new_actions: &[],
                passives: &[],
            },
        ),
        guild(
            24,
            "Galactic Bards",
            R,
            1,
            CardPower {
                prelude: None,
                new_actions: &[],
                passives: &[Passive::NoZeroMarker {
                    ambitions: NoZeroScope::SurpassPivot,
                }],
            },
        ),
        vox(25, "Mass Uprising", VoxEffect::ShipInEachSystemOfCluster),
        vox(26, "Populist Demands", VoxEffect::DeclareAnyAmbition),
        vox(27, "Outrage Spreads", VoxEffect::OutrageAll),
        vox(28, "Song of Freedom", VoxEffect::ReturnCityMaySeize),
        vox(29, "Guild Struggle", VoxEffect::StealGuildCardAndRecycle),
        vox(30, "Call to Action", VoxEffect::DrawFromDiscardBottom),
    ]
};

#[inline]
pub fn court_card(id: CourtCardId) -> &'static CourtCardDef {
    &COURT_DECK[id.0 as usize]
}

/// Court row width: 3 at 2 players, 4 at 3-4 players (p4 step H).
#[inline]
pub const fn court_row_size(players: u8) -> u8 {
    if players == 2 { 3 } else { 4 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_numbers_are_positional() {
        for (i, card) in COURT_DECK.iter().enumerate() {
            assert_eq!(card.id.as_index(), i);
            assert_eq!(card.number as usize, i + 1);
            match card.kind {
                CourtCardKind::Guild => {
                    assert!(i < GUILD_CARD_COUNT);
                    assert!(card.suit.is_some());
                    assert!(card.power.is_some());
                    assert!(card.vox.is_none());
                }
                CourtCardKind::Vox => {
                    assert!(i >= GUILD_CARD_COUNT);
                    assert!(card.suit.is_none());
                    assert_eq!(card.raid_cost, 0);
                    assert!(card.vox.is_some());
                    assert!(card.discard_on_secure());
                }
            }
        }
    }

    #[test]
    fn rulebook_legible_numbers_agree() {
        // Seven printed numbers are legible in the rulebook and pin the
        // ordering (see the TS source header).
        for (number, name) in [
            (1, "Loyal Engineers"),
            (3, "Material Cartel"),
            (4, "Admin Union"),
            (9, "Shipping Interest"),
            (11, "Arms Union"),
            (15, "Loyal Marines"),
            (18, "Secret Order"),
        ] {
            assert_eq!(COURT_DECK[number - 1].name, name);
        }
    }

    #[test]
    fn court_row_sizes() {
        assert_eq!(court_row_size(2), 3);
        assert_eq!(court_row_size(3), 4);
        assert_eq!(court_row_size(4), 4);
    }

    #[test]
    fn only_song_of_freedom_shuffles_back() {
        for card in &COURT_DECK {
            if let Some(effect) = card.vox {
                assert_eq!(effect.shuffle_back_in(), card.name == "Song of Freedom");
            }
        }
    }
}
