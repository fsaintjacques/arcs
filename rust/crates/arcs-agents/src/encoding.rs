//! The global action encoding: factorized policy heads over the whole action
//! vocabulary, plus a flat key for tabular and debugging use. Plan §3.
//!
//! # Why factorized heads rather than one flat action space
//!
//! Arcs' 34 action variants are *parameterized* — a Move names a source, a
//! destination and a ship count; a Battle names a system, a defender and a
//! three-way dice split — so the flat space is enormous while any one node
//! offers a handful of actions. Measured on this engine by
//! `corpus_shape_is_what_the_encoding_documents` in `tests/encoding.rs`, which
//! regenerates the corpus on demand: **1,200 random games across 2/3/4 players
//! and 376,387 decision nodes contain 10,453 distinct actions across 33
//! distinct action kinds, with a mean of 10.7 and a maximum of 269 legal
//! actions per node.** (An independent sweep with a different random driver
//! put those at 10,741 / 32 / 10.7 / 312 — the shape is a property of the game,
//! not of a seed set.) A one-hot policy over ~10^4 outputs would be 99.9%
//! masked at every node, would have to be re-derived from a fresh enumeration
//! whenever the rules change, and would have no output at all for an action
//! variant that never appeared in the sample.
//!
//! **[`HeadTargets`] is therefore the primary policy interface.** A policy net
//! predicts one logit vector per field ([`HEAD_SIZES`], ~184 outputs in
//! total); an action's prior is the product of its fields' legality-masked
//! softmaxes. That is the standard AlphaZero treatment of parameterized moves,
//! and it buys two things this port specifically needs:
//!
//! - **No enumeration.** The head sizes are the *domains* of the parameters
//!   (systems, cards, counts), which are properties of the game's components,
//!   not of a sampled action list.
//! - **Expansion survival.** When Leaders & Lore adds action variants, the
//!   `kind` head grows by the new variants and every other head is unchanged —
//!   a trained net's system/card/count structure transfers instead of being
//!   invalidated.
//!
//! [`global_index`] flattens the same targets into one integer. It is a
//! **sparse key**, roughly 1.8 · 10^9 wide, for hash tables, trajectory logs
//! and debugging — *not* a one-hot policy index. Nothing should allocate an
//! array of that length.
//!
//! # Versioning
//!
//! [`HEAD_SIZES`] is versioned exactly the way `FEATURE_SIZE` is (see
//! [`crate::nn::features`]): a net is trained against one layout, and a rules
//! change that moves a field's cardinality — a new action variant, a bigger
//! map, a wider Court row — must force a retrain rather than silently remap
//! actions under a trained head. [`ENCODING_VERSION`] is the number to bump,
//! and `encoding_layout_is_pinned` is the test that makes forgetting to
//! impossible.
//!
//! # Known lossiness (deliberate, and tested)
//!
//! Two Guild-card variants carry *lists* that no fixed-width head can hold:
//! Farseers' recycle names a subset of the hand (`CardPrelude::cards`) and
//! Pressgang names a multiset of gained resources (`CardAction::gain`). Their
//! targets record the list's **length**, so two recycles of the same size map
//! to the same target. In the corpus above that collapses 10,453 distinct
//! actions into 10,019 distinct targets — 4% of the vocabulary, all of it
//! inside those two Guild abilities. A policy that must tell them apart needs
//! an extra head, and adding one is a visible, [`ENCODING_VERSION`]-bumping
//! change. Every other action kind is injective;
//! `action_targets_is_injective_outside_the_list_variants` asserts exactly
//! that over the sampled corpus.

use arcs_engine::cards::ACTION_CARD_COUNT;
use arcs_engine::court::COURT_CARD_COUNT;
use arcs_engine::map::SYSTEM_COUNT;
use arcs_engine::{Action, CardActionName, FollowMode, HitTarget};

