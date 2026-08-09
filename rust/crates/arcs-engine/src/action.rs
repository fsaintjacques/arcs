//! The action vocabulary, mirroring the TS `Action` union (types.ts) and the
//! canonical string keys of `src/engine/encode.ts` — plus the decoder the TS
//! side never needed.
//!
//! `Action` is `Copy + Eq + Hash` and small (asserted <= 24 bytes) so search
//! trees can key nodes by the enum itself. Optional parameters use
//! fixed-size `Option`s and bounded lists; where the TS encoding
//! distinguishes an *absent* list from an *empty* one (Farseers' `cards`),
//! the Rust type is `Option<InlineVec>` for the same reason.

use crate::inline_vec::InlineVec;
use crate::types::{
    ActionCardId, AmbitionId, BuildingKind, CourtCardId, Player, ResourceType, SystemId, index_enum,
};

/// Hand cards named by a Farseers recycle (bounded by the hand).
pub type CardList = InlineVec<ActionCardId, 7>;

/// Resources gained by Pressgang, one per returned Captive (bounded by empty
/// resource slots).
pub type ResourceList = InlineVec<ResourceType, 6>;

index_enum! {
    /// How a follower plays into the trick (TS `'surpass' | 'copy' | 'pivot'`).
    pub enum FollowMode {
        Surpass,
        Copy,
        Pivot,
    }
}

index_enum! {
    /// The named Guild-card actions (TS carries the printed name string).
    pub enum CardActionName {
        Manufacture,
        Synthesize,
        Pressgang,
        Execute,
        Abduct,
        Trade,
    }
}

impl CardActionName {
    pub const fn printed(self) -> &'static str {
        match self {
            CardActionName::Manufacture => "Manufacture",
            CardActionName::Synthesize => "Synthesize",
            CardActionName::Pressgang => "Pressgang",
            CardActionName::Execute => "Execute",
            CardActionName::Abduct => "Abduct",
            CardActionName::Trade => "Trade",
        }
    }

    pub fn from_printed(name: &str) -> Option<Self> {
        CardActionName::ALL
            .into_iter()
            .find(|n| n.printed() == name)
    }
}

/// Where a battle hit lands (the TS `assignHit` target union).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HitTarget {
    /// A defending ship (or the attacker's own for `AssignSelf`).
    Ship { fresh: bool },
    /// A defending building, by index in the system's building list.
    Building { building: u8 },
}

/// Every move a player can make, mirroring the TS `Action` union variant for
/// variant (types.ts:392). Variants whose phases land in R2 (board actions,
/// battle) and R3 (card powers, Vox, Farseers) are already here so the type
/// never changes shape again.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Action {
    // --- play phase ---
    Lead {
        card: ActionCardId,
    },
    Follow {
        card: ActionCardId,
        mode: FollowMode,
    },
    /// Initiative holder declines to play: initiative passes, round ends
    /// (p8).
    PassInitiative,
    // --- mulligan ---
    Mulligan {
        take: bool,
    },
    // --- prelude ---
    DeclareAmbition {
        ambition: AmbitionId,
    },
    Seize {
        card: ActionCardId,
    },
    SpendResource {
        slot: u8,
    },
    /// Spend a resource as another type, via a Loyal Guild card (p20).
    SpendResourceAs {
        slot: u8,
        spend_as: ResourceType,
    },
    /// Use a Guild card's `Prelude:` ability (R3). The optional fields are
    /// the ability's parameters — see the TS doc comment.
    CardPrelude {
        card: CourtCardId,
        system: Option<SystemId>,
        slot: Option<u8>,
        target: Option<Player>,
        take_card: Option<CourtCardId>,
        played: Option<ActionCardId>,
        cards: Option<CardList>,
    },
    BeginActions,
    // --- actions (R2) ---
    Tax {
        system: SystemId,
        building: u8,
    },
    BuildShip {
        system: SystemId,
        building: u8,
    },
    BuildBuilding {
        system: SystemId,
        kind: BuildingKind,
    },
    /// A Guild card's new action, taken instead of the standard one it
    /// replaces (R3).
    CardAction {
        card: CourtCardId,
        name: CardActionName,
        gain: Option<ResourceList>,
        count: Option<u8>,
        slot: Option<u8>,
        system: Option<SystemId>,
        building: Option<u8>,
        give_slot: Option<u8>,
    },
    Move {
        from: SystemId,
        to: SystemId,
        ships: u8,
    },
    Catapult {
        to: SystemId,
        ships: u8,
    },
    CatapultStop,
    Repair {
        system: SystemId,
        building: Option<u8>,
    },
    Influence {
        slot: u8,
    },
    Secure {
        slot: u8,
    },
    Battle {
        system: SystemId,
        defender: Player,
        assault: u8,
        skirmish: u8,
        raid: u8,
    },
    EndTurn,
    // --- battle assignment (R2) ---
    /// Assign one self-hit to a Loyal ship in the battle system.
    AssignSelf {
        fresh: bool,
    },
    /// Assign one hit to a defending ship (or building once ships are gone).
    AssignHit {
        target: HitTarget,
    },
    /// Spend keys to steal a resource slot or a Guild card.
    RaidResource {
        slot: u8,
    },
    RaidCard {
        card: CourtCardId,
    },
    RaidDone,
    /// Skirmishers: reroll `count` blank skirmish dice (0 declines).
    RerollSkirmish {
        count: u8,
    },
    // --- Farseers (R3) ---
    /// Choose whose hand to look at, or `None` to decline.
    PeekTarget {
        target: Option<Player>,
    },
    /// Swap one of your cards for one of theirs.
    PeekSwap {
        give: ActionCardId,
        take: ActionCardId,
    },
    PeekSwapSkip,
    // --- Vox `When Secured` (R3) ---
    /// Resolve the pending Vox card; which fields matter depends on the
    /// card — see the TS doc comment.
    Vox {
        cluster: Option<u8>,
        ambition: Option<AmbitionId>,
        resource: Option<ResourceType>,
        system: Option<SystemId>,
        building: Option<u8>,
        seize: Option<bool>,
        target: Option<Player>,
        card: Option<CourtCardId>,
    },
    VoxSkip,
    // --- reinforce ---
    Reinforce {
        system: SystemId,
    },
}

