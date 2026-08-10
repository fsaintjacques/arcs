//! Determinized Information-Set MCTS (ISMCTS) with max^n backup. Ported from
//! `src/agents/mcts.ts`.
//!
//! Arcs is an imperfect-information game with more than two players, which
//! rules out plain UCT on a shared tree and rules out minimax backup:
//!
//!   - **Determinization**: every iteration samples a world consistent with
//!     the agent's observation ([`determinize`]) and searches that. Nodes are
//!     keyed by the action sequence from the root, so statistics pool across
//!     worlds — the "single-observer ISMCTS" formulation.
//!   - **max^n backup**: each node carries a value *per seat*, and selection
//!     at a node maximises the value of the seat to move there. With 3-4
//!     players that is the honest generalisation of UCT; assuming the
//!     opponents minimise the root player's score would model a coalition
//!     that is not in the game.
//!   - **Chance nodes** are resolved from the RNG as encountered, so the tree
//!     is open-loop across dice: children of a chance node pool over
//!     outcomes, which is what makes a fixed iteration budget usable at this
//!     branching factor.
//!
//! This is the older search. It stays in the port because the gauntlet ladder
//! names two frozen anchors built from it (`anchor-mcts300-v0`,
//! `anchor-mcts-c-v1`), and an anchor is never re-implemented in terms of a
//! newer agent — the whole point is that it cannot move.

use arcs_engine::game::{apply_action_mut, get_pending, resolve_chance_mut};
use arcs_engine::{
    Action, GameState, Observation, Pending, Rng, SplitMix64, VariantDef, determinize,
};

use crate::agent::{Agent, AgentCtx};
use crate::candidates::{CandidateOpts, generate_candidates};
use crate::eval::{DEFAULT_WEIGHTS, Weights};
use crate::rollout::{RolloutOpts, SeatValues, narrow, rollout, terminal_vector, value_vector};
use crate::tree::{Arena, index_of};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MctsOpts {
    /// Search iterations per decision.
    pub iterations: usize,
    /// Exploration constant of UCT.
    pub c: f64,
    /// Decisions played per rollout before the heuristic values the position.
    pub rollout_depth: usize,
    /// Cap on branching at any one node (see [`narrow`]).
    pub max_actions: usize,
    /// Tree depth in decisions; beyond this the rollout policy takes over.
    pub max_depth: usize,
    /// Trim wide nodes with [`generate_candidates`] instead of blind
    /// [`narrow`].
    pub candidates: bool,
    pub weights: Weights,
}

impl Default for MctsOpts {
    fn default() -> Self {
        MctsOpts {
            iterations: 300,
            c: 0.9,
            rollout_depth: 30,
            max_actions: 12,
            max_depth: 12,
            candidates: false,
            weights: DEFAULT_WEIGHTS,
        }
    }
}

pub struct Mcts {
    name: String,
    opts: MctsOpts,
    tree: Arena,
}

impl Mcts {
    pub fn new(name: impl Into<String>, opts: MctsOpts) -> Self {
        Mcts {
            name: name.into(),
            opts,
            tree: Arena::new(),
        }
    }

    /// UCT over the children that are legal *in this determinization*.
    /// Availability varies between iterations, so the parent visit count is
    /// replaced by the sum over the currently-available children — the
    /// standard ISMCTS correction that stops rarely-legal actions from
    /// looking unexplored forever.
    fn uct_child(&self, node: usize, available: &[usize]) -> Option<usize> {
        let seat = self.tree.nodes[node].player.as_index();
        let total: u32 = available.iter().map(|&c| self.tree.nodes[c].visits).sum();
        let log_total = f64::from(total.max(2)).ln();

        let mut best = None;
        let mut best_score = f64::NEG_INFINITY;
        for &child in available {
            let n = &self.tree.nodes[child];
            if n.visits == 0 {
                return Some(child);
            }
            let visits = f64::from(n.visits);
            let exploit = n.totals[seat] / visits;
            let explore = self.opts.c * (log_total / visits).sqrt();
            let score = exploit + explore;
            if score > best_score {
                best_score = score;
                best = Some(child);
            }
        }
        best
    }

