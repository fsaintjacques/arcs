//! Evaluation, dice math, candidate generation, the playing agents, the
//! global action encoding and the NN scaffolding.
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
//!
//! Two modules exist for the training path rather than for play:
//! [`encoding`] factorizes an action into policy heads, and [`nn`] turns a
//! position into features and features into a value.

pub mod agent;
pub mod anchors;
pub mod candidates;
pub mod dicemath;
pub mod encoding;
pub mod eval;
pub mod greedy;
pub mod mcts;
pub mod mcts2;
pub mod montecarlo;
pub mod nn;
pub mod random;
pub mod rollout;
mod tree;

pub use agent::{Agent, AgentCtx};
pub use anchors::{
    ANCHOR_LADDER, ANCHOR_MCTS2_V2_CONFIG, ANCHOR_WEIGHTS_V0, ANCHOR_WEIGHTS_V1, Mcts2Config,
};
pub use candidates::{CandidateOpts, generate_candidates};
pub use dicemath::{BattleOutcome, ExpectedTotals, battle_distribution, expected_battle, top_mass};
pub use encoding::{
    ENCODING_VERSION, HEAD_SIZES, HeadSizes, HeadTargets, action_targets, global_index,
};
pub use eval::{
    DEFAULT_WEIGHTS, ProjectedAmbition, Weights, default_weights, evaluate,
    projected_ambition_power, relative_evaluate,
};
pub use greedy::{BattleValuation, Greedy, GreedyOpts};
pub use mcts::{Mcts, MctsOpts};
pub use mcts2::{Clock, Mcts2, Mcts2Opts};
pub use montecarlo::{MonteCarlo, MonteCarloOpts};
pub use nn::{FEATURE_SIZE, Mlp, MlpShape, extract_features};
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
    // --- evaluation tier ---
    pub weights: Option<Weights>,
    pub settle: Option<bool>,
    pub samples: Option<usize>,
    pub battles: Option<BattleValuation>,
    /// Probability mass enumerated exactly at a battle — read by `greedy`'s
    /// exact valuation and by mcts2's frontier battles.
    pub battle_mass: Option<f64>,
    // --- search tier ---
    /// Iterations per decision (`mcts`, `mcts2`).
    pub iterations: Option<usize>,
    /// Wall-clock budget per decision in ms (`mcts2`). There is deliberately
    /// no way to *clear* an entry's budget: `mcts2-play` is defined by having
    /// one, and an opts bag that could silently remove it would turn an
    /// interactive agent into an unbounded one.
    pub time_ms: Option<u64>,
    /// Where a wall-clock budget reads the time. Set by hosts without
    /// `std::time` — the browser binding passes `Date.now()`; see
    /// [`mcts2::Clock`].
    pub clock: Option<mcts2::Clock>,
    /// PUCT exploration constant (`mcts2`).
    pub c_puct: Option<f64>,
    /// UCT exploration constant (`mcts`).
    pub c: Option<f64>,
    /// Cap on candidates per node.
    pub max_actions: Option<usize>,
    /// Tree-depth cap.
    pub max_depth: Option<usize>,
    /// Decisions played per rollout (`mcts`, `mc`).
    pub rollout_depth: Option<usize>,
    /// Determinizations pooled per decision (`mcts2`); 0 = one per iteration.
    pub worlds: Option<usize>,
    /// Softmax temperature over prior evaluations (`mcts2`).
    pub prior_temp: Option<f64>,
    /// Ablation: eval priors on or off (`mcts2`).
    pub priors: Option<bool>,
    /// Ablation: rollout-valued vs truncated eval-valued leaves (`mcts2`).
    pub rollout_leaf: Option<bool>,
    /// Informed candidate trimming instead of blind `narrow` (`mcts`).
    pub candidates: Option<bool>,
    /// Rollouts per candidate action (`mc`).
    pub rollouts: Option<usize>,
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

    fn over_mcts(&self, base: MctsOpts) -> MctsOpts {
        MctsOpts {
            iterations: self.iterations.unwrap_or(base.iterations),
            c: self.c.unwrap_or(base.c),
            rollout_depth: self.rollout_depth.unwrap_or(base.rollout_depth),
            max_actions: self.max_actions.unwrap_or(base.max_actions),
            max_depth: self.max_depth.unwrap_or(base.max_depth),
            candidates: self.candidates.unwrap_or(base.candidates),
            weights: self.weights.unwrap_or(base.weights),
        }
    }

    fn over_mcts2(&self, base: Mcts2Opts) -> Mcts2Opts {
        Mcts2Opts {
            iterations: self.iterations.unwrap_or(base.iterations),
            time_ms: self.time_ms.or(base.time_ms),
            clock: self.clock.or(base.clock),
            c_puct: self.c_puct.unwrap_or(base.c_puct),
            max_actions: self.max_actions.unwrap_or(base.max_actions),
            worlds: self.worlds.unwrap_or(base.worlds),
            prior_temp: self.prior_temp.unwrap_or(base.prior_temp),
            battle_mass: self.battle_mass.unwrap_or(base.battle_mass),
            max_depth: self.max_depth.unwrap_or(base.max_depth),
            priors: self.priors.unwrap_or(base.priors),
            rollout_leaf: self.rollout_leaf.unwrap_or(base.rollout_leaf),
            weights: self.weights.unwrap_or(base.weights),
        }
    }

    fn over_mc(&self, base: MonteCarloOpts) -> MonteCarloOpts {
        MonteCarloOpts {
            rollouts: self.rollouts.unwrap_or(base.rollouts),
            depth: self.rollout_depth.unwrap_or(base.depth),
            max_actions: self.max_actions.unwrap_or(base.max_actions),
            weights: self.weights.unwrap_or(base.weights),
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
/// The TS registry also carries `greedy-t1`/`mcts-t1`, built from the CEM
/// run-1 weights in `src/agents/weights/tuned1.ts`. Those weights were never
/// promoted, and R4 did not port them, so the two entries are absent here
/// rather than pointing at the untuned defaults and quietly measuring
/// something else.
pub const AGENT_NAMES: [&str; 10] = [
    "random",
    "random+",
    "greedy",
    "greedy-flat",
    "mc",
    "mcts",
    "mcts-fast",
    "mcts-c",
    "mcts2",
    "mcts2-play",
];

/// Build an agent by registry name. (`makeAgent` in `src/agents/index.ts`.)
///
/// The frozen anchors are buildable but not listed — see [`agent_names`].
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
        // Flat Monte-Carlo over sampled worlds.
        "mc" => Ok(Box::new(MonteCarlo::new(
            "mc",
            opts.over_mc(MonteCarloOpts::default()),
        ))),
        // Determinized ISMCTS with max^n backup.
        "mcts" => Ok(Box::new(Mcts::new(
            "mcts",
            opts.over_mcts(MctsOpts::default()),
        ))),
        // A deliberately cheap MCTS, for fast batches.
        "mcts-fast" => Ok(Box::new(Mcts::new(
            "mcts-fast",
            opts.over_mcts(MctsOpts {
                iterations: 120,
                rollout_depth: 20,
                ..MctsOpts::default()
            }),
        ))),
        // MCTS with informed candidate trimming instead of blind narrow().
        "mcts-c" => Ok(Box::new(Mcts::new(
            "mcts-c",
            opts.over_mcts(MctsOpts {
                candidates: true,
                ..MctsOpts::default()
            }),
        ))),
        // PUCT with eval priors over pooled determinized worlds.
        "mcts2" => Ok(Box::new(Mcts2::new(
            "mcts2",
            opts.over_mcts2(Mcts2Opts {
                iterations: 440,
                ..Mcts2Opts::default()
            }),
        ))),
        // The same brain with an interactive wall-clock budget.
        "mcts2-play" => Ok(Box::new(Mcts2::new(
            "mcts2-play",
            opts.over_mcts2(Mcts2Opts {
                iterations: 100_000,
                time_ms: Some(600),
                ..Mcts2Opts::default()
            }),
        ))),
        // --- frozen gauntlet anchors; see anchors.rs. Never retune these. ---
        //
        // Each one pins *every* parameter that moves it, not just the
        // weights: an anchor whose iteration count followed the live default
        // would re-baseline every past measurement the day that default
        // changed.
        "anchor-greedy-v0" => Ok(Box::new(Greedy::new(
            "anchor-greedy-v0",
            opts.over(GreedyOpts {
                weights: ANCHOR_WEIGHTS_V0,
                ..GreedyOpts::default()
            }),
        ))),
        "anchor-mcts300-v0" => Ok(Box::new(Mcts::new(
            "anchor-mcts300-v0",
            opts.over_mcts(MctsOpts {
                iterations: 300,
                weights: ANCHOR_WEIGHTS_V0,
                ..MctsOpts::default()
            }),
        ))),
        "anchor-mcts-c-v1" => Ok(Box::new(Mcts::new(
            "anchor-mcts-c-v1",
            opts.over_mcts(MctsOpts {
                iterations: 300,
                candidates: true,
                weights: ANCHOR_WEIGHTS_V1,
                ..MctsOpts::default()
            }),
        ))),
        "anchor-mcts2-v2" => Ok(Box::new(Mcts2::new(
            "anchor-mcts2-v2",
            opts.over_mcts2(Mcts2Opts::from_config(
                ANCHOR_MCTS2_V2_CONFIG,
                ANCHOR_WEIGHTS_V1,
            )),
        ))),
        _ => Err(UnknownAgent),
    }
}