// ---------------------------------------------------------------------------
// Canonical keys (port of encode.ts, plus the decoder)
// ---------------------------------------------------------------------------

const AMBITION_NAMES: [&str; AmbitionId::COUNT] =
    ["tycoon", "tyrant", "warlord", "keeper", "empath"];
const RESOURCE_NAMES: [&str; ResourceType::COUNT] =
    ["material", "fuel", "weapon", "relic", "psionic"];

fn ambition_name(a: AmbitionId) -> &'static str {
    AMBITION_NAMES[a.as_index()]
}

fn resource_name(r: ResourceType) -> &'static str {
    RESOURCE_NAMES[r.as_index()]
}

use core::fmt::Write as _;

fn opt<T: Into<usize> + Copy>(out: &mut String, x: Option<T>) {
    if let Some(x) = x {
        let _ = write!(out, "{}", x.into());
    }
}

/// The TS `arr()`: `''` for absent, `[a.b.c]` for present.
fn card_arr(out: &mut String, cards: Option<&CardList>) {
    if let Some(cards) = cards {
        out.push('[');
        for (i, c) in cards.iter().enumerate() {
            if i > 0 {
                out.push('.');
            }
            let _ = write!(out, "{}", c.0);
        }
        out.push(']');
    }
}

fn resource_arr(out: &mut String, gain: Option<&ResourceList>) {
    if let Some(gain) = gain {
        out.push('[');
        for (i, r) in gain.iter().enumerate() {
            if i > 0 {
                out.push('.');
            }
            out.push_str(resource_name(*r));
        }
        out.push(']');
    }
}

