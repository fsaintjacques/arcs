//! The action vocabulary, mirroring the TS `Action` union (types.ts) and the
//! canonical string keys of `src/engine/encode.ts` — plus the decoder the TS
//! side never needed.
//!
//! `Action` is `Copy + Eq + Hash` and small (asserted <= 16 bytes) so search
//! trees can key nodes by the enum itself.
//!
//! Where the TS union carries a bag of optional properties whose meaning
//! depends on which card is resolving — `vox`, `cardAction`, `cardPrelude` —
//! the Rust action carries a **choice enum** instead, so an illegal
//! combination of parameters is unrepresentable and every consumer gets an
//! exhaustive `match`.
//!
//! The flat bag survives only as a projection — [`VoxParts`],
//! [`CardActionParts`], [`PreludeParts`] — because three *encodings* want it
//! and none of them is the type: the canonical key ([`Display`], byte-identical
//! to `encodeAction`), the wasm JSON boundary, and the NN head targets.
//! Flattening is a property of the wire formats, not of the action.

use core::fmt::{self, Write as _};
use core::str::FromStr;

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
    ///
    /// [`crate::court::NewAction`] stores this variant rather than the printed
    /// string, so enumeration never parses a name; `printed` exists for the
    /// wire format alone.
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

impl fmt::Display for CardActionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.printed())
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

/// What a Guild card's `Prelude:` ability asks the player to name.
///
/// One variant per [`crate::court::PreludeAbility`] parameter shape — the
/// abilities that ask nothing (`ShipInEveryGate`, `FillSlots`,
/// `GainResources`, `SeizeInitiative`) share [`PreludeChoice::Bare`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PreludeChoice {
    /// The ability takes no parameter.
    #[default]
    Bare,
    /// `PlaceShips`: the system to place them in.
    System(SystemId),
    /// `StealResource`, and the resource half of `StealAny`.
    StealResource { target: Player, slot: u8 },
    /// The Guild-card half of `StealAny` (Silver-Tongues).
    StealCard { target: Player, card: CourtCardId },
    /// `ConvertResource` (Relic Fence): the slot to give up.
    ConvertResource { slot: u8 },
    /// `AttachUnion`: the face-up played card to attach to.
    Union { played: ActionCardId },
    /// `RecycleHand` (Farseers): the hand cards to discard. An empty list
    /// still recycles — it discards nothing and redraws Farseers itself.
    Recycle { cards: CardList },
}

/// The TS-shaped optional-field view of a [`PreludeChoice`].
///
/// Three encodings need the flat form — the canonical key ([`Display`]), the
/// wasm JSON boundary, and the NN head targets — so it is derived once, here,
/// instead of being re-flattened at each of them. The choice enum is the type;
/// this is the projection the wire formats agreed on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PreludeParts {
    pub system: Option<SystemId>,
    pub slot: Option<u8>,
    pub target: Option<Player>,
    pub take_card: Option<CourtCardId>,
    pub played: Option<ActionCardId>,
    pub cards: Option<CardList>,
}

impl PreludeChoice {
    pub fn parts(self) -> PreludeParts {
        let mut p = PreludeParts::default();
        match self {
            PreludeChoice::Bare => {}
            PreludeChoice::System(s) => p.system = Some(s),
            PreludeChoice::StealResource { target, slot } => {
                p.target = Some(target);
                p.slot = Some(slot);
            }
            PreludeChoice::StealCard { target, card } => {
                p.target = Some(target);
                p.take_card = Some(card);
            }
            PreludeChoice::ConvertResource { slot } => p.slot = Some(slot),
            PreludeChoice::Union { played } => p.played = Some(played),
            PreludeChoice::Recycle { cards } => p.cards = Some(cards),
        }
        p
    }
}