/// The registry's names, for CLIs and error messages. (`agentNames` in
/// `src/agents/index.ts`.) The frozen anchors are buildable but excluded, so
/// listing agents does not invite tuning a yardstick.
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

    /// The gauntlet ladder *is* the contract: every anchor it names must be
    /// buildable, under its own name, without being listed for tuning.
    #[test]
    fn builds_every_anchor_on_the_ladder() {
        for name in ANCHOR_LADDER {
            let agent = make_agent(name, &AgentOpts::default()).expect("anchor builds");
            assert_eq!(agent.name(), name);
            assert!(
                !AGENT_NAMES.contains(&name),
                "{name} is a frozen anchor and must not be listed"
            );
        }
    }

    #[test]
    fn unknown_names_still_fail_loudly() {
        assert_eq!(
            make_agent("mcts3", &AgentOpts::default()).err(),
            Some(UnknownAgent)
        );
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

    /// A registry entry's own defaults survive an opts bag that does not
    /// mention them — the property that makes `mcts-fast` cheap and
    /// `mcts2-play` time-bounded even when a caller overrides something else.
    #[test]
    fn registry_defaults_survive_partial_overrides() {
        let opts = AgentOpts {
            c_puct: Some(2.0),
            ..AgentOpts::default()
        };
        let merged = opts.over_mcts2(Mcts2Opts {
            iterations: 100_000,
            time_ms: Some(600),
            ..Mcts2Opts::default()
        });
        assert_eq!(merged.c_puct, 2.0);
        assert_eq!(merged.time_ms, Some(600));
        assert_eq!(merged.iterations, 100_000);

        let fast = AgentOpts::default().over_mcts(MctsOpts {
            iterations: 120,
            rollout_depth: 20,
            ..MctsOpts::default()
        });
        assert_eq!(fast.iterations, 120);
        assert_eq!(fast.rollout_depth, 20);
    }

    /// The anchor configuration is expanded field for field; nothing may fall
    /// back to a live default that could later move.
    #[test]
    fn the_mcts2_anchor_pins_every_parameter() {
        let opts = Mcts2Opts::from_config(ANCHOR_MCTS2_V2_CONFIG, ANCHOR_WEIGHTS_V1);
        assert_eq!(opts.iterations, ANCHOR_MCTS2_V2_CONFIG.iterations);
        assert_eq!(opts.c_puct, ANCHOR_MCTS2_V2_CONFIG.c_puct);
        assert_eq!(opts.max_actions, ANCHOR_MCTS2_V2_CONFIG.max_actions);
        assert_eq!(opts.worlds, ANCHOR_MCTS2_V2_CONFIG.worlds);
        assert_eq!(opts.prior_temp, ANCHOR_MCTS2_V2_CONFIG.prior_temp);
        assert_eq!(opts.battle_mass, ANCHOR_MCTS2_V2_CONFIG.battle_mass);
        assert_eq!(opts.max_depth, ANCHOR_MCTS2_V2_CONFIG.max_depth);
        assert_eq!(opts.priors, ANCHOR_MCTS2_V2_CONFIG.priors);
        assert_eq!(opts.rollout_leaf, ANCHOR_MCTS2_V2_CONFIG.rollout_leaf);
        assert_eq!(opts.weights, ANCHOR_WEIGHTS_V1);
    }
}