/// The canonical key of an action — stable, compact, injective. Matches the
/// TS `encodeAction` byte for byte on every variant.
pub fn encode_action(a: Action) -> String {
    let mut s = String::with_capacity(24);
    match a {
        Action::Lead { card } => {
            let _ = write!(s, "ld:{}", card.0);
        }
        Action::Follow { card, mode } => {
            let m = match mode {
                FollowMode::Surpass => 's',
                FollowMode::Copy => 'c',
                FollowMode::Pivot => 'p',
            };
            let _ = write!(s, "fo:{}:{m}", card.0);
        }
        Action::PassInitiative => s.push_str("pi"),
        Action::Mulligan { take } => {
            let _ = write!(s, "mu:{}", take as u8);
        }
        Action::DeclareAmbition { ambition } => {
            let _ = write!(s, "da:{}", ambition_name(ambition));
        }
        Action::Seize { card } => {
            let _ = write!(s, "sz:{}", card.0);
        }
        Action::SpendResource { slot } => {
            let _ = write!(s, "sr:{slot}");
        }
        Action::SpendResourceAs { slot, spend_as } => {
            let _ = write!(s, "sa:{slot}:{}", resource_name(spend_as));
        }
        Action::CardPrelude {
            card,
            system,
            slot,
            target,
            take_card,
            played,
            cards,
        } => {
            let _ = write!(s, "cp:{}:", card.0);
            opt(&mut s, system);
            s.push(':');
            opt(&mut s, slot.map(|x| x as usize));
            s.push(':');
            opt(&mut s, target);
            s.push(':');
            opt(&mut s, take_card);
            s.push(':');
            opt(&mut s, played);
            s.push(':');
            card_arr(&mut s, cards.as_ref());
        }
        Action::BeginActions => s.push_str("ba"),
        Action::Tax { system, building } => {
            let _ = write!(s, "tx:{}:{building}", system.0);
        }
        Action::BuildShip { system, building } => {
            let _ = write!(s, "bs:{}:{building}", system.0);
        }
        Action::BuildBuilding { system, kind } => {
            let k = match kind {
                BuildingKind::City => 'c',
                BuildingKind::Starport => 's',
            };
            let _ = write!(s, "bb:{}:{k}", system.0);
        }
        Action::CardAction {
            card,
            name,
            gain,
            count,
            slot,
            system,
            building,
            give_slot,
        } => {
            let _ = write!(s, "ca:{}:{}:", card.0, name.printed());
            resource_arr(&mut s, gain.as_ref());
            s.push(':');
            opt(&mut s, count.map(|x| x as usize));
            s.push(':');
            opt(&mut s, slot.map(|x| x as usize));
            s.push(':');
            opt(&mut s, system);
            s.push(':');
            opt(&mut s, building.map(|x| x as usize));
            s.push(':');
            opt(&mut s, give_slot.map(|x| x as usize));
        }
        Action::Move { from, to, ships } => {
            let _ = write!(s, "mv:{}:{}:{ships}", from.0, to.0);
        }
        Action::Catapult { to, ships } => {
            let _ = write!(s, "ct:{}:{ships}", to.0);
        }
        Action::CatapultStop => s.push_str("cs"),
        Action::Repair { system, building } => {
            let _ = write!(s, "rp:{}:", system.0);
            match building {
                None => s.push('n'),
                Some(b) => {
                    let _ = write!(s, "{b}");
                }
            }
        }
        Action::Influence { slot } => {
            let _ = write!(s, "in:{slot}");
        }
        Action::Secure { slot } => {
            let _ = write!(s, "se:{slot}");
        }
        Action::Battle {
            system,
            defender,
            assault,
            skirmish,
            raid,
        } => {
            let _ = write!(
                s,
                "bt:{}:{}:{assault}/{skirmish}/{raid}",
                system.0, defender.0
            );
        }
        Action::EndTurn => s.push_str("et"),
        Action::AssignSelf { fresh } => {
            let _ = write!(s, "as:{}", fresh as u8);
        }
        Action::AssignHit { target } => match target {
            HitTarget::Ship { fresh } => {
                let _ = write!(s, "ah:s:{}", fresh as u8);
            }
            HitTarget::Building { building } => {
                let _ = write!(s, "ah:b:{building}");
            }
        },
        Action::RaidResource { slot } => {
            let _ = write!(s, "rr:{slot}");
        }
        Action::RaidCard { card } => {
            let _ = write!(s, "rc:{}", card.0);
        }
        Action::RaidDone => s.push_str("rd"),
        Action::RerollSkirmish { count } => {
            let _ = write!(s, "rs:{count}");
        }
        Action::PeekTarget { target } => {
            s.push_str("pt:");
            match target {
                None => s.push('n'),
                Some(p) => {
                    let _ = write!(s, "{}", p.0);
                }
            }
        }
        Action::PeekSwap { give, take } => {
            let _ = write!(s, "ps:{}:{}", give.0, take.0);
        }
        Action::PeekSwapSkip => s.push_str("pk"),
        Action::Vox {
            cluster,
            ambition,
            resource,
            system,
            building,
            seize,
            target,
            card,
        } => {
            s.push_str("vx:");
            opt(&mut s, cluster.map(|x| x as usize));
            s.push(':');
            if let Some(a) = ambition {
                s.push_str(ambition_name(a));
            }
            s.push(':');
            if let Some(r) = resource {
                s.push_str(resource_name(r));
            }
            s.push(':');
            opt(&mut s, system);
            s.push(':');
            opt(&mut s, building.map(|x| x as usize));
            s.push(':');
            if let Some(sz) = seize {
                let _ = write!(s, "{}", sz as u8);
            }
            s.push(':');
            opt(&mut s, target);
            s.push(':');
            opt(&mut s, card);
        }
        Action::VoxSkip => s.push_str("vs"),
        Action::Reinforce { system } => {
            let _ = write!(s, "rf:{}", system.0);
        }
    }
    s
}

