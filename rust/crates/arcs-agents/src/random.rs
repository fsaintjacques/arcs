//! The floor baselines, ported from `src/agents/random.ts`.

use arcs_engine::{Action, Observation, Rng};

use crate::agent::{Agent, AgentCtx};
use crate::rollout::useful_indices;

/// Uniform random over legal actions. The floor baseline.
pub struct Random;

impl Agent for Random {
    fn name(&self) -> &str {
        "random"
    }

    fn choose(&mut self, _obs: &Observation, legal: &[Action], ctx: &mut AgentCtx) -> usize {
        ctx.rng.gen_range(legal.len())
    }
}

/// Random, but never ends a turn while it still has actions to spend and
/// never throws cards away to seize. A meaningfully stronger floor than
/// [`Random`], and the default rollout policy for search agents.
pub struct RandomPlus;

impl Agent for RandomPlus {
    fn name(&self) -> &str {
        "random+"
    }

    fn choose(&mut self, _obs: &Observation, legal: &[Action], ctx: &mut AgentCtx) -> usize {
        let useful = useful_indices(legal);
        if useful.is_empty() {
            ctx.rng.gen_range(legal.len())
        } else {
            useful[ctx.rng.gen_range(useful.len())]
        }
    }
}