/// A Guild card's new action and its parameters (p20).
///
/// The variant *is* the printed name, so a card action can no longer name one
/// ability and carry another's parameters.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CardActionChoice {
    /// Mining Interest: gain 1 Material.
    Manufacture,
    /// Shipping Interest: gain 1 Fuel.
    Synthesize,
    /// Prison Wardens: return Captives to gain one resource for each.
    Pressgang { gain: ResourceList },
    /// Prison Wardens: move `count` Captives to your Trophies.
    Execute { count: u8 },
    /// Court Enforcers: capture every Rival agent from a Court slot.
    Abduct { slot: u8 },
    /// Elder Broker: swap a resource with the owner of a city you control.
    Trade {
        system: SystemId,
        building: u8,
        /// Their slot, holding a resource of the city's planet type.
        slot: u8,
        /// Your slot, holding a type they do not have.
        give_slot: u8,
    },
}

impl CardActionChoice {
    /// The printed name this choice takes, which is what its pip cost is
    /// looked up by.
    pub const fn name(self) -> CardActionName {
        match self {
            CardActionChoice::Manufacture => CardActionName::Manufacture,
            CardActionChoice::Synthesize => CardActionName::Synthesize,
            CardActionChoice::Pressgang { .. } => CardActionName::Pressgang,
            CardActionChoice::Execute { .. } => CardActionName::Execute,
            CardActionChoice::Abduct { .. } => CardActionName::Abduct,
            CardActionChoice::Trade { .. } => CardActionName::Trade,
        }
    }
}

/// The TS-shaped optional-field view of a [`CardActionChoice`]. The printed
/// name is not here — it is [`CardActionChoice::name`]. See [`PreludeParts`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CardActionParts {
    pub gain: Option<ResourceList>,
    pub count: Option<u8>,
    pub slot: Option<u8>,
    pub system: Option<SystemId>,
    pub building: Option<u8>,
    pub give_slot: Option<u8>,
}

impl CardActionChoice {
    pub fn parts(self) -> CardActionParts {
        let mut p = CardActionParts::default();
        match self {
            CardActionChoice::Manufacture | CardActionChoice::Synthesize => {}
            CardActionChoice::Pressgang { gain } => p.gain = Some(gain),
            CardActionChoice::Execute { count } => p.count = Some(count),
            CardActionChoice::Abduct { slot } => p.slot = Some(slot),
            CardActionChoice::Trade {
                system,
                building,
                slot,
                give_slot,
            } => {
                p.system = Some(system);
                p.building = Some(building);
                p.slot = Some(slot);
                p.give_slot = Some(give_slot);
            }
        }
        p
    }
}

/// What a Vox card's `When Secured` effect asks the securing player.
///
/// One variant per [`crate::court::VoxEffect`] that needs a decision; Call to
/// Action (`DrawFromDiscardBottom`) resolves inline and never reaches here.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum VoxChoice {
    /// Mass Uprising: place 1 ship in each system of this cluster.
    Cluster(u8),
    /// Populist Demands: declare this ambition.
    Declare(AmbitionId),
    /// Outrage Spreads: every player provokes Outrage of this type.
    Outrage(ResourceType),
    /// Song of Freedom: return this city, and maybe seize the initiative.
    ReturnCity {
        system: SystemId,
        building: u8,
        seize: bool,
    },
    /// Guild Struggle: steal this Guild card from this Rival.
    Steal { target: Player, card: CourtCardId },
}

/// The TS-shaped optional-field view of a [`VoxChoice`]. See [`PreludeParts`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct VoxParts {
    pub cluster: Option<u8>,
    pub ambition: Option<AmbitionId>,
    pub resource: Option<ResourceType>,
    pub system: Option<SystemId>,
    pub building: Option<u8>,
    pub seize: Option<bool>,
    pub target: Option<Player>,
    pub card: Option<CourtCardId>,
}

impl VoxChoice {
    pub fn parts(self) -> VoxParts {
        let mut p = VoxParts::default();
        match self {
            VoxChoice::Cluster(c) => p.cluster = Some(c),
            VoxChoice::Declare(a) => p.ambition = Some(a),
            VoxChoice::Outrage(r) => p.resource = Some(r),
            VoxChoice::ReturnCity {
                system,
                building,
                seize,
            } => {
                p.system = Some(system);
                p.building = Some(building);
                p.seize = Some(seize);
            }
            VoxChoice::Steal { target, card } => {
                p.target = Some(target);
                p.card = Some(card);
            }
        }
        p
    }
}