// --- decoding ---------------------------------------------------------------

fn parse_u8(s: &str) -> Option<u8> {
    s.parse().ok()
}

fn parse_opt_u8(s: &str) -> Option<Option<u8>> {
    if s.is_empty() {
        Some(None)
    } else {
        parse_u8(s).map(Some)
    }
}

fn parse_ambition(s: &str) -> Option<AmbitionId> {
    AMBITION_NAMES
        .iter()
        .position(|n| *n == s)
        .and_then(AmbitionId::from_index)
}

fn parse_resource(s: &str) -> Option<ResourceType> {
    RESOURCE_NAMES
        .iter()
        .position(|n| *n == s)
        .and_then(ResourceType::from_index)
}

/// `''` -> absent, `[]` -> empty list, `[a.b]` -> items parsed by `item`.
fn parse_arr<T: Copy + Default, const N: usize>(
    s: &str,
    item: impl Fn(&str) -> Option<T>,
) -> Option<Option<InlineVec<T, N>>> {
    if s.is_empty() {
        return Some(None);
    }
    let body = s.strip_prefix('[')?.strip_suffix(']')?;
    let mut out = InlineVec::new();
    if body.is_empty() {
        return Some(Some(out));
    }
    for part in body.split('.') {
        if out.is_full() {
            return None;
        }
        out.push(item(part)?);
    }
    Some(Some(out))
}

/// Split into exactly `N` fields on ':' (empty fields preserved).
fn fields<const N: usize>(s: &str) -> Option<[&str; N]> {
    let mut out = [""; N];
    let mut it = s.split(':');
    for slot in &mut out {
        *slot = it.next()?;
    }
    if it.next().is_some() {
        return None;
    }
    Some(out)
}