/// Bump when any field's cardinality or meaning changes. A net trained
/// against an older version must be retrained, not reinterpreted.
pub const ENCODING_VERSION: u32 = 1;

/// Cardinality of each head, in [`HeadTargets`] field order.
///
/// Every field reserves its **largest value as the "unused" sentinel**, so a
/// real parameter keeps its natural encoding (system 3 is 3, slot 0 is 0,
/// count 0 is 0) and a policy head does not have to learn an off-by-one.
pub const HEAD_SIZES: HeadSizes = HeadSizes {
    // The 34 `Action` variants.
    kind: 34,
    // 28 action cards, then 31 Court cards, then the sentinel.
    card: (ACTION_CARD_COUNT + COURT_CARD_COUNT + 1) as u16,
    // 24 systems, then the sentinel.
    system: (SYSTEM_COUNT + 1) as u16,
    // Resource slots 0..5, building indices 0..1, Court slots 0..3 and a
    // battle's raid dice 0..6 all live here; 7 is the sentinel.
    slot: 8,
    // Ship counts reach a player's whole fleet (15 ships), dice counts and
    // reroll counts reach 6, list lengths reach 7; 16 is the sentinel.
    count: 17,
    // Per-kind meaning: follow mode (3), ambition (5), resource type (5),
    // building kind (2), seat (4), Guild action name (6), a boolean (2);
    // 7 is the sentinel.
    mode: 8,
    // Second system (0..23), second action card (0..27), second dice
    // component (0..6); 31 is the sentinel.
    aux: 32,
};

/// The per-field cardinalities. A separate struct rather than an array so a
/// field can never be read at the wrong index.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HeadSizes {
    pub kind: u16,
    pub card: u16,
    pub system: u16,
    pub slot: u16,
    pub count: u16,
    pub mode: u16,
    pub aux: u16,
}

impl HeadSizes {
    /// Total policy outputs across all heads — the width of the concatenated
    /// logit vector a policy net produces.
    pub const fn total_outputs(self) -> u32 {
        self.kind as u32
            + self.card as u32
            + self.system as u32
            + self.slot as u32
            + self.count as u32
            + self.mode as u32
            + self.aux as u32
    }

    /// Width of the flattened [`global_index`] space.
    pub const fn global_span(self) -> u64 {
        self.kind as u64
            * self.card as u64
            * self.system as u64
            * self.slot as u64
            * self.count as u64
            * self.mode as u64
            * self.aux as u64
    }
}

/// The sentinel value of each field: "this action does not use this head".
pub const KIND_NONE: u8 = (HEAD_SIZES.kind - 1) as u8;
pub const CARD_NONE: u8 = (HEAD_SIZES.card - 1) as u8;
pub const SYSTEM_NONE: u8 = (HEAD_SIZES.system - 1) as u8;
pub const SLOT_NONE: u8 = (HEAD_SIZES.slot - 1) as u8;
pub const COUNT_NONE: u8 = (HEAD_SIZES.count - 1) as u8;
pub const MODE_NONE: u8 = (HEAD_SIZES.mode - 1) as u8;
pub const AUX_NONE: u8 = (HEAD_SIZES.aux - 1) as u8;

/// Where a Court card starts in the shared `card` head.
const COURT_CARD_BASE: u8 = ACTION_CARD_COUNT as u8;

/// One action, factorized into per-head targets.
///
/// `kind` is the `Action` discriminant; the rest are the parameters, with
/// per-kind meanings documented on [`action_targets`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HeadTargets {
    pub kind: u8,
    pub card: u8,
    pub system: u8,
    pub slot: u8,
    pub count: u8,
    pub mode: u8,
    pub aux: u8,
}

impl Default for HeadTargets {
    /// Every head at its sentinel — the shape of a parameterless action.
    fn default() -> Self {
        HeadTargets {
            kind: KIND_NONE,
            card: CARD_NONE,
            system: SYSTEM_NONE,
            slot: SLOT_NONE,
            count: COUNT_NONE,
            mode: MODE_NONE,
            aux: AUX_NONE,
        }
    }
}