/// Every move a player can make, mirroring the TS `Action` union variant for
/// variant (types.ts:392).
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
    /// Use a Guild card's `Prelude:` ability (p20).
    CardPrelude {
        card: CourtCardId,
        choice: PreludeChoice,
    },
    BeginActions,
    // --- actions ---
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
    /// replaces (p20).
    CardAction {
        card: CourtCardId,
        choice: CardActionChoice,
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
    // --- battle assignment ---
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
    // --- Farseers ---
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
    // --- Vox `When Secured` ---
    /// Resolve the pending Vox card.
    Vox(VoxChoice),
    VoxSkip,
    // --- reinforce ---
    Reinforce {
        system: SystemId,
    },
    /// Place a just-gained resource in a free slot of raid-cost `tier`
    /// (p17). Only offered when [`crate::VariantDef::choose_placement`] is on;
    /// **no TS counterpart** — see the encoding note below.
    PlaceResource {
        tier: u8,
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

/// The TS `opt()`: `''` for absent.
fn opt<W: fmt::Write, T: Into<usize> + Copy>(out: &mut W, x: Option<T>) -> fmt::Result {
    match x {
        Some(x) => write!(out, "{}", x.into()),
        None => Ok(()),
    }
}

/// The TS `arr()`: `''` for absent, `[a.b.c]` for present.
fn card_arr<W: fmt::Write>(out: &mut W, cards: Option<&CardList>) -> fmt::Result {
    let Some(cards) = cards else { return Ok(()) };
    out.write_char('[')?;
    for (i, c) in cards.iter().enumerate() {
        if i > 0 {
            out.write_char('.')?;
        }
        write!(out, "{}", c.0)?;
    }
    out.write_char(']')
}

fn resource_arr<W: fmt::Write>(out: &mut W, gain: Option<&ResourceList>) -> fmt::Result {
    let Some(gain) = gain else { return Ok(()) };
    out.write_char('[')?;
    for (i, r) in gain.iter().enumerate() {
        if i > 0 {
            out.write_char('.')?;
        }
        out.write_str(resource_name(*r))?;
    }
    out.write_char(']')
}

/// The canonical key of an action — stable, compact, injective. Matches the
/// TS `encodeAction` byte for byte on every variant.
///
/// The three choice enums flatten back into TS's optional-field bags here, and
/// only here: the wire format is the compatibility surface, the type is not.
impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Action::Lead { card } => write!(f, "ld:{}", card.0),
            Action::Follow { card, mode } => {
                let m = match mode {
                    FollowMode::Surpass => 's',
                    FollowMode::Copy => 'c',
                    FollowMode::Pivot => 'p',
                };
                write!(f, "fo:{}:{m}", card.0)
            }
            Action::PassInitiative => f.write_str("pi"),
            Action::Mulligan { take } => write!(f, "mu:{}", take as u8),
            Action::DeclareAmbition { ambition } => write!(f, "da:{}", ambition_name(ambition)),
            Action::Seize { card } => write!(f, "sz:{}", card.0),
            Action::SpendResource { slot } => write!(f, "sr:{slot}"),
            Action::SpendResourceAs { slot, spend_as } => {
                write!(f, "sa:{slot}:{}", resource_name(spend_as))
            }
            Action::CardPrelude { card, choice } => {
                let p = choice.parts();
                write!(f, "cp:{}:", card.0)?;
                opt(f, p.system)?;
                f.write_char(':')?;
                opt(f, p.slot.map(usize::from))?;
                f.write_char(':')?;
                opt(f, p.target)?;
                f.write_char(':')?;
                opt(f, p.take_card)?;
                f.write_char(':')?;
                opt(f, p.played)?;
                f.write_char(':')?;
                card_arr(f, p.cards.as_ref())
            }
            Action::BeginActions => f.write_str("ba"),
            Action::Tax { system, building } => write!(f, "tx:{}:{building}", system.0),
            Action::BuildShip { system, building } => write!(f, "bs:{}:{building}", system.0),
            Action::BuildBuilding { system, kind } => {
                let k = match kind {
                    BuildingKind::City => 'c',
                    BuildingKind::Starport => 's',
                };
                write!(f, "bb:{}:{k}", system.0)
            }
            Action::CardAction { card, choice } => {
                let p = choice.parts();
                write!(f, "ca:{}:{}:", card.0, choice.name().printed())?;
                resource_arr(f, p.gain.as_ref())?;
                f.write_char(':')?;
                opt(f, p.count.map(usize::from))?;
                f.write_char(':')?;
                opt(f, p.slot.map(usize::from))?;
                f.write_char(':')?;
                opt(f, p.system)?;
                f.write_char(':')?;
                opt(f, p.building.map(usize::from))?;
                f.write_char(':')?;
                opt(f, p.give_slot.map(usize::from))
            }
            Action::Move { from, to, ships } => write!(f, "mv:{}:{}:{ships}", from.0, to.0),
            Action::Catapult { to, ships } => write!(f, "ct:{}:{ships}", to.0),
            Action::CatapultStop => f.write_str("cs"),
            Action::Repair { system, building } => {
                write!(f, "rp:{}:", system.0)?;
                match building {
                    None => f.write_char('n'),
                    Some(b) => write!(f, "{b}"),
                }
            }
            Action::Influence { slot } => write!(f, "in:{slot}"),
            Action::Secure { slot } => write!(f, "se:{slot}"),
            Action::Battle {
                system,
                defender,
                assault,
                skirmish,
                raid,
            } => write!(
                f,
                "bt:{}:{}:{assault}/{skirmish}/{raid}",
                system.0, defender.0
            ),
            Action::EndTurn => f.write_str("et"),
            Action::AssignSelf { fresh } => write!(f, "as:{}", fresh as u8),
            Action::AssignHit { target } => match target {
                HitTarget::Ship { fresh } => write!(f, "ah:s:{}", fresh as u8),
                HitTarget::Building { building } => write!(f, "ah:b:{building}"),
            },
            Action::RaidResource { slot } => write!(f, "rr:{slot}"),
            Action::RaidCard { card } => write!(f, "rc:{}", card.0),
            Action::RaidDone => f.write_str("rd"),
            Action::RerollSkirmish { count } => write!(f, "rs:{count}"),
            Action::PeekTarget { target } => {
                f.write_str("pt:")?;
                match target {
                    None => f.write_char('n'),
                    Some(p) => write!(f, "{}", p.0),
                }
            }
            Action::PeekSwap { give, take } => write!(f, "ps:{}:{}", give.0, take.0),
            Action::PeekSwapSkip => f.write_str("pk"),
            Action::Vox(choice) => {
                let p = choice.parts();
                f.write_str("vx:")?;
                opt(f, p.cluster.map(usize::from))?;
                f.write_char(':')?;
                if let Some(a) = p.ambition {
                    f.write_str(ambition_name(a))?;
                }
                f.write_char(':')?;
                if let Some(r) = p.resource {
                    f.write_str(resource_name(r))?;
                }
                f.write_char(':')?;
                opt(f, p.system)?;
                f.write_char(':')?;
                opt(f, p.building.map(usize::from))?;
                f.write_char(':')?;
                if let Some(sz) = p.seize {
                    write!(f, "{}", sz as u8)?;
                }
                f.write_char(':')?;
                opt(f, p.target)?;
                f.write_char(':')?;
                opt(f, p.card)
            }
            Action::VoxSkip => f.write_str("vs"),
            Action::Reinforce { system } => write!(f, "rf:{}", system.0),
            // `pr` is a Rust-only tag: the TS engine has no placement action,
            // so this key can never appear in a trace exported from TS.
            Action::PlaceResource { tier } => write!(f, "pr:{tier}"),
        }
    }
}