/// Decode a canonical key back into the action it names. Total inverse of
/// [`encode_action`]: `decode_action(&encode_action(a)) == Some(a)` for every
/// well-formed action.
pub fn decode_action(key: &str) -> Option<Action> {
    let (tag, rest) = match key.split_once(':') {
        Some((tag, rest)) => (tag, rest),
        None => (key, ""),
    };
    match tag {
        "ld" => Some(Action::Lead {
            card: ActionCardId(parse_u8(rest)?),
        }),
        "fo" => {
            let [card, mode] = fields::<2>(rest)?;
            let mode = match mode {
                "s" => FollowMode::Surpass,
                "c" => FollowMode::Copy,
                "p" => FollowMode::Pivot,
                _ => return None,
            };
            Some(Action::Follow {
                card: ActionCardId(parse_u8(card)?),
                mode,
            })
        }
        "pi" if rest.is_empty() => Some(Action::PassInitiative),
        "mu" => match rest {
            "0" => Some(Action::Mulligan { take: false }),
            "1" => Some(Action::Mulligan { take: true }),
            _ => None,
        },
        "da" => Some(Action::DeclareAmbition {
            ambition: parse_ambition(rest)?,
        }),
        "sz" => Some(Action::Seize {
            card: ActionCardId(parse_u8(rest)?),
        }),
        "sr" => Some(Action::SpendResource {
            slot: parse_u8(rest)?,
        }),
        "sa" => {
            let [slot, r] = fields::<2>(rest)?;
            Some(Action::SpendResourceAs {
                slot: parse_u8(slot)?,
                spend_as: parse_resource(r)?,
            })
        }
        "cp" => {
            let [card, system, slot, target, take_card, played, cards] = fields::<7>(rest)?;
            Some(Action::CardPrelude {
                card: CourtCardId(parse_u8(card)?),
                system: parse_opt_u8(system)?.map(SystemId),
                slot: parse_opt_u8(slot)?,
                target: parse_opt_u8(target)?.map(Player),
                take_card: parse_opt_u8(take_card)?.map(CourtCardId),
                played: parse_opt_u8(played)?.map(ActionCardId),
                cards: parse_arr(cards, |c| parse_u8(c).map(ActionCardId))?,
            })
        }
        "ba" if rest.is_empty() => Some(Action::BeginActions),
        "tx" => {
            let [system, building] = fields::<2>(rest)?;
            Some(Action::Tax {
                system: SystemId(parse_u8(system)?),
                building: parse_u8(building)?,
            })
        }
        "bs" => {
            let [system, building] = fields::<2>(rest)?;
            Some(Action::BuildShip {
                system: SystemId(parse_u8(system)?),
                building: parse_u8(building)?,
            })
        }
        "bb" => {
            let [system, kind] = fields::<2>(rest)?;
            let kind = match kind {
                "c" => BuildingKind::City,
                "s" => BuildingKind::Starport,
                _ => return None,
            };
            Some(Action::BuildBuilding {
                system: SystemId(parse_u8(system)?),
                kind,
            })
        }
        "ca" => {
            let [card, name, gain, count, slot, system, building, give_slot] = fields::<8>(rest)?;
            Some(Action::CardAction {
                card: CourtCardId(parse_u8(card)?),
                name: CardActionName::from_printed(name)?,
                gain: parse_arr(gain, parse_resource)?,
                count: parse_opt_u8(count)?,
                slot: parse_opt_u8(slot)?,
                system: parse_opt_u8(system)?.map(SystemId),
                building: parse_opt_u8(building)?,
                give_slot: parse_opt_u8(give_slot)?,
            })
        }
        "mv" => {
            let [from, to, ships] = fields::<3>(rest)?;
            Some(Action::Move {
                from: SystemId(parse_u8(from)?),
                to: SystemId(parse_u8(to)?),
                ships: parse_u8(ships)?,
            })
        }
        "ct" => {
            let [to, ships] = fields::<2>(rest)?;
            Some(Action::Catapult {
                to: SystemId(parse_u8(to)?),
                ships: parse_u8(ships)?,
            })
        }
        "cs" if rest.is_empty() => Some(Action::CatapultStop),
        "rp" => {
            let [system, building] = fields::<2>(rest)?;
            let building = if building == "n" {
                None
            } else {
                Some(parse_u8(building)?)
            };
            Some(Action::Repair {
                system: SystemId(parse_u8(system)?),
                building,
            })
        }
        "in" => Some(Action::Influence {
            slot: parse_u8(rest)?,
        }),
        "se" => Some(Action::Secure {
            slot: parse_u8(rest)?,
        }),
        "bt" => {
            let [system, defender, split] = fields::<3>(rest)?;
            let mut it = split.split('/');
            let assault = parse_u8(it.next()?)?;
            let skirmish = parse_u8(it.next()?)?;
            let raid = parse_u8(it.next()?)?;
            if it.next().is_some() {
                return None;
            }
            Some(Action::Battle {
                system: SystemId(parse_u8(system)?),
                defender: Player(parse_u8(defender)?),
                assault,
                skirmish,
                raid,
            })
        }
        "et" if rest.is_empty() => Some(Action::EndTurn),
        "as" => match rest {
            "0" => Some(Action::AssignSelf { fresh: false }),
            "1" => Some(Action::AssignSelf { fresh: true }),
            _ => None,
        },
        "ah" => {
            let [kind, arg] = fields::<2>(rest)?;
            let target = match kind {
                "s" => HitTarget::Ship {
                    fresh: match arg {
                        "0" => false,
                        "1" => true,
                        _ => return None,
                    },
                },
                "b" => HitTarget::Building {
                    building: parse_u8(arg)?,
                },
                _ => return None,
            };
            Some(Action::AssignHit { target })
        }
        "rr" => Some(Action::RaidResource {
            slot: parse_u8(rest)?,
        }),
        "rc" => Some(Action::RaidCard {
            card: CourtCardId(parse_u8(rest)?),
        }),
        "rd" if rest.is_empty() => Some(Action::RaidDone),
        "rs" => Some(Action::RerollSkirmish {
            count: parse_u8(rest)?,
        }),
        "pt" => {
            let target = if rest == "n" {
                None
            } else {
                Some(Player(parse_u8(rest)?))
            };
            Some(Action::PeekTarget { target })
        }
        "ps" => {
            let [give, take] = fields::<2>(rest)?;
            Some(Action::PeekSwap {
                give: ActionCardId(parse_u8(give)?),
                take: ActionCardId(parse_u8(take)?),
            })
        }
        "pk" if rest.is_empty() => Some(Action::PeekSwapSkip),
        "vx" => {
            let [
                cluster,
                ambition,
                resource,
                system,
                building,
                seize,
                target,
                card,
            ] = fields::<8>(rest)?;
            let seize = match seize {
                "" => None,
                "0" => Some(false),
                "1" => Some(true),
                _ => return None,
            };
            let ambition = if ambition.is_empty() {
                None
            } else {
                Some(parse_ambition(ambition)?)
            };
            let resource = if resource.is_empty() {
                None
            } else {
                Some(parse_resource(resource)?)
            };
            Some(Action::Vox {
                cluster: parse_opt_u8(cluster)?,
                ambition,
                resource,
                system: parse_opt_u8(system)?.map(SystemId),
                building: parse_opt_u8(building)?,
                seize,
                target: parse_opt_u8(target)?.map(Player),
                card: parse_opt_u8(card)?.map(CourtCardId),
            })
        }
        "vs" if rest.is_empty() => Some(Action::VoxSkip),
        "rf" => Some(Action::Reinforce {
            system: SystemId(parse_u8(rest)?),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_is_small() {
        // The plan targets <= 16 bytes; the multi-parameter variants
        // (`CardPrelude` with its 5 options + bounded card list) land at
        // 21 bytes without contortions. Assert the achieved bound.
        assert!(
            size_of::<Action>() <= 24,
            "Action is {} bytes",
            size_of::<Action>()
        );
    }

    // Ported from tests/encode.test.ts "distinguishes the cases JSON key
    // order or emptiness could blur".
    #[test]
    fn distinguishes_absent_from_empty_and_null_from_zero() {
        let bare = Action::CardPrelude {
            card: CourtCardId(16),
            system: None,
            slot: None,
            target: None,
            take_card: None,
            played: None,
            cards: None,
        };
        let empty = Action::CardPrelude {
            card: CourtCardId(16),
            system: None,
            slot: None,
            target: None,
            take_card: None,
            played: None,
            cards: Some(CardList::new()),
        };
        assert_ne!(encode_action(bare), encode_action(empty));

        assert_ne!(
            encode_action(Action::Repair {
                system: SystemId(3),
                building: None
            }),
            encode_action(Action::Repair {
                system: SystemId(3),
                building: Some(0)
            }),
        );
        assert_ne!(
            encode_action(Action::PeekTarget { target: None }),
            encode_action(Action::PeekTarget {
                target: Some(Player(0))
            }),
        );
        assert_ne!(
            encode_action(Action::AssignHit {
                target: HitTarget::Ship { fresh: true }
            }),
            encode_action(Action::AssignHit {
                target: HitTarget::Building { building: 1 }
            }),
        );
    }

    #[test]
    fn keys_match_the_ts_format() {
        // Spot checks against strings the TS encoder produces.
        assert_eq!(
            encode_action(Action::Lead {
                card: ActionCardId(9)
            }),
            "ld:9"
        );
        assert_eq!(
            encode_action(Action::Follow {
                card: ActionCardId(3),
                mode: FollowMode::Surpass
            }),
            "fo:3:s"
        );
        assert_eq!(encode_action(Action::PassInitiative), "pi");
        assert_eq!(
            encode_action(Action::DeclareAmbition {
                ambition: AmbitionId::Warlord
            }),
            "da:warlord"
        );
        assert_eq!(
            encode_action(Action::SpendResourceAs {
                slot: 2,
                spend_as: ResourceType::Fuel
            }),
            "sa:2:fuel"
        );
        assert_eq!(
            encode_action(Action::Battle {
                system: SystemId(7),
                defender: Player(2),
                assault: 3,
                skirmish: 1,
                raid: 0,
            }),
            "bt:7:2:3/1/0"
        );
        assert_eq!(
            encode_action(Action::CardPrelude {
                card: CourtCardId(16),
                system: None,
                slot: None,
                target: None,
                take_card: None,
                played: None,
                cards: Some(CardList::from_slice(&[ActionCardId(1), ActionCardId(4)])),
            }),
            "cp:16::::::[1.4]"
        );
        assert_eq!(
            encode_action(Action::Repair {
                system: SystemId(3),
                building: None
            }),
            "rp:3:n"
        );
    }
}
