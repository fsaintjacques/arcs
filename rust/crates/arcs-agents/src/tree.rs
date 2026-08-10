//! The node arena both search agents share.
//!
//! TS keys children by `encodeAction` strings in a `Map` (`src/agents/mcts.ts`
//! and `mcts2.ts`); Rust keys them by the [`Action`] enum itself, which is
//! `Copy + Eq + Hash` and at most 24 bytes. That is one of the port's stated
//! wins: a search that expands hundreds of nodes per decision no longer
//! formats and allocates a string per child lookup.
//!
//! The child map is an **insertion-ordered `Vec`**, not a `HashMap`. Two
//! reasons, both load-bearing:
//!
//! - `std`'s `HashMap` seeds its hasher randomly per process, so iterating it
//!   to find the most-visited child would break ties differently from run to
//!   run. Every measurement in `docs/GAUNTLET.md` rests on "same seed, same
//!   game"; a randomised tie-break would quietly forfeit it.
//! - A node holds at most `max_actions` children (12-16), so a linear scan
//!   over a contiguous `Vec` beats hashing a 24-byte enum anyway.
//!
//! Nodes live in a flat `Vec` and refer to each other by index rather than by
//! `Box`, because the descent is recursive and mutates the parent after the
//! child returns — an arena makes that a pair of independent indexes instead
//! of a borrow-checker argument.

use arcs_engine::{Action, MAX_SEATS, Player};

use crate::rollout::SeatValues;

/// One search node. `totals` is per seat (max^n), so selection at a node can
/// maximise the value of *the seat to move there* rather than assuming the
/// opponents form a coalition against the root — the honest generalisation of
/// UCT to 3-4 players.
pub(crate) struct Node {
    /// Seat to move at this node.
    pub player: Player,
    pub visits: u32,
    /// Total backed-up value per seat.
    pub totals: SeatValues,
    /// Children in creation order, keyed by the action that reaches them.
    pub children: Vec<(Action, usize)>,
    /// The action that reaches this node from its parent (`None` at the root).
    pub action: Option<Action>,
    /// Softmax prior per action, fixed at expansion time (mcts2 only).
    pub priors: Option<Vec<(Action, f64)>>,
}

impl Node {
    #[inline]
    pub fn child(&self, a: Action) -> Option<usize> {
        self.children
            .iter()
            .find_map(|(key, i)| (*key == a).then_some(*i))
    }

    #[inline]
    pub fn prior(&self, a: Action) -> Option<f64> {
        self.priors
            .as_ref()?
            .iter()
            .find_map(|(key, p)| (*key == a).then_some(*p))
    }
}

/// The node arena. Cleared and reused between decisions, so a long game does
/// not re-allocate a tree per move.
pub(crate) struct Arena {
    pub nodes: Vec<Node>,
}

impl Arena {
    pub fn new() -> Self {
        Arena { nodes: Vec::new() }
    }

    /// Drop the previous decision's tree and start a fresh root.
    pub fn reset(&mut self, player: Player) -> usize {
        self.nodes.clear();
        self.push(player, None)
    }

    pub fn push(&mut self, player: Player, action: Option<Action>) -> usize {
        self.nodes.push(Node {
            player,
            visits: 0,
            totals: [0.0; MAX_SEATS],
            children: Vec::new(),
            action,
            priors: None,
        });
        self.nodes.len() - 1
    }

    #[inline]
    pub fn backup(&mut self, node: usize, value: &SeatValues) {
        let n = &mut self.nodes[node];
        n.visits += 1;
        for (slot, v) in n.totals.iter_mut().zip(value.iter()) {
            *slot += v;
        }
    }

    /// The most-visited child of `node`, which is what both agents play:
    /// robust to a single lucky evaluation in a way that the highest-value
    /// child is not. Ties go to the child created first, matching the TS
    /// scan over `Map` insertion order.
    pub fn most_visited(&self, node: usize) -> Option<Action> {
        let mut best = None;
        let mut best_visits = 0u32;
        for &(action, child) in &self.nodes[node].children {
            let visits = self.nodes[child].visits;
            if best.is_none() || visits > best_visits {
                best_visits = visits;
                best = Some(action);
            }
        }
        best
    }
}

/// Translate the search's chosen action back into an index into the *real*
/// legal list, which is what [`crate::Agent`] returns.
///
/// The search runs in determinized worlds, but only Rival hands, deck order
/// and face-down plays differ between a world and the true state, and none of
/// those change what the *root* seat may legally do — so the lookup succeeds.
/// Index 0 is the defensive fallback rather than a panic: an agent that
/// cannot find its own move should play a legal one, not abort a batch.
pub(crate) fn index_of(legal: &[Action], action: Option<Action>) -> usize {
    match action {
        Some(a) => legal.iter().position(|x| *x == a).unwrap_or(0),
        None => 0,
    }
}