/// The canonical key of an action. (`encodeAction` in encode.ts; the same
/// thing as `action.to_string()`, kept under the TS name.)
pub fn encode_action(a: Action) -> String {
    let mut s = String::with_capacity(24);
    let _ = write!(s, "{a}");
    s
}

// --- decoding ---------------------------------------------------------------

/// A canonical key that names no action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ParseActionError;

impl fmt::Display for ParseActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a canonical action key")
    }
}

impl core::error::Error for ParseActionError {}

impl FromStr for Action {
    type Err = ParseActionError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        decode_action(key).ok_or(ParseActionError)
    }
}

fn parse_u8(s: &str) -> Option<u8> {
    s.parse().ok()
}

fn parse_bool(s: &str) -> Option<bool> {
    match s {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
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
/// [`Display`]: `key.parse::<Action>()` round-trips every well-formed action.
/// (Also reachable as `Action::from_str`; this is the TS-named form.)
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
        "mu" => Some(Action::Mulligan {
            take: parse_bool(rest)?,
        }),
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
            // Which fields are populated names the choice. Order matters:
            // StealCard sets `target` too, and Recycle is the only shape that
            // can legitimately carry an empty payload.
            let recycle = parse_arr(cards, |c| parse_u8(c).map(ActionCardId))?;
            let choice = if let Some(cards) = recycle {
                PreludeChoice::Recycle { cards }
            } else if !system.is_empty() {
                PreludeChoice::System(SystemId(parse_u8(system)?))
            } else if !played.is_empty() {
                PreludeChoice::Union {
                    played: ActionCardId(parse_u8(played)?),
                }
            } else if !take_card.is_empty() {
                PreludeChoice::StealCard {
                    target: Player(parse_u8(target)?),
                    card: CourtCardId(parse_u8(take_card)?),
                }
            } else if !target.is_empty() {
                PreludeChoice::StealResource {
                    target: Player(parse_u8(target)?),
                    slot: parse_u8(slot)?,
                }
            } else if !slot.is_empty() {
                PreludeChoice::ConvertResource {
                    slot: parse_u8(slot)?,
                }
            } else {
                PreludeChoice::Bare
            };
            Some(Action::CardPrelude {
                card: CourtCardId(parse_u8(card)?),
                choice,
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
            // The printed name selects the variant; each then claims exactly
            // the fields it owns and requires them to be present.
            let choice = match CardActionName::from_printed(name)? {
                CardActionName::Manufacture => CardActionChoice::Manufacture,
                CardActionName::Synthesize => CardActionChoice::Synthesize,
                CardActionName::Pressgang => CardActionChoice::Pressgang {
                    gain: parse_arr(gain, parse_resource)??,
                },
                CardActionName::Execute => CardActionChoice::Execute {
                    count: parse_u8(count)?,
                },
                CardActionName::Abduct => CardActionChoice::Abduct {
                    slot: parse_u8(slot)?,
                },
                CardActionName::Trade => CardActionChoice::Trade {
                    system: SystemId(parse_u8(system)?),
                    building: parse_u8(building)?,
                    slot: parse_u8(slot)?,
                    give_slot: parse_u8(give_slot)?,
                },
            };
            Some(Action::CardAction {
                card: CourtCardId(parse_u8(card)?),
                choice,
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
        "as" => Some(Action::AssignSelf {
            fresh: parse_bool(rest)?,
        }),
        "ah" => {
            let [kind, arg] = fields::<2>(rest)?;
            let target = match kind {
                "s" => HitTarget::Ship {
                    fresh: parse_bool(arg)?,
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
            // As with `cp`, the populated fields name the choice. An all-empty
            // bag names nothing — `vs` (VoxSkip) is how a player declines.
            let choice = if !cluster.is_empty() {
                VoxChoice::Cluster(parse_u8(cluster)?)
            } else if !ambition.is_empty() {
                VoxChoice::Declare(parse_ambition(ambition)?)
            } else if !resource.is_empty() {
                VoxChoice::Outrage(parse_resource(resource)?)
            } else if !system.is_empty() {
                VoxChoice::ReturnCity {
                    system: SystemId(parse_u8(system)?),
                    building: parse_u8(building)?,
                    seize: parse_bool(seize)?,
                }
            } else if !target.is_empty() {
                VoxChoice::Steal {
                    target: Player(parse_u8(target)?),
                    card: CourtCardId(parse_u8(card)?),
                }
            } else {
                return None;
            };
            Some(Action::Vox(choice))
        }
        "vs" if rest.is_empty() => Some(Action::VoxSkip),
        "rf" => Some(Action::Reinforce {
            system: SystemId(parse_u8(rest)?),
        }),
        "pr" => Some(Action::PlaceResource {
            tier: parse_u8(rest)?,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_is_small() {
        // The plan's target. Nesting the parameter bags into choice enums —
        // the widest was `Vox`'s eight `Option`s at 13 bytes of payload —
        // brought this back under the original budget from 24.
        assert!(
            size_of::<Action>() <= 16,
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
            choice: PreludeChoice::Bare,
        };
        let empty = Action::CardPrelude {
            card: CourtCardId(16),
            choice: PreludeChoice::Recycle {
                cards: CardList::new(),
            },
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
                choice: PreludeChoice::Recycle {
                    cards: CardList::from_slice(&[ActionCardId(1), ActionCardId(4)]),
                },
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

    /// The choice enums must not disturb the wire format: every shape still
    /// lands in the field slot `encode.ts` puts it in.
    ///
    /// Every expected key here was produced by running the TS `encodeAction`
    /// on the equivalent object literal, not derived from this file — they are
    /// the parity contract, so they are transcribed rather than computed.
    #[test]
    fn choice_enums_flatten_to_the_ts_field_bags() {
        for (action, key) in [
            (Action::Vox(VoxChoice::Cluster(3)), "vx:3:::::::"),
            (
                Action::Vox(VoxChoice::Declare(AmbitionId::Keeper)),
                "vx::keeper::::::",
            ),
            (
                Action::Vox(VoxChoice::Outrage(ResourceType::Psionic)),
                "vx:::psionic:::::",
            ),
            (
                Action::Vox(VoxChoice::ReturnCity {
                    system: SystemId(9),
                    building: 1,
                    seize: true,
                }),
                "vx::::9:1:1::",
            ),
            (
                Action::Vox(VoxChoice::Steal {
                    target: Player(2),
                    card: CourtCardId(7),
                }),
                "vx:::::::2:7",
            ),
            (
                Action::CardAction {
                    card: CourtCardId(11),
                    choice: CardActionChoice::Pressgang {
                        gain: ResourceList::from_slice(&[
                            ResourceType::Material,
                            ResourceType::Relic,
                        ]),
                    },
                },
                "ca:11:Pressgang:[material.relic]:::::",
            ),
            (
                Action::CardAction {
                    card: CourtCardId(11),
                    choice: CardActionChoice::Execute { count: 2 },
                },
                "ca:11:Execute::2::::",
            ),
            (
                Action::CardAction {
                    card: CourtCardId(22),
                    choice: CardActionChoice::Trade {
                        system: SystemId(5),
                        building: 0,
                        slot: 1,
                        give_slot: 2,
                    },
                },
                "ca:22:Trade:::1:5:0:2",
            ),
            (
                Action::CardPrelude {
                    card: CourtCardId(14),
                    choice: PreludeChoice::System(SystemId(6)),
                },
                "cp:14:6:::::",
            ),
            (
                Action::CardPrelude {
                    card: CourtCardId(19),
                    choice: PreludeChoice::StealCard {
                        target: Player(1),
                        card: CourtCardId(3),
                    },
                },
                "cp:19:::1:3::",
            ),
        ] {
            assert_eq!(encode_action(action), key);
            assert_eq!(key.parse::<Action>(), Ok(action), "round trip of {key}");
        }
    }

    #[test]
    fn from_str_rejects_a_bag_that_names_no_choice() {
        // The all-empty Vox bag was representable before the choice enum;
        // declining is `vs`.
        assert_eq!("vx:::::::".parse::<Action>(), Err(ParseActionError));
        assert_eq!("vs".parse::<Action>(), Ok(Action::VoxSkip));
    }
}
