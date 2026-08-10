//! The Arcs rules engine, ported from the TypeScript reference in
//! `src/engine/` (which remains the source of truth during the port).
//!
//! This crate has **zero runtime dependencies**; `serde` derives are behind
//! the optional `serde` feature. See the R-series port plan: R0 covers the
//! vocabulary, primitives and const data tables — the state machine arrives
//! in R1+.

pub mod action;
pub mod ambitions;
pub mod cards;
pub mod court;
pub mod dice;
pub mod game;
pub mod inline_vec;
pub mod map;
pub mod observe;
pub mod player_board;
pub mod powers;
pub mod rng;
pub mod setup;
pub mod state;
pub mod types;

pub use action::{
    Action, CardActionChoice, CardActionName, FollowMode, HitTarget, ParseActionError,
    PreludeChoice, VoxChoice, decode_action, encode_action,
};
pub use ambitions::{
    AmbitionResult, ambition_count, ambition_count_with, score_ambition, score_chapter,
};
pub use game::{
    Pending, RuleError, Standing, apply_action_mut, apply_battle_roll_mut, get_pending,
    legal_actions, new_game, resolve_chance_mut, standings, winner,
};
pub use inline_vec::InlineVec;
pub use observe::{
    DeterminizeOptions, HIDDEN_CARD, Observation, determinize, observe, sample_world,
};
pub use powers::{
    AbilitySource, ability_sources, cartel_holder, cartel_icons, discard_guild_card,
    gain_guild_card, provoke_outrage, steal_guild_card, survives_outrage, weapon_icons,
};
pub use rng::{ChanceSource, Rng, SplitMix64};
pub use setup::{SetupMode, VariantDef, make_variant};
pub use state::{GameState, MAX_SEATS, PlayerState, TurnState};
pub use types::{
    ActionCardId, ActionKind, AmbitionId, BuildingKind, CourtCardId, DieType, Phase, PlayMode,
    Player, ResourceType, Suit, SystemId,
};