impl HeadTargets {
    fn of(kind: ActionKindId) -> Self {
        HeadTargets {
            kind: kind as u8,
            ..Default::default()
        }
    }
}

/// The `kind` head's vocabulary — one value per `Action` variant, in the
/// declaration order of `arcs_engine::Action`.
///
/// Written out rather than derived from `core::mem::discriminant` because the
/// discriminant of a Rust enum is not a stable, observable number, and this
/// one is written into training data.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ActionKindId {
    Lead = 0,
    Follow = 1,
    PassInitiative = 2,
    Mulligan = 3,
    DeclareAmbition = 4,
    Seize = 5,
    SpendResource = 6,
    SpendResourceAs = 7,
    CardPrelude = 8,
    BeginActions = 9,
    Tax = 10,
    BuildShip = 11,
    BuildBuilding = 12,
    CardAction = 13,
    Move = 14,
    Catapult = 15,
    CatapultStop = 16,
    Repair = 17,
    Influence = 18,
    Secure = 19,
    Battle = 20,
    EndTurn = 21,
    AssignSelf = 22,
    AssignHit = 23,
    RaidResource = 24,
    RaidCard = 25,
    RaidDone = 26,
    RerollSkirmish = 27,
    PeekTarget = 28,
    PeekSwap = 29,
    PeekSwapSkip = 30,
    Vox = 31,
    VoxSkip = 32,
    Reinforce = 33,
}

/// Clamp a count into the `count` head's domain. Counts larger than a whole
/// fleet cannot occur, so this only ever fires on a corrupt state.
#[inline]
fn count_of(n: u8) -> u8 {
    n.min(COUNT_NONE - 1)
}

#[inline]
fn aux_of(n: u8) -> u8 {
    n.min(AUX_NONE - 1)
}

