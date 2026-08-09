//! Core vocabulary for the Arcs engine, mirroring `src/engine/types.ts`.
//!
//! Every enum is `#[repr(u8)]` with a stable `ALL` order matching the TS
//! string-union order, so `as_index()` agrees with the TS arrays
//! (`RESOURCE_TYPES`, `SUITS`, `AMBITIONS`, ...) and `[T; N]` maps replace
//! `Record<K, T>` objects.

/// Defines a `#[repr(u8)]` enum with `ALL`, `COUNT`, `as_index` and
/// `from_index`, keeping variant order the single source of truth.
macro_rules! index_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($(#[$vmeta:meta])* $variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[repr(u8)]
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        $vis enum $name { $($(#[$vmeta])* $variant),+ }

        impl $name {
            pub const COUNT: usize = [$($name::$variant),+].len();
            pub const ALL: [$name; Self::COUNT] = [$($name::$variant),+];

            #[inline]
            pub const fn as_index(self) -> usize {
                self as usize
            }

            #[inline]
            pub const fn from_index(i: usize) -> Option<Self> {
                if i < Self::COUNT { Some(Self::ALL[i]) } else { None }
            }
        }
    };
}

/// Defines a `u8` newtype id with an index round-trip.
macro_rules! id_newtype {
    ($(#[$meta:meta])* $vis:vis struct $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, Default)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        $vis struct $name(pub u8);

        impl $name {
            #[inline]
            pub const fn as_index(self) -> usize {
                self.0 as usize
            }
        }

        impl From<$name> for usize {
            #[inline]
            fn from(v: $name) -> usize {
                v.0 as usize
            }
        }
    };
}

pub(crate) use {id_newtype, index_enum};

id_newtype! {
    /// A seat at the table, `0..players`.
    pub struct Player
}

id_newtype! {
    /// A system id, `cluster * 4 + slot`, slot 0 the gate (see `map`).
    pub struct SystemId
}

id_newtype! {
    /// An action card id, `suit_index * 7 + (number - 1)` (see `cards`).
    pub struct ActionCardId
}

id_newtype! {
    /// A Court card id, printed number minus 1: Guild `0..25`, Vox `25..31`.
    pub struct CourtCardId
}

index_enum! {
    /// The 5 resource types, also the 5 planet types and Guild suits.
    pub enum ResourceType {
        Material,
        Fuel,
        Weapon,
        Relic,
        Psionic,
    }
}

index_enum! {
    /// The 4 action-card suits.
    pub enum Suit {
        Administration,
        Aggression,
        Construction,
        Mobilization,
    }
}

index_enum! {
    /// The 7 standard actions an action pip can buy.
    pub enum ActionKind {
        Tax,
        Build,
        Move,
        Repair,
        Influence,
        Secure,
        Battle,
    }
}

index_enum! {
    /// The 5 ambitions.
    pub enum AmbitionId {
        Tycoon,
        Tyrant,
        Warlord,
        Keeper,
        Empath,
    }
}

index_enum! {
    /// The 3 battle die types.
    pub enum DieType {
        Assault,
        Skirmish,
        Raid,
    }
}

index_enum! {
    /// The 2 building kinds.
    pub enum BuildingKind {
        City,
        Starport,
    }
}

index_enum! {
    /// How a card entered the trick. `Lead` is the initiative holder.
    pub enum PlayMode {
        Lead,
        Surpass,
        Copy,
        Pivot,
    }
}

index_enum! {
    /// Game phases, mirroring the TS `Phase` union plus a reserved
    /// `LeaderDraft` variant for the Leaders & Lore expansion (unimplemented,
    /// slotted before `Deal` per the port plan).
    pub enum Phase {
        /// Reserved for Leaders & Lore; never entered by the base game.
        LeaderDraft,
        /// Chance: shuffle and deal a chapter's hands.
        Deal,
        /// Decision: 2-player mulligan.
        Mulligan,
        /// Decision: play a card (lead / surpass / copy / pivot) or pass.
        Play,
        /// Decision: declare, seize, spend resources, then begin actions.
        Prelude,
        /// Decision: spend pips and free actions, or end the turn.
        Actions,
        /// Decision: continue or stop a catapult move.
        Catapult,
        /// Chance: roll the collected battle dice (or a reroll).
        BattleRoll,
        /// Decision: Skirmishers — how many blanks to reroll.
        BattleReroll,
        /// Decision: assign hits one at a time, then raid.
        BattleAssign,
        /// Decision: Farseers — whose hand to look at.
        PeekTarget,
        /// Decision: Farseers — swap a card with the peeked hand.
        PeekSwap,
        /// Decision: a wiped-out player places 3 ships in a gate.
        Reinforce,
        Over,
    }
}

// Defaults for the enums stored in `InlineVec`s (its `new`/`collect` fill
// dead slots with `T::default()`; the value itself is never read). Manual
// rather than derived so `index_enum!` needs no `#[default]` plumbing.
#[allow(clippy::derivable_impls)]
impl Default for ResourceType {
    fn default() -> Self {
        ResourceType::Material
    }
}

#[allow(clippy::derivable_impls)]
impl Default for AmbitionId {
    fn default() -> Self {
        AmbitionId::Tycoon
    }
}

/// A `Record<ResourceType, T>` becomes a dense array indexed by
/// [`ResourceType::as_index`].
pub type ByResource<T> = [T; ResourceType::COUNT];

/// A `Record<DieType, T>` becomes a dense array indexed by
/// [`DieType::as_index`].
pub type ByDieType<T> = [T; DieType::COUNT];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_index_round_trips() {
        for (i, r) in ResourceType::ALL.iter().enumerate() {
            assert_eq!(r.as_index(), i);
            assert_eq!(ResourceType::from_index(i), Some(*r));
        }
        assert_eq!(ResourceType::from_index(ResourceType::COUNT), None);
        for (i, p) in Phase::ALL.iter().enumerate() {
            assert_eq!(p.as_index(), i);
            assert_eq!(Phase::from_index(i), Some(*p));
        }
    }

    #[test]
    fn counts_match_ts() {
        assert_eq!(ResourceType::COUNT, 5);
        assert_eq!(Suit::COUNT, 4);
        assert_eq!(ActionKind::COUNT, 7);
        assert_eq!(AmbitionId::COUNT, 5);
        assert_eq!(DieType::COUNT, 3);
        assert_eq!(BuildingKind::COUNT, 2);
        assert_eq!(PlayMode::COUNT, 4);
        // 13 TS phases + the reserved LeaderDraft.
        assert_eq!(Phase::COUNT, 14);
    }
}
