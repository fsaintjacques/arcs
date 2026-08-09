//! Shared plumbing for the sampling agents: playing a position out with a
//! cheap policy, and turning the reached position into a value for every
//! seat. Ported from `src/agents/rollout.ts`.

use arcs_engine::game::{apply_action_mut, get_pending, resolve_chance_mut, standings};
use arcs_engine::{Action, GameState, MAX_SEATS, Pending, Player, Rng, VariantDef};

use crate::eval::{Weights, evaluate};

/// A per-seat value vector. Entries past `s.players` are always 0.
pub type SeatValues = [f64; MAX_SEATS];

/// Value vector over seats: each seat's share of the total heuristic value,
/// summing to 1. Shares are what max^n backup wants — bounded, comparable
/// across positions, and zero-sum-ish without assuming two players.
/// (`valueVector` in rollout.ts.)
pub fn value_vector(s: &GameState, v: &VariantDef, w: &Weights) -> SeatValues {
    let players = s.players as usize;
    let mut raw = [0.0f64; MAX_SEATS];
    for (p, slot) in raw.iter_mut().enumerate().take(players) {
        *slot = evaluate(s, v, Player(p as u8), w);
    }
    let mut min = f64::INFINITY;
    for &x in raw.iter().take(players) {
        min = min.min(x);
    }
    // Shift so every share is non-negative, then normalise.
    let mut out = [0.0f64; MAX_SEATS];
    let mut total = 0.0f64;
    for p in 0..players {
        out[p] = raw[p] - min + 1e-6;
        total += out[p];
    }
    for slot in out.iter_mut().take(players) {
        *slot /= total;
    }
    out
}

