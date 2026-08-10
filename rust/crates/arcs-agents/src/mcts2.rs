//! Truncated determinized ISMCTS with PUCT — the search built for the
//! evaluation this repo spent three milestones improving. Ported from
//! `src/agents/mcts2.ts`.
//!
//! The design premise — three FINDINGS sections said eval improvements drown
//! in rollouts, so value leaves with the eval directly — was measured and
//! *refuted* by this agent's own ablations: at equal time, rollout leaves beat
//! eval leaves by +18.8 ± 11.5. What actually carries the agent is using the
//! evaluation as a *policy guide* rather than a value function. Structure, and
//! why each piece is here:
//!
//!   - **PUCT with eval priors.** At a node's expansion, every candidate is
//!     applied once on a copy and scored with [`relative_evaluate`]; a softmax
//!     over those scores becomes the prior. Selection is
//!     Q + c_puct · P · √ΣN / (1 + N), with max^n Q per seat as before —
//!     opponents maximise themselves, not a coalition against the root.
//!     Ablating this costs −37.5 ± 12.1: it is the engine of the gain.
//!   - **Rollout-valued leaves** (default). One node expands per iteration and
//!     a `random+` rollout values it — noisy, but an estimate of the actual
//!     game outcome, which the ablations say beats the eval's opinion of the
//!     position. `rollout_leaf: false` restores truncated eval-valued leaves
//!     for experiments.
//!   - **World pooling.** [`Mcts2Opts::worlds`] determinizations are sampled
//!     once per decision and cycled across iterations, so each world is
//!     searched several times instead of paying `determinize` per iteration.
//!   - **Exact battle chance at the frontier.** When an iteration ends on an
//!     unrolled battle, the leaf value is the probability-weighted eval over
//!     [`battle_distribution`]'s top mass instead of one sampled roll. Chance
//!     met mid-descent stays open-loop (sampled), as in [`crate::Mcts`].
//!   - Candidates come from [`generate_candidates`]; node keys are the
//!     [`Action`] enum itself (see the `tree` module), not TS's `encodeAction`
//!     strings.
//!
//! The ablation switches (`priors`, `rollout_leaf`, `worlds: 0`) exist so the
//! gauntlet can price each idea separately.

use std::time::Instant;

use arcs_engine::game::{apply_action_mut, apply_battle_roll_mut, get_pending, resolve_chance_mut};
use arcs_engine::{
    Action, GameState, MAX_SEATS, Observation, Pending, Phase, Player, SplitMix64, VariantDef,
    determinize,
};

use crate::agent::{Agent, AgentCtx};
use crate::anchors::Mcts2Config;
use crate::candidates::{CandidateOpts, generate_candidates};
use crate::dicemath::{battle_distribution, top_mass};
use crate::eval::{DEFAULT_WEIGHTS, Weights, relative_evaluate};
use crate::rollout::{RolloutOpts, SeatValues, rollout, terminal_vector, value_vector};
use crate::tree::{Arena, index_of};

/// Rollout depth at a truncated leaf. Hard-coded in `mcts2.ts` rather than
/// exposed as an option, and kept that way so the two agents stay comparable.
const LEAF_ROLLOUT_DEPTH: usize = 30;

/// How often the wall-clock budget is checked, in iterations. Reading the
/// clock is not free relative to a 400-iteration budget, and the deadline only
/// needs to be honoured to within one batch.
const DEADLINE_EVERY: usize = 8;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Mcts2Opts {
    /// Iteration cap per decision.
    pub iterations: usize,
    /// Wall-clock budget in ms; whichever of the two limits hits first.
    pub time_ms: Option<u64>,
    /// PUCT exploration constant.
    pub c_puct: f64,
    /// Cap on candidates per node (see [`generate_candidates`]).
    pub max_actions: usize,
    /// Determinizations pooled per decision; 0 = fresh world per iteration.
    pub worlds: usize,
    /// Softmax temperature over prior evals, in evaluation units.
    pub prior_temp: f64,
    /// Probability mass enumerated exactly at frontier battles.
    pub battle_mass: f64,
    /// Descent-depth safety cap.
    pub max_depth: usize,
    /// Ablation: uniform priors instead of eval priors.
    pub priors: bool,
    /// Rollout-valued leaves (default) vs truncated eval-valued leaves.
    pub rollout_leaf: bool,
    pub weights: Weights,
}

impl Default for Mcts2Opts {
    fn default() -> Self {
        Mcts2Opts {
            iterations: 400,
            time_ms: None,
            c_puct: 1.5,
            max_actions: 16,
            worlds: 16,
            prior_temp: 1.0,
            battle_mass: 0.9,
            max_depth: 64,
            priors: true,
            rollout_leaf: true,
            weights: DEFAULT_WEIGHTS,
        }
    }
}