/// Factorize an action into per-head targets. A **total** function: an
/// exhaustive `match`, so a new `Action` variant is a compile error here
/// rather than a silently mis-encoded training sample.
///
/// Per-kind field meanings (the sentinel means "unused"):
///
/// | kind | card | system | slot | count | mode | aux |
/// |---|---|---|---|---|---|---|
/// | `Lead`/`Seize` | action card | | | | | |
/// | `Follow` | action card | | | | follow mode | |
/// | `Mulligan` | | | | | take? | |
/// | `DeclareAmbition` | | | | | ambition | |
/// | `SpendResource(As)` | | | slot | | spend-as type | |
/// | `CardPrelude` | Court card | system | slot | recycle size | target seat | taken Court card, else played action card |
/// | `Tax`/`BuildShip` | | system | building | | | |
/// | `BuildBuilding` | | system | | | building kind | |
/// | `CardAction` | Court card | system | slot, else give-slot | count, else gain size | action name | building |
/// | `Move` | | from | | ships | | to |
/// | `Catapult` | | to | | ships | | |
/// | `Repair` | | system | building | | | |
/// | `Influence`/`Secure`/`RaidResource` | | | Court/resource slot | | | |
/// | `Battle` | | system | raid dice | assault dice | defender seat | skirmish dice |
/// | `AssignSelf` | | | | | fresh? | |
/// | `AssignHit` | | | building | | 0 damaged ship / 1 fresh ship / 2 building | |
/// | `RaidCard` | Court card | | | | | |
/// | `RerollSkirmish` | | | | dice | | |
/// | `PeekTarget` | | | | | target seat | |
/// | `PeekSwap` | given card | | | | | taken card |
/// | `Vox` | Court card | system | building | cluster | ambition, else resource | target seat, else seize? |
/// | `Reinforce` | | gate | | | | |
///
/// Parameterless variants (`PassInitiative`, `BeginActions`, `CatapultStop`,
/// `EndTurn`, `RaidDone`, `PeekSwapSkip`, `VoxSkip`) carry only their kind.
pub fn action_targets(a: Action) -> HeadTargets {
    use ActionKindId as K;
    match a {
        Action::Lead { card } => HeadTargets {
            card: card.0,
            ..HeadTargets::of(K::Lead)
        },
        Action::Follow { card, mode } => HeadTargets {
            card: card.0,
            mode: match mode {
                FollowMode::Surpass => 0,
                FollowMode::Copy => 1,
                FollowMode::Pivot => 2,
            },
            ..HeadTargets::of(K::Follow)
        },
        Action::PassInitiative => HeadTargets::of(K::PassInitiative),
        Action::Mulligan { take } => HeadTargets {
            mode: take as u8,
            ..HeadTargets::of(K::Mulligan)
        },
        Action::DeclareAmbition { ambition } => HeadTargets {
            mode: ambition.as_index() as u8,
            ..HeadTargets::of(K::DeclareAmbition)
        },
        Action::Seize { card } => HeadTargets {
            card: card.0,
            ..HeadTargets::of(K::Seize)
        },
        Action::SpendResource { slot } => HeadTargets {
            slot,
            ..HeadTargets::of(K::SpendResource)
        },
        Action::SpendResourceAs { slot, spend_as } => HeadTargets {
            slot,
            mode: spend_as.as_index() as u8,
            ..HeadTargets::of(K::SpendResourceAs)
        },
        Action::CardPrelude {
            card,
            system,
            slot,
            target,
            take_card,
            played,
            cards,
        } => HeadTargets {
            card: COURT_CARD_BASE + card.0,
            system: system.map_or(SYSTEM_NONE, |s| s.0),
            slot: slot.unwrap_or(SLOT_NONE),
            // Lossy by design: the recycled hand subset is summarised by its
            // size. See the module docs.
            count: cards.map_or(COUNT_NONE, |c| count_of(c.len() as u8)),
            mode: target.map_or(MODE_NONE, |p| p.0),
            aux: take_card
                .map(|c| aux_of(c.0))
                .or(played.map(|c| aux_of(c.0)))
                .unwrap_or(AUX_NONE),
            ..HeadTargets::of(K::CardPrelude)
        },
        Action::BeginActions => HeadTargets::of(K::BeginActions),
        Action::Tax { system, building } => HeadTargets {
            system: system.0,
            slot: building,
            ..HeadTargets::of(K::Tax)
        },
        Action::BuildShip { system, building } => HeadTargets {
            system: system.0,
            slot: building,
            ..HeadTargets::of(K::BuildShip)
        },
        Action::BuildBuilding { system, kind } => HeadTargets {
            system: system.0,
            mode: kind.as_index() as u8,
            ..HeadTargets::of(K::BuildBuilding)
        },
        Action::CardAction {
            card,
            name,
            gain,
            count,
            slot,
            system,
            building,
            give_slot,
        } => HeadTargets {
            card: COURT_CARD_BASE + card.0,
            system: system.map_or(SYSTEM_NONE, |s| s.0),
            slot: slot.or(give_slot).unwrap_or(SLOT_NONE),
            // Lossy by design: a Pressgang gain is summarised by its size.
            count: count
                .or(gain.map(|g| g.len() as u8))
                .map_or(COUNT_NONE, count_of),
            mode: match name {
                CardActionName::Manufacture => 0,
                CardActionName::Synthesize => 1,
                CardActionName::Pressgang => 2,
                CardActionName::Execute => 3,
                CardActionName::Abduct => 4,
                CardActionName::Trade => 5,
            },
            aux: building.map_or(AUX_NONE, aux_of),
            ..HeadTargets::of(K::CardAction)
        },
        Action::Move { from, to, ships } => HeadTargets {
            system: from.0,
            count: count_of(ships),
            aux: aux_of(to.0),
            ..HeadTargets::of(K::Move)
        },
        Action::Catapult { to, ships } => HeadTargets {
            system: to.0,
            count: count_of(ships),
            ..HeadTargets::of(K::Catapult)
        },
        Action::CatapultStop => HeadTargets::of(K::CatapultStop),
        Action::Repair { system, building } => HeadTargets {
            system: system.0,
            slot: building.unwrap_or(SLOT_NONE),
            ..HeadTargets::of(K::Repair)
        },
        Action::Influence { slot } => HeadTargets {
            slot,
            ..HeadTargets::of(K::Influence)
        },
        Action::Secure { slot } => HeadTargets {
            slot,
            ..HeadTargets::of(K::Secure)
        },
        Action::Battle {
            system,
            defender,
            assault,
            skirmish,
            raid,
        } => HeadTargets {
            system: system.0,
            slot: raid,
            count: count_of(assault),
            mode: defender.0,
            aux: aux_of(skirmish),
            ..HeadTargets::of(K::Battle)
        },
        Action::EndTurn => HeadTargets::of(K::EndTurn),
        Action::AssignSelf { fresh } => HeadTargets {
            mode: fresh as u8,
            ..HeadTargets::of(K::AssignSelf)
        },
        Action::AssignHit { target } => match target {
            HitTarget::Ship { fresh } => HeadTargets {
                mode: fresh as u8,
                ..HeadTargets::of(K::AssignHit)
            },
            HitTarget::Building { building } => HeadTargets {
                slot: building,
                mode: 2,
                ..HeadTargets::of(K::AssignHit)
            },
        },
        Action::RaidResource { slot } => HeadTargets {
            slot,
            ..HeadTargets::of(K::RaidResource)
        },
        Action::RaidCard { card } => HeadTargets {
            card: COURT_CARD_BASE + card.0,
            ..HeadTargets::of(K::RaidCard)
        },
        Action::RaidDone => HeadTargets::of(K::RaidDone),
        Action::RerollSkirmish { count } => HeadTargets {
            count: count_of(count),
            ..HeadTargets::of(K::RerollSkirmish)
        },
        Action::PeekTarget { target } => HeadTargets {
            mode: target.map_or(MODE_NONE, |p| p.0),
            ..HeadTargets::of(K::PeekTarget)
        },
        Action::PeekSwap { give, take } => HeadTargets {
            card: give.0,
            aux: aux_of(take.0),
            ..HeadTargets::of(K::PeekSwap)
        },
        Action::PeekSwapSkip => HeadTargets::of(K::PeekSwapSkip),
        Action::Vox {
            cluster,
            ambition,
            resource,
            system,
            building,
            seize,
            target,
            card,
        } => HeadTargets {
            card: card.map_or(CARD_NONE, |c| COURT_CARD_BASE + c.0),
            system: system.map_or(SYSTEM_NONE, |s| s.0),
            slot: building.unwrap_or(SLOT_NONE),
            count: cluster.map_or(COUNT_NONE, count_of),
            mode: ambition
                .map(|a| a.as_index() as u8)
                .or(resource.map(|r| r.as_index() as u8))
                .unwrap_or(MODE_NONE),
            // No Vox card asks for a seat *and* a seize choice, so the two
            // share `aux`; the seize flag sits above the seat range.
            aux: target
                .map(|p| p.0)
                .or(seize.map(|s| VOX_SEIZE_BASE + s as u8))
                .unwrap_or(AUX_NONE),
            ..HeadTargets::of(K::Vox)
        },
        Action::VoxSkip => HeadTargets::of(K::VoxSkip),
        Action::Reinforce { system } => HeadTargets {
            system: system.0,
            ..HeadTargets::of(K::Reinforce)
        },
    }
}

