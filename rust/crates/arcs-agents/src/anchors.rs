//! Frozen yardsticks for the strength gauntlet (docs/GAUNTLET.md), ported
//! verbatim from `src/agents/anchors.ts`.
//!
//! An anchor never moves once frozen: its weights are a literal copy taken on
//! the day it was frozen, not a reference to [`crate::DEFAULT_WEIGHTS`], so
//! tuning the live defaults cannot silently re-baseline every past
//! measurement. The list is append-only — a bot that beats the ladder becomes
//! a *new* anchor generation (`-v1`, `-v2`, …); an old anchor is never
//! edited.
//!
//! These constants are a **versioned public contract shared with the TS
//! engine**: `tests/anchor_weights.rs` asserts them field for field against a
//! fixture printed from `src/agents/anchors.ts`. Editing a number here
//! invalidates every gauntlet row ever recorded against it.

use crate::eval::Weights;

/// `defaultWeights` as they stood when the gauntlet was established,
/// re-expressed whenever the [`Weights`] struct grows: terms added later
/// carry weight 0 and the flat `resource: 0.7` became a uniform per-type map,
/// so the evaluation computes the identical number it did on freeze day.
pub const ANCHOR_WEIGHTS_V0: Weights = Weights {
    power: 1.0,
    declared_lead: 0.9,
    declared_contest: 0.35,
    latent_ambition: 0.45,
    fresh_ship: 0.5,
    damaged_ship: 0.2,
    starport: 1.4,
    city: 2.2,
    control: 0.5,
    resource_slot: 0.4,
    resource_value: [0.7, 0.7, 0.7, 0.7, 0.7],
    court_agent: 0.35,
    court_lead: 1.1,
    guild_card: 1.0,
    initiative: 1.2,
    hand_card: 0.25,
    hand_pips: 0.0,
    hand_actionable: 0.0,
    hand_high_card: 0.0,
    declarable_lead: 0.0,
    outrage: 1.0,
};

/// The M2 evaluation weights as frozen with `anchor-mcts-c-v1` (2026-08-09):
/// the hand-quality/scarcity terms at their hand-set values, before any tuned
/// weights are promoted.
pub const ANCHOR_WEIGHTS_V1: Weights = Weights {
    power: 1.0,
    declared_lead: 0.9,
    declared_contest: 0.35,
    latent_ambition: 0.35,
    fresh_ship: 0.5,
    damaged_ship: 0.2,
    starport: 1.4,
    city: 2.2,
    control: 0.5,
    resource_slot: 0.4,
    resource_value: [0.65, 0.65, 0.5, 0.85, 0.85],
    court_agent: 0.35,
    court_lead: 1.1,
    guild_card: 1.0,
    initiative: 1.2,
    hand_card: 0.1,
    hand_pips: 0.15,
    hand_actionable: 0.1,
    hand_high_card: 0.3,
    declarable_lead: 0.5,
    outrage: 1.0,
};

/// The full configuration of `anchor-mcts2-v2` (frozen 2026-08-09), pinned
/// value by value so later changes to the mcts2 defaults cannot move the
/// yardstick. (`anchorMcts2V2Config` in anchors.ts.) The search agent it
/// configures lands in R5; the constant is frozen here so the two cannot drift
/// apart in the meantime.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Mcts2Config {
    pub iterations: usize,
    pub c_puct: f64,
    pub max_actions: usize,
    pub worlds: usize,
    pub prior_temp: f64,
    pub battle_mass: f64,
    pub max_depth: usize,
    pub priors: bool,
    pub rollout_leaf: bool,
}

pub const ANCHOR_MCTS2_V2_CONFIG: Mcts2Config = Mcts2Config {
    iterations: 440,
    c_puct: 1.5,
    max_actions: 16,
    worlds: 16,
    prior_temp: 1.0,
    battle_mass: 0.9,
    max_depth: 64,
    priors: true,
    rollout_leaf: true,
};

/// The gauntlet ladder, oldest to newest. A candidate must beat the newest
/// anchor separated and regress against none of the older ones.
///
/// The three search anchors are named here because the ladder *is* the
/// contract — shortening it would silently weaken every future promotion
/// claim. [`crate::make_agent`] can only build `anchor-greedy-v0` until R5
/// lands the search agents.
pub const ANCHOR_LADDER: [&str; 4] = [
    "anchor-greedy-v0",
    "anchor-mcts300-v0",
    "anchor-mcts-c-v1",
    "anchor-mcts2-v2",
];