impl Mcts2Opts {
    /// Expand a frozen anchor configuration into a full option set. The
    /// anchor pins every search parameter, so only the weights are left to
    /// supply. (`{ ...anchorMcts2V2Config, weights }` in `index.ts`.)
    pub fn from_config(config: Mcts2Config, weights: Weights) -> Self {
        Mcts2Opts {
            iterations: config.iterations,
            time_ms: None,
            c_puct: config.c_puct,
            max_actions: config.max_actions,
            worlds: config.worlds,
            prior_temp: config.prior_temp,
            battle_mass: config.battle_mass,
            max_depth: config.max_depth,
            priors: config.priors,
            rollout_leaf: config.rollout_leaf,
            weights,
        }
    }
}

pub struct Mcts2 {
    name: String,
    opts: Mcts2Opts,
    tree: Arena,
    /// Determinizations pooled for the current decision.
    pool: Vec<GameState>,
}

impl Mcts2 {
    pub fn new(name: impl Into<String>, opts: Mcts2Opts) -> Self {
        Mcts2 {
            name: name.into(),
            opts,
            tree: Arena::new(),
            pool: Vec::new(),
        }
    }

    /// Value the frontier position without descending further.
    fn frontier_value(
        &self,
        s: &mut GameState,
        v: &VariantDef,
        rng: &mut SplitMix64,
    ) -> SeatValues {
        if self.opts.rollout_leaf {
            return rollout(
                s,
                v,
                rng,
                &RolloutOpts {
                    depth: LEAF_ROLLOUT_DEPTH,
                    weights: self.opts.weights,
                },
            );
        }
        match get_pending(s, v) {
            Pending::Over => terminal_vector(s),
            // An unrolled battle: exact expectation over the printed dice.
            Pending::Chance
                if s.phase == Phase::BattleRoll
                    && s.battle.is_some_and(|b| b.pending_reroll == 0) =>
            {
                let dice = s.battle.expect("battleRoll without a battle").dice;
                let outcomes = top_mass(
                    battle_distribution(dice[0], dice[1], dice[2]),
                    self.opts.battle_mass,
                );
                let mut acc = [0.0f64; MAX_SEATS];
                for o in outcomes {
                    let mut branch = *s;
                    if apply_battle_roll_mut(&mut branch, v, o.totals).is_err() {
                        continue;
                    }
                    let val = value_vector(&branch, v, &self.opts.weights);
                    for (slot, x) in acc.iter_mut().zip(val.iter()) {
                        *slot += o.p * x;
                    }
                }
                acc
            }
            _ => value_vector(s, v, &self.opts.weights),
        }
    }

    /// Softmax over one-step evaluations, fixed at expansion time.
    fn compute_priors(
        &self,
        s: &GameState,
        v: &VariantDef,
        player: Player,
        candidates: &[Action],
    ) -> Vec<(Action, f64)> {
        let uniform = 1.0 / candidates.len().max(1) as f64;
        if !self.opts.priors || candidates.is_empty() {
            return candidates.iter().map(|&a| (a, uniform)).collect();
        }

        let mut scores: Vec<(Action, f64)> = Vec::with_capacity(candidates.len());
        let mut best = f64::NEG_INFINITY;
        for &a in candidates {
            // Illegal in this world: keep -inf so its prior floors out. (TS
            // catches the `applyActionMut` throw for the same reason; the
            // Rust engine hands back a `Result` instead.)
            let mut after = *s;
            let score = match apply_action_mut(&mut after, v, a) {
                Ok(()) => relative_evaluate(&after, v, player, &self.opts.weights),
                Err(_) => f64::NEG_INFINITY,
            };
            scores.push((a, score));
            if score > best {
                best = score;
            }
        }
        // deviation: when *every* candidate is illegal in this world, TS
        // computes `-Infinity - -Infinity = NaN` and hands the search NaN
        // priors, which silently disable PUCT selection at that node. Rust
        // falls back to the uniform prior the ablation switch already uses.
        if !best.is_finite() {
            return candidates.iter().map(|&a| (a, uniform)).collect();
        }

        let mut total = 0.0f64;
        for (_, score) in scores.iter_mut() {
            let p = (((*score - best) / self.opts.prior_temp).min(0.0)).exp();
            *score = p;
            total += p;
        }
        for (_, p) in scores.iter_mut() {
            *p /= total;
        }
        scores
    }