/// Where the Vox seize flag sits in the shared `aux` head, above the 4 seats.
const VOX_SEIZE_BASE: u8 = 4;

/// Mixed-radix flatten of [`HeadTargets`] into one integer.
///
/// **This is a sparse key, not a policy index.** The span is
/// `HEAD_SIZES.global_span()` ≈ 1.8 · 10^9, while a real game reaches ~10^4
/// distinct actions (see the module docs), so an array indexed by this would
/// be 99.999% empty. Use it as a hash-map key, a trajectory-log column, or a
/// tabular-agent key; use [`HeadTargets`] for policy heads.
///
/// Injective on `HeadTargets` by construction, since every field is checked
/// against its declared cardinality by `field_values_stay_inside_head_sizes`.
pub fn global_index(t: HeadTargets) -> u64 {
    let mut n = t.kind as u64;
    n = n * HEAD_SIZES.card as u64 + t.card as u64;
    n = n * HEAD_SIZES.system as u64 + t.system as u64;
    n = n * HEAD_SIZES.slot as u64 + t.slot as u64;
    n = n * HEAD_SIZES.count as u64 + t.count as u64;
    n = n * HEAD_SIZES.mode as u64 + t.mode as u64;
    n * HEAD_SIZES.aux as u64 + t.aux as u64
}