    fn descend(
        &mut self,
        node: usize,
        s: &mut GameState,
        v: &VariantDef,
        rng: &mut SplitMix64,
        depth: usize,
    ) -> SeatValues {
        // Resolve any chance the position owes before looking at the node.
        for _ in 0..64 {
            if get_pending(s, v) != Pending::Chance {
                break;
            }
            if resolve_chance_mut(s, v, rng).is_err() {
                break;
            }
        }

        let pending = get_pending(s, v);
        let player = match pending {
            Pending::Over => {
                let value = terminal_vector(s);
                self.tree.backup(node, &value);
                return value;
            }
            Pending::Chance => {
                let value = value_vector(s, v, &self.opts.weights);
                self.tree.backup(node, &value);
                return value;
            }
            Pending::Decision { player } => player,
        };
        if depth >= self.opts.max_depth {
            let value = self.rollout_from(s, v, rng);
            self.tree.backup(node, &value);
            return value;
        }

        self.tree.nodes[node].player = player;

        // Legality is re-derived every visit: this determinization's dice and
        // hands may differ from the one that first expanded this node.
        let mut legal = Vec::new();
        arcs_engine::legal_actions(s, v, &mut legal);
        let legal = if self.opts.candidates {
            generate_candidates(
                s,
                v,
                player,
                &legal,
                &CandidateOpts {
                    max: self.opts.max_actions,
                    weights: self.opts.weights,
                },
            )
        } else {
            narrow(&legal, self.opts.max_actions)
        };

        let mut untried: Vec<Action> = Vec::new();
        let mut available: Vec<usize> = Vec::new();
        for &a in &legal {
            match self.tree.nodes[node].child(a) {
                Some(child) => available.push(child),
                None => untried.push(a),
            }
        }

        if !untried.is_empty() {
            let action = untried[rng.gen_range(untried.len())];
            let child = self.tree.push(player, Some(action));
            self.tree.nodes[node].children.push((action, child));
            // TS lets `applyActionMut` throw here; the actions come from the
            // enumerator for *this* world, so it never legitimately does.
            // Rust must answer the `Result`, and valuing the position where
            // it stands keeps one buggy iteration from aborting a batch.
            let value = if apply_action_mut(s, v, action).is_err() {
                value_vector(s, v, &self.opts.weights)
            } else {
                self.rollout_from(s, v, rng)
            };
            self.tree.backup(child, &value);
            self.tree.backup(node, &value);
            return value;
        }

        let chosen = match self.uct_child(node, &available) {
            Some(child) => child,
            None => {
                let value = value_vector(s, v, &self.opts.weights);
                self.tree.backup(node, &value);
                return value;
            }
        };
        let action = self.tree.nodes[chosen]
            .action
            .expect("a child has an action");
        if apply_action_mut(s, v, action).is_err() {
            let value = value_vector(s, v, &self.opts.weights);
            self.tree.backup(node, &value);
            return value;
        }
        let value = self.descend(chosen, s, v, rng, depth + 1);
        self.tree.backup(node, &value);
        value
    }

    fn rollout_from(&self, s: &mut GameState, v: &VariantDef, rng: &mut SplitMix64) -> SeatValues {
        rollout(
            s,
            v,
            rng,
            &RolloutOpts {
                depth: self.opts.rollout_depth,
                weights: self.opts.weights,
            },
        )
    }
}

impl Agent for Mcts {
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

        for _ in 0..self.opts.iterations {
            let mut state = determinize(obs, v, &mut rng, Default::default());
            self.descend(root, &mut state, v, &mut rng, 0);
        }

        ctx.rng = rng;
        index_of(legal, self.tree.most_visited(root))
    }
}
