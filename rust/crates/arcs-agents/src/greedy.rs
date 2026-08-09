//! One-step lookahead over [`relative_evaluate`], ported from
//! `src/agents/greedy.ts`.
//!
//! The agent determinizes its observation once per decision, so the lookahead
//! happens in a complete legal world rather than one with blanked-out hands,
//! then plays the action with the best resulting position.
//!
//! Actions in Arcs open cascades — a Battle rolls dice and then asks where the
//! hits land; a Move off a starport asks whether to keep Catapulting. A naive
//! one-step eval scores `battle` *before* any damage exists and so never
//! fights. [`GreedyOpts::settle`] therefore plays the cascade out to the end
//! of the action, taking chance nodes from the RNG and resolving the agent's
//! own follow-up decisions with the same greedy rule. FINDINGS calls scoring
//! before the consequences exist a methodology trap, which is why settling is
//! on by default and `greedy-flat` exists only to price it.

use arcs_engine::game::{apply_action_mut, apply_battle_roll_mut, get_pending, resolve_chance_mut};
use arcs_engine::{
    Action, GameState, Observation, Pending, Phase, Player, Rng, VariantDef, determinize,
};

use crate::agent::{Agent, AgentCtx};
use crate::dicemath::{battle_distribution, top_mass};
use crate::eval::{DEFAULT_WEIGHTS, Weights, relative_evaluate};

/// How to value the battle roll.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BattleValuation {
    /// Average `samples` sampled rolls (the TS default).
    #[default]
    Sample,
    /// Take the expectation over [`battle_distribution`].
    ///
    /// Exact measured *weaker* for this 1-ply agent — see FINDINGS.md: with
    /// ~17 battle variants on offer, the argmax over noisy estimates plays
    /// the optimistic tail, and that optimism was subsidising battles the
    /// evaluation underprices. Kept as an option for search agents and
    /// experiments.
    Exact,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GreedyOpts {
    pub weights: Weights,
    /// Play cascades out before scoring. Turning this off makes a weaker bot.
    pub settle: bool,
    /// Cascade chance nodes are sampled this many times and the values
    /// averaged, so a battle is judged on (an estimate of) its expectation.
    pub samples: usize,
    pub battles: BattleValuation,
    /// Probability mass enumerated when `battles` is
    /// [`BattleValuation::Exact`].
    pub battle_mass: f64,
}

impl Default for GreedyOpts {
    fn default() -> Self {
        GreedyOpts {
            weights: DEFAULT_WEIGHTS,
            settle: true,
            samples: 3,
            battles: BattleValuation::Sample,
            battle_mass: 0.95,
        }
    }
}

/// Phases that belong to the action currently being resolved.
#[inline]
fn is_cascade(phase: Phase) -> bool {
    matches!(
        phase,
        Phase::BattleRoll | Phase::BattleAssign | Phase::Catapult
    )
}

pub struct Greedy {
    name: String,
    opts: GreedyOpts,
    /// Scratch buffer, so a decision allocates nothing per candidate.
    scratch: Vec<Action>,
}

impl Greedy {
    pub fn new(name: impl Into<String>, opts: GreedyOpts) -> Self {
        Greedy {
            name: name.into(),
            opts: GreedyOpts {
                samples: opts.samples.max(1),
                ..opts
            },
            scratch: Vec::new(),
        }
    }

    /// Resolve the rest of the current action: chance nodes from the RNG, and
    /// the agent's own cascade decisions by a 1-ply argmax on the same
    /// evaluation. (`settle` in greedy.ts.)
    fn settle(
        &mut self,
        mut cur: GameState,
        v: &VariantDef,
        player: Player,
        rng: &mut impl Rng,
    ) -> GameState {
        let w = &self.opts.weights;
        for _ in 0..64 {
            match get_pending(&cur, v) {
                Pending::Over => break,
                Pending::Chance => {
                    if !is_cascade(cur.phase) {
                        break; // a chapter deal is not our business
                    }
                    if resolve_chance_mut(&mut cur, v, rng).is_err() {
                        break;
                    }
                    continue;
                }
                Pending::Decision { player: actor } => {
                    if !is_cascade(cur.phase) || actor != player {
                        break;
                    }
                }
            }

            arcs_engine::legal_actions(&cur, v, &mut self.scratch);
            let mut best: Option<GameState> = None;
            let mut best_value = f64::NEG_INFINITY;
            for i in 0..self.scratch.len() {
                let mut next = cur;
                if apply_action_mut(&mut next, v, self.scratch[i]).is_err() {
                    continue;
                }
                let value = relative_evaluate(&next, v, player, w);
                if value > best_value {
                    best_value = value;
                    best = Some(next);
                }
            }
            match best {
                Some(next) => cur = next,
                None => break,
            }
        }
        cur
    }

    /// The value of a world reached by one candidate action.
    fn value_of(
        &mut self,
        after: GameState,
        v: &VariantDef,
        player: Player,
        rng: &mut impl Rng,
    ) -> f64 {
        let w = self.opts.weights;
        if !self.opts.settle {
            return relative_evaluate(&after, v, player, &w);
        }

        let chance = get_pending(&after, v) == Pending::Chance;
        let exact_battle = self.opts.battles == BattleValuation::Exact
            && chance
            && after.phase == Phase::BattleRoll
            && after.battle.is_some_and(|b| b.pending_reroll == 0);

        if exact_battle {
            // The dice are printed: value the battle as the exact expectation
            // over its outcome distribution, each outcome settled to the end
            // of the action, instead of averaging a few sampled rolls.
            let dice = after.battle.expect("battleRoll without a battle").dice;
            let outcomes = top_mass(
                battle_distribution(dice[0], dice[1], dice[2]),
                self.opts.battle_mass,
            );
            let mut total = 0.0f64;
            for o in outcomes {
                let mut branch = after;
                if apply_battle_roll_mut(&mut branch, v, o.totals).is_err() {
                    continue;
                }
                let settled = self.settle(branch, v, player, rng);
                total += o.p * relative_evaluate(&settled, v, player, &w);
            }
            return total;
        }

        if chance && is_cascade(after.phase) {
            // A cascade chance with no closed form here (Skirmishers
            // reroll): average over sampled resolutions.
            let mut total = 0.0f64;
            for _ in 0..self.opts.samples {
                let settled = self.settle(after, v, player, rng);
                total += relative_evaluate(&settled, v, player, &w);
            }
            return total / self.opts.samples as f64;
        }

        let settled = self.settle(after, v, player, rng);
        relative_evaluate(&settled, v, player, &w)
    }
}

impl Agent for Greedy {
    fn name(&self) -> &str {
        &self.name
    }

    fn choose(&mut self, obs: &Observation, legal: &[Action], ctx: &mut AgentCtx) -> usize {
        let v = ctx.variant;
        let world = determinize(obs, v, &mut ctx.rng, Default::default());
        let mut rng = ctx.rng;
        let mut best = 0usize;
        let mut best_value = f64::NEG_INFINITY;

        for (i, &a) in legal.iter().enumerate() {
            let mut after = world;
            if apply_action_mut(&mut after, v, a).is_err() {
                continue; // illegal in this sampled world (hidden-information mismatch)
            }
            let value = self.value_of(after, v, ctx.player, &mut rng);
            if value > best_value {
                best_value = value;
                best = i;
            }
        }
        ctx.rng = rng;
        best
    }
}
