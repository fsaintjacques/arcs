//! The Arcs rules engine, ported from the TypeScript reference in
//! `src/engine/` (which remains the source of truth during the port).
//!
//! This crate has **zero runtime dependencies**; `serde` derives are behind
//! the optional `serde` feature. See the R-series port plan: R0 covers the
//! vocabulary, primitives and const data tables — the state machine arrives
//! in R1+.

pub mod ambitions;
pub mod cards;
pub mod court;
pub mod dice;
pub mod inline_vec;
pub mod map;
pub mod player_board;
pub mod rng;
pub mod setup;
pub mod types;

pub use inline_vec::InlineVec;
pub use rng::{ChanceSource, Rng, SplitMix64};
pub use setup::{SetupMode, VariantDef, make_variant};
pub use types::{
    ActionCardId, ActionKind, AmbitionId, BuildingKind, CourtCardId, DieType, Phase, PlayMode,
    Player, ResourceType, Suit, SystemId,
};
