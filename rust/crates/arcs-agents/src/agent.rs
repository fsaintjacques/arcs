//! The decision-process API — the contract every FFI binding crosses.
//! Ported from `src/agents/types.ts`, with the plan §3 shape.
//!
//! An agent returns **an index into the legal slice**, not an `Action`. That
//! is the central design point of the port: "you cannot play an unoffered
//! action" becomes structural rather than a runtime check, and an index is
//! the one thing every host language (Python, JS, an NN policy head) can
//! return without reconstructing the action vocabulary.
//!
//! Agents are handed an [`Observation`], not the raw state, so they cannot
//! read Rival hands or deck order by accident — Arcs is an
//! imperfect-information game and honest bots are the point of the lab.
//! Search agents call [`arcs_engine::determinize`] to sample a complete world
//! to search in.

use arcs_engine::{Action, Observation, Player, SplitMix64, VariantDef};

/// Everything an agent needs besides the observation and the legal list.
///
/// The RNG lives here by value rather than behind a trait object so the
/// context stays `Sized` and allocation-free; a seat's stream is forked from
/// the game seed by the harness (R6).
pub struct AgentCtx<'v> {
    pub variant: &'v VariantDef,
    /// RNG for the agent's own randomness (determinizations, rollouts,
    /// tie-breaks).
    pub rng: SplitMix64,
    /// Seat this agent is playing.
    pub player: Player,
}

impl<'v> AgentCtx<'v> {
    pub fn new(variant: &'v VariantDef, player: Player, seed: u64) -> Self {
        AgentCtx {
            variant,
            rng: SplitMix64::new(seed),
            player,
        }
    }
}

/// An agent picks one of the legal actions at every decision node.
///
/// `choose` returns an index into `legal`; returning anything out of range is
/// a bug in the agent, and the harness treats it as one.
pub trait Agent {
    fn name(&self) -> &str;

    fn choose(&mut self, obs: &Observation, legal: &[Action], ctx: &mut AgentCtx) -> usize;
}