    /// PUCT selection over the candidates legal in *this* world, creating the
    /// chosen child if it does not exist yet.
    fn puct_child(&mut self, node: usize, candidates: &[Action]) -> Option<usize> {
        let seat = self.tree.nodes[node].player.as_index();
        let parent = &self.tree.nodes[node];
        let mut sum_visits = 0u32;
        for &a in candidates {
            if let Some(child) = parent.child(a) {
                sum_visits += self.tree.nodes[child].visits;
            }
        }
        let sqrt_sum = f64::from(sum_visits + 1).sqrt();
        let parent_avg = if parent.visits > 0 {
            parent.totals[seat] / f64::from(parent.visits)
        } else {
            0.0
        };
        let fallback_prior = 1.0 / candidates.len().max(1) as f64;

        let mut best: Option<usize> = None;
        let mut best_action: Option<Action> = None;
        let mut best_score = f64::NEG_INFINITY;
        for &a in candidates {
            let child = parent.child(a);
            let visits = child.map_or(0, |i| self.tree.nodes[i].visits);
            // Unvisited children start from the parent's running value rather
            // than infinity, so the prior actually orders first exploration.
            let q = match child {
                Some(i) if visits > 0 => self.tree.nodes[i].totals[seat] / f64::from(visits),
                _ => parent_avg,
            };
            let p = parent.prior(a).unwrap_or(fallback_prior);
            let score = q + self.opts.c_puct * p * (sqrt_sum / (1.0 + f64::from(visits)));
            if score > best_score {
                best_score = score;
                best = child;
                best_action = Some(a);
            }
        }

        let action = best_action?;
        Some(match best {
            Some(child) => child,
            None => {
                let player = self.tree.nodes[node].player;
                let child = self.tree.push(player, Some(action));
                self.tree.nodes[node].children.push((action, child));
                child
            }
        })
    }

    fn descend(
        &mut self,
        node: usize,
        s: &mut GameState,
        v: &VariantDef,
        rng: &mut SplitMix64,
        depth: usize,
    ) -> SeatValues {
        // Resolve chance owed before the node — open loop, except that a
        // battle roll at the frontier is valued exactly in `frontier_value`.
        for _ in 0..64 {
            if get_pending(s, v) != Pending::Chance {
                break;
            }
            if self.tree.nodes[node].visits == 0
                && s.phase == Phase::BattleRoll
                && !self.opts.rollout_leaf
            {
                break;
            }
            if resolve_chance_mut(s, v, rng).is_err() {
                break;
            }
        }

        let player = match get_pending(s, v) {
            Pending::Over => {
                let value = terminal_vector(s);
                self.tree.backup(node, &value);
                return value;
            }
            Pending::Chance => {
                let value = self.frontier_value(s, v, rng);
                self.tree.backup(node, &value);
                return value;
            }
            Pending::Decision { player } => player,
        };
        if depth >= self.opts.max_depth {
            let value = self.frontier_value(s, v, rng);
            self.tree.backup(node, &value);
            return value;
        }

        self.tree.nodes[node].player = player;

        // First visit: value the frontier and stop — one new node per
        // iteration. Priors wait for the second visit, so the many nodes the
        // search never returns to never pay for them.
        if self.tree.nodes[node].visits == 0 {
            let value = self.frontier_value(s, v, rng);
            self.tree.backup(node, &value);
            return value;
        }

        let mut legal = Vec::new();
        arcs_engine::legal_actions(s, v, &mut legal);
        let candidates = generate_candidates(
            s,
            v,
            player,
            &legal,
            &CandidateOpts {
                max: self.opts.max_actions,
                weights: self.opts.weights,
            },
        );
        if self.tree.nodes[node].priors.is_none() {
            let priors = self.compute_priors(s, v, player, &candidates);
            self.tree.nodes[node].priors = Some(priors);
        }

        let chosen = match self.puct_child(node, &candidates) {
            Some(child) => child,
            None => {
                let value = self.frontier_value(s, v, rng);
                self.tree.backup(node, &value);
                return value;
            }
        };
        let action = self.tree.nodes[chosen]
            .action
            .expect("a child has an action");
        if apply_action_mut(s, v, action).is_err() {
            // See `mcts.rs`: TS throws here, Rust answers the `Result`.
            let value = self.frontier_value(s, v, rng);
            self.tree.backup(node, &value);
            return value;
        }
        let value = self.descend(chosen, s, v, rng, depth + 1);
        self.tree.backup(node, &value);
        value
    }
}

impl Agent for Mcts2 {
    fn name(&self) -> &str {
        &self.name
    }

    fn choose(&mut self, obs: &Observation, legal: &[Action], ctx: &mut AgentCtx) -> usize {
        if legal.len() == 1 {
            return 0;
        }
        let v = ctx.variant;
        let root = self.tree.reset(ctx.player);
        let mut rng = ctx.rng;

        self.pool.clear();
        for _ in 0..self.opts.worlds {
            self.pool
                .push(determinize(obs, v, &mut rng, Default::default()));
        }
        let start = Instant::now();
        let budget = self.opts.time_ms.map(core::time::Duration::from_millis);

        for iter in 0..self.opts.iterations {
            if iter > 0
                && iter % DEADLINE_EVERY == 0
                && budget.is_some_and(|b| start.elapsed() >= b)
            {
                break;
            }
            let mut state = if self.pool.is_empty() {
                determinize(obs, v, &mut rng, Default::default())
            } else {
                self.pool[iter % self.pool.len()]
            };
            self.descend(root, &mut state, v, &mut rng, 0);
        }

        ctx.rng = rng;
        index_of(legal, self.tree.most_visited(root))
    }
}
