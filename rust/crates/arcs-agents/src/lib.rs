//! Evaluation, dice math, candidate generation and the playing agents.
//!
//! Ported from `src/agents/`. The crate keeps the engine's zero-runtime-dep
//! rule: everything here is arithmetic over [`arcs_engine`] types.
//!
//! The one contract worth reading first is [`Agent`]: a bot is handed an
//! [`arcs_engine::Observation`] and the legal action slice, and answers with an
//! **index into that slice**. Every host language the port targets — a Rust
//! search, a Python policy, a browser script, an NN head — can return an
//! integer, so that signature is what the wasm and PyO3 bindings will cross
//! unchanged.

pub mod agent;
pub mod anchors;
pub mod candidates;
pub mod dicemath;
pub mod eval;
pub mod greedy;
pub mod random;
pub mod rollout;

pub use agent::{Agent, AgentCtx};
pub use anchors::{ANCHOR_LADDER, ANCHOR_MCTS2_V2_CONFIG, ANCHOR_WEIGHTS_V0, ANCHOR_WEIGHTS_V1};
pub use candidates::{CandidateOpts, generate_candidates};
pub use dicemath::{BattleOutcome, ExpectedTotals, battle_distribution, expected_battle, top_mass};
pub use eval::{
    DEFAULT_WEIGHTS, ProjectedAmbition, Weights, default_weights, evaluate,
    projected_ambition_power, relative_evaluate,
};
pub use greedy::{BattleValuation, Greedy, GreedyOpts};
pub use random::{Random, RandomPlus};
pub use rollout::{RolloutOpts, SeatValues, narrow, rollout, terminal_vector, value_vector};

/// Options a registry entry may override, the Rust form of the TS
/// `Record<string, unknown>` opts bag.
///
/// Typed rather than dynamic because the harness rebuilds agents from
/// `(name, opts)` specs on other threads (R6) and a typo in a key should not
/// silently produce a differently-configured bot — that class of mistake is
/// exactly what `docs/FINDINGS.md` records as costing real measurement time.
/// `None` means "keep the entry's own default".
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct AgentOpts {
    pub weights: Option<Weights>,
    pub settle: Option<bool>,
    pub samples: Option<usize>,
    pub battles: Option<BattleValuation>,
    pub battle_mass: Option<f64>,
}

impl AgentOpts {
    /// Apply the overrides that are set over a base configuration.
    fn over(&self, base: GreedyOpts) -> GreedyOpts {
        GreedyOpts {
            weights: self.weights.unwrap_or(base.weights),
            settle: self.settle.unwrap_or(base.settle),
            samples: self.samples.unwrap_or(base.samples),
            battles: self.battles.unwrap_or(base.battles),
            battle_mass: self.battle_mass.unwrap_or(base.battle_mass),
        }
    }
}

/// An agent name the registry does not know.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct UnknownAgent;

impl core::fmt::Display for UnknownAgent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown agent (available: {})", AGENT_NAMES.join(", "))
    }
}

impl core::error::Error for UnknownAgent {}

/// Every agent this crate can build, in registry order.
///
/// The search tier — `mc`, `mcts`, `mcts-fast`, `mcts-c`, `mcts2`,
/// `mcts2-play`, the tuned `greedy-t1`/`mcts-t1`, and the three search anchors
/// `anchor-mcts300-v0` / `anchor-mcts-c-v1` / `anchor-mcts2-v2` — lands in R5.
/// [`ANCHOR_LADDER`] already names the search anchors, because the ladder is
/// the gauntlet's contract and must not be quietly shortened; [`make_agent`]
/// reports them as unknown until the search agents exist.
pub const AGENT_NAMES: [&str; 4] = ["random", "random+", "greedy", "greedy-flat"];

/// Build an agent by registry name. (`makeAgent` in `src/agents/index.ts`.)
pub fn make_agent(name: &str, opts: &AgentOpts) -> Result<Box<dyn Agent>, UnknownAgent> {
    match name {
        "random" => Ok(Box::new(Random)),
        "random+" => Ok(Box::new(RandomPlus)),
        "greedy" => Ok(Box::new(Greedy::new(
            "greedy",
            opts.over(GreedyOpts::default()),
        ))),
        // Greedy without cascade settling — shows what settling is worth.
        "greedy-flat" => Ok(Box::new(Greedy::new(
            "greedy-flat",
            opts.over(GreedyOpts {
                settle: false,
                ..GreedyOpts::default()
            }),
        ))),
        // A frozen yardstick: the weights are a literal copy taken on freeze
        // day, so re-tuning the live defaults cannot re-baseline past
        // measurements. Never retune these.
        "anchor-greedy-v0" => Ok(Box::new(Greedy::new(
            "anchor-greedy-v0",
            opts.over(GreedyOpts {
                weights: ANCHOR_WEIGHTS_V0,
                ..GreedyOpts::default()
            }),
        ))),
        _ => Err(UnknownAgent),
    }
}

/// The registry's names, for CLIs and error messages. (`agentNames` in
/// `src/agents/index.ts`.) `anchor-greedy-v0` is buildable but excluded, as in
/// TS, so listing agents does not invite tuning a frozen anchor.
pub fn agent_names() -> Vec<&'static str> {
    AGENT_NAMES.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_every_listed_agent() {
        let opts = AgentOpts::default();
        for name in agent_names() {
            let agent = make_agent(name, &opts).expect("listed agent builds");
            assert_eq!(agent.name(), name, "an agent reports its registry name");
        }
    }

    #[test]
    fn builds_the_frozen_greedy_anchor() {
        let agent = make_agent("anchor-greedy-v0", &AgentOpts::default()).expect("anchor builds");
        assert_eq!(agent.name(), "anchor-greedy-v0");
    }

    /// The search anchors are named by the ladder but cannot be built yet;
    /// they must fail loudly rather than resolve to something else.
    #[test]
    fn search_agents_are_not_yet_registered() {
        for name in ["mcts", "mcts2", "anchor-mcts300-v0", "anchor-mcts-c-v1"] {
            assert_eq!(
                make_agent(name, &AgentOpts::default()).err(),
                Some(UnknownAgent)
            );
        }
    }

    #[test]
    fn opts_override_only_what_they_set() {
        let opts = AgentOpts {
            settle: Some(false),
            ..AgentOpts::default()
        };
        let merged = opts.over(GreedyOpts::default());
        assert!(!merged.settle);
        assert_eq!(merged.samples, GreedyOpts::default().samples);
        assert_eq!(merged.weights, GreedyOpts::default().weights);
    }
}
