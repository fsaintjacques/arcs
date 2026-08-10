//! Flat Monte-Carlo: sample a world, play every candidate action out several
//! times with a cheap policy, keep the action with the best mean value.
//! Ported from `src/agents/montecarlo.ts`.
//!
//! No tree, so it is blind to its own follow-up decisions, but it is the
//! cheapest agent that reasons about consequences several plies deep and it is
//! a useful control for whether MCTS's tree is actually earning its cost.

use arcs_engine::game::apply_action_mut;
use arcs_engine::{Action, Observation, VariantDef, determinize};

use crate::agent::{Agent, AgentCtx};
use crate::eval::{DEFAULT_WEIGHTS, Weights};
use crate::rollout::{RolloutOpts, narrow, rollout};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MonteCarloOpts {
    /// Rollouts per candidate action.
    pub rollouts: usize,
    /// Decisions played per rollout before the heuristic takes over.
    pub depth: usize,
    /// Cap on candidate actions considered (see [`narrow`]).
    pub max_actions: usize,
    pub weights: Weights,
}

impl Default for MonteCarloOpts {
    fn default() -> Self {
        MonteCarloOpts {
            rollouts: 12,
            depth: 40,
            max_actions: 16,
            weights: DEFAULT_WEIGHTS,
        }
    }
}

pub struct MonteCarlo {
    name: String,
    opts: MonteCarloOpts,
}

impl MonteCarlo {
    pub fn new(name: impl Into<String>, opts: MonteCarloOpts) -> Self {
        MonteCarlo {
            name: name.into(),
            opts,
        }
    }
}

impl Agent for MonteCarlo {
    fn name(&self) -> &str {
        &self.name
    }

    fn choose(&mut self, obs: &Observation, legal: &[Action], ctx: &mut AgentCtx) -> usize {
        let v: &VariantDef = ctx.variant;
        let candidates = narrow(legal, self.opts.max_actions);
        // TS returns the winning `Action`; the Rust contract is an index into
        // the legal list, and `narrow` is a pure filter so every candidate is
        // in it. Index 0 is `candidates[0]`, which is what TS starts from.
        let mut best = 0usize;
        let mut best_value = f64::NEG_INFINITY;

        for &a in &candidates {
            let mut total = 0.0f64;
            let mut played = 0usize;
            for _ in 0..self.opts.rollouts {
                let mut next = determinize(obs, v, &mut ctx.rng, Default::default());
                if apply_action_mut(&mut next, v, a).is_err() {
                    continue; // illegal in this sampled world
                }
                let value = rollout(
                    &mut next,
                    v,
                    &mut ctx.rng,
                    &RolloutOpts {
                        depth: self.opts.depth,
                        weights: self.opts.weights,
                    },
                );
                total += value[ctx.player.as_index()];
                played += 1;
            }
            if played == 0 {
                continue;
            }
            let value = total / played as f64;
            if value > best_value {
                best_value = value;
                best = legal.iter().position(|x| *x == a).unwrap_or(0);
            }
        }
        best
    }
}