/// Undo [`global_index`]. The pair is a bijection on well-formed targets, so
/// a trajectory log can store one integer per decision and still recover the
/// heads a policy was trained against.
pub fn decode_global_index(n: u64) -> HeadTargets {
    let mut n = n;
    let aux = (n % HEAD_SIZES.aux as u64) as u8;
    n /= HEAD_SIZES.aux as u64;
    let mode = (n % HEAD_SIZES.mode as u64) as u8;
    n /= HEAD_SIZES.mode as u64;
    let count = (n % HEAD_SIZES.count as u64) as u8;
    n /= HEAD_SIZES.count as u64;
    let slot = (n % HEAD_SIZES.slot as u64) as u8;
    n /= HEAD_SIZES.slot as u64;
    let system = (n % HEAD_SIZES.system as u64) as u8;
    n /= HEAD_SIZES.system as u64;
    let card = (n % HEAD_SIZES.card as u64) as u8;
    n /= HEAD_SIZES.card as u64;
    HeadTargets {
        kind: n as u8,
        card,
        system,
        slot,
        count,
        mode,
        aux,
    }
}

/// Does every field of `t` fall inside its declared cardinality? The property
/// that makes [`HEAD_SIZES`] trustworthy as a versioned contract: a head that
/// is one value too narrow would silently alias two actions.
pub fn is_well_formed(t: HeadTargets) -> bool {
    (t.kind as u16) < HEAD_SIZES.kind
        && (t.card as u16) < HEAD_SIZES.card
        && (t.system as u16) < HEAD_SIZES.system
        && (t.slot as u16) < HEAD_SIZES.slot
        && (t.count as u16) < HEAD_SIZES.count
        && (t.mode as u16) < HEAD_SIZES.mode
        && (t.aux as u16) < HEAD_SIZES.aux
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout is a versioned contract shared with every trained net and
    /// every recorded trajectory. Changing a number here without bumping
    /// [`ENCODING_VERSION`] would remap actions under an already-trained
    /// policy, so pin both together.
    #[test]
    fn encoding_layout_is_pinned() {
        assert_eq!(ENCODING_VERSION, 1);
        assert_eq!(
            HEAD_SIZES,
            HeadSizes {
                kind: 34,
                card: 60,
                system: 25,
                slot: 8,
                count: 17,
                mode: 8,
                aux: 32,
            }
        );
        assert_eq!(HEAD_SIZES.total_outputs(), 184);
        assert_eq!(HEAD_SIZES.global_span(), 1_775_616_000);
    }

    #[test]
    fn global_index_round_trips() {
        let t = action_targets(Action::Battle {
            system: arcs_engine::SystemId(7),
            defender: arcs_engine::Player(2),
            assault: 3,
            skirmish: 1,
            raid: 0,
        });
        assert_eq!(decode_global_index(global_index(t)), t);
        let bare = action_targets(Action::EndTurn);
        assert_eq!(decode_global_index(global_index(bare)), bare);
    }
}