/// Terminal value: 1 for the winner, 0 for everyone else. Power ties are
/// broken by turn order from the initiative holder — exactly the rule the
/// table plays by (`standings`, p19) — so search backs up the result the game
/// would actually award rather than splitting a tie the rules never split.
/// (`terminalVector` in rollout.ts.)
pub fn terminal_vector(s: &GameState) -> SeatValues {
    let mut out = [0.0f64; MAX_SEATS];
    out[standings(s).as_slice()[0].player.as_index()] = 1.0;
    out
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RolloutOpts {
    /// Decisions to play before falling back to the heuristic.
    pub depth: usize,
    pub weights: Weights,
}

impl Default for RolloutOpts {
    fn default() -> Self {
        RolloutOpts {
            depth: 24,
            weights: crate::eval::DEFAULT_WEIGHTS,
        }
    }
}

/// Play `opts.depth` decisions with a uniform-random-but-not-idle policy,
/// then value the position. Cheap on purpose: rollout quality matters much
/// less than the number of rollouts at this branching factor.
///
/// The TS version takes the state by reference and mutates it; so does this
/// one — callers pass a scratch copy (which is a memcpy here, not a
/// `cloneState`). (`rollout` in rollout.ts.)
pub fn rollout(
    s: &mut GameState,
    v: &VariantDef,
    rng: &mut impl Rng,
    opts: &RolloutOpts,
) -> SeatValues {
    let mut actions: Vec<Action> = Vec::new();
    for _ in 0..opts.depth {
        match get_pending(s, v) {
            Pending::Over => return terminal_vector(s),
            Pending::Chance => {
                if resolve_chance_mut(s, v, rng).is_err() {
                    break;
                }
                continue;
            }
            Pending::Decision { .. } => {}
        }
        arcs_engine::legal_actions(s, v, &mut actions);
        let pick = pick_useful(&actions, rng);
        if apply_action_mut(s, v, pick).is_err() {
            break;
        }
    }
    if get_pending(s, v) == Pending::Over {
        return terminal_vector(s);
    }
    value_vector(s, v, &opts.weights)
}

/// The `random+` policy: uniform over the actions that do something, falling
/// back to the whole list when idling is all that is on offer.
pub(crate) fn useful_indices(actions: &[Action]) -> Vec<usize> {
    (0..actions.len())
        .filter(|&i| !matches!(actions[i], Action::EndTurn | Action::Seize { .. }))
        .collect()
}

fn pick_useful(actions: &[Action], rng: &mut impl Rng) -> Action {
    let useful = useful_indices(actions);
    if useful.is_empty() {
        actions[rng.gen_range(actions.len())]
    } else {
        actions[useful[rng.gen_range(useful.len())]]
    }
}

/// Trim a wide action list to `max` entries. Arcs offers hundreds of legal
/// actions at some nodes (every ship count on every edge, every dice split),
/// and a fixed-budget search spends itself on breadth if it sees them all.
///
/// The trim is **deterministic**: same action list in, same subset out. A
/// determinized search revisits a node under different sampled worlds, and a
/// randomised trim would give the node a different child set each time, so its
/// statistics would describe no particular decision. Kinds are taken
/// round-robin, so no action type is ever invisible however many `move`
/// variations crowd the list.
///
/// Where TS groups by the `t` tag string, Rust groups by the enum
/// discriminant — the same partition, since the tags are 1:1 with the
/// variants. (`narrow` in rollout.ts.)
pub fn narrow(actions: &[Action], max: usize) -> Vec<Action> {
    if actions.len() <= max {
        return actions.to_vec();
    }
    let by_kind = group_by_kind(actions);
    round_robin(&by_kind, max)
}

/// Group actions by variant, keeping first-appearance order of the kinds and
/// enumeration order within each — the JS `Map` iteration order `narrow`
/// relies on.
pub(crate) fn group_by_kind(actions: &[Action]) -> Vec<Vec<Action>> {
    let mut kinds: Vec<core::mem::Discriminant<Action>> = Vec::new();
    let mut lists: Vec<Vec<Action>> = Vec::new();
    for &a in actions {
        let d = core::mem::discriminant(&a);
        match kinds.iter().position(|k| *k == d) {
            Some(i) => lists[i].push(a),
            None => {
                kinds.push(d);
                lists.push(vec![a]);
            }
        }
    }
    lists
}

/// Take one action per kind per round until `max` is reached.
pub(crate) fn round_robin(by_kind: &[Vec<Action>], max: usize) -> Vec<Action> {
    let mut out: Vec<Action> = Vec::with_capacity(max);
    let mut round = 0usize;
    while out.len() < max {
        let mut added = false;
        for list in by_kind {
            if round >= list.len() {
                continue;
            }
            out.push(list[round]);
            added = true;
            if out.len() >= max {
                break;
            }
        }
        if !added {
            break;
        }
        round += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcs_engine::{ActionCardId, SystemId};

    fn sample() -> Vec<Action> {
        vec![
            Action::Move {
                from: SystemId(0),
                to: SystemId(1),
                ships: 1,
            },
            Action::Move {
                from: SystemId(0),
                to: SystemId(2),
                ships: 1,
            },
            Action::Move {
                from: SystemId(0),
                to: SystemId(3),
                ships: 1,
            },
            Action::Move {
                from: SystemId(0),
                to: SystemId(4),
                ships: 1,
            },
            Action::BuildShip {
                system: SystemId(1),
                building: 0,
            },
            Action::EndTurn,
        ]
    }

    // Ported from tests/agents.test.ts "narrow is a no-op when the list
    // already fits".
    #[test]
    fn narrow_is_a_no_op_when_the_list_already_fits() {
        let actions = sample();
        assert_eq!(narrow(&actions, 10), actions);
    }

    // Ported from tests/agents.test.ts "keeps at least one of every action
    // kind".
    #[test]
    fn narrow_keeps_at_least_one_of_every_action_kind() {
        let actions = sample();
        let kept = narrow(&actions, 3);
        assert_eq!(kept.len(), 3);
        let kinds: Vec<_> = kept.iter().map(core::mem::discriminant).collect();
        assert!(kinds.contains(&core::mem::discriminant(&actions[0])));
        assert!(kinds.contains(&core::mem::discriminant(&actions[4])));
        assert!(kinds.contains(&core::mem::discriminant(&Action::EndTurn)));
    }

    // Ported from tests/agents.test.ts "is deterministic, so a search node
    // keeps the same children".
    #[test]
    fn narrow_is_deterministic() {
        let actions = sample();
        assert_eq!(narrow(&actions, 4), narrow(&actions, 4));
    }

    #[test]
    fn useful_indices_skip_idling_and_seizing() {
        let actions = vec![
            Action::EndTurn,
            Action::Seize {
                card: ActionCardId(3),
            },
            Action::CatapultStop,
        ];
        assert_eq!(useful_indices(&actions), vec![2]);
        assert!(useful_indices(&actions[..2]).is_empty());
    }
}
