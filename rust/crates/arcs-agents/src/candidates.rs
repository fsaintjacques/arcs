//! Informed candidate generation — [`crate::narrow`] with eyes. Ported from
//! `src/agents/candidates.ts`.
//!
//! `narrow` trims wide nodes deterministically, one action kind at a time,
//! but *within* a kind it keeps whatever the legality enumerator emitted
//! first — for `move` that is "first source, first neighbour, one ship",
//! which FINDINGS.md ranks as the search's single biggest loss. This keeps
//! narrow's skeleton (deterministic, output ⊆ input, every kind stays
//! visible) and replaces enumeration order with an informed ordering per
//! kind, so the round-robin's early picks are the actions worth searching.
//!
//! FINDINGS records it as the first separated search-tier gain
//! (+15.0 ± 12.7), so its determinism is load-bearing: the same state and the
//! same list must always produce the same subset, or a determinized search
//! node's statistics describe no particular decision.
//!
//! Contract: a pure filter. It never synthesises an action, so it cannot
//! change legality; it only decides which legal actions a budget-bound
//! search gets to see.

use arcs_engine::cards::action_card;
use arcs_engine::map::{SYSTEM_COUNT, SystemKind};
use arcs_engine::state::MAX_SEATS;
use arcs_engine::{Action, GameState, Player, SystemId, VariantDef};

use crate::dicemath::expected_battle;
use crate::eval::Weights;
use crate::rollout::{group_by_kind, round_robin};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CandidateOpts {
    pub max: usize,
    pub weights: Weights,
}

impl Default for CandidateOpts {
    fn default() -> Self {
        CandidateOpts {
            max: 12,
            weights: crate::eval::DEFAULT_WEIGHTS,
        }
    }
}

/// Stable ranking: score descending, enumeration order as the tiebreak.
fn ranked(list: &mut [Action], score: impl Fn(&Action) -> f64) {
    let mut scored: Vec<(f64, usize, Action)> = list
        .iter()
        .enumerate()
        .map(|(i, a)| (score(a), i, *a))
        .collect();
    scored.sort_by(|x, y| {
        y.0.partial_cmp(&x.0)
            .expect("candidate scores are finite")
            .then_with(|| x.1.cmp(&y.1))
    });
    for (slot, row) in list.iter_mut().zip(scored) {
        *slot = row.2;
    }
}

/// How much a destination is worth moving toward. Cheap and positional.
///
/// The TS version memoizes into a `Map` keyed by destination; Rust uses a
/// dense per-system array with the same lazy fill, so the score of a system
/// is computed at most once per call.
fn edge_scores(
    s: &GameState,
    v: &VariantDef,
    player: Player,
    tos: impl Iterator<Item = SystemId>,
) -> [f64; SYSTEM_COUNT] {
    let mut scores = [0.0f64; SYSTEM_COUNT];
    let mut done = [false; SYSTEM_COUNT];
    for to in tos {
        let i = to.as_index();
        if done[i] {
            continue;
        }
        done[i] = true;
        let st = &s.systems[i];
        let mut score = 0.0f64;
        let mut rival_ships = 0u32;
        let mut rival_buildings = 0u32;
        for p in 0..s.players as usize {
            if p == player.as_index() {
                continue;
            }
            rival_ships += st.fresh[p] as u32 + st.damaged[p] as u32;
        }
        for b in st.buildings.iter() {
            if b.player() != player {
                rival_buildings += 1;
            }
        }
        if rival_ships == 0 && s.control_of(to) != Some(player) {
            score += 2.0; // uncontested claim
        }
        if rival_buildings > 0 {
            score += 2.0; // something to raid or besiege
        }
        if rival_ships > 0 {
            score += 1.0; // contact
        }
        if v.systems[i].kind == SystemKind::Planet
            && st.buildings.len() < v.systems[i].building_slots as usize
        {
            score += 1.0; // room to build
        }
        scores[i] = score;
    }
    scores
}

/// Who controls a system if `player`'s fresh count there were `my_fresh`.
fn control_with(
    fresh: &[u8; MAX_SEATS],
    players: u8,
    player: Player,
    my_fresh: i32,
) -> Option<Player> {
    let mut best = -1i32;
    let mut winner = None;
    let mut tied = false;
    for (p, &here) in fresh.iter().enumerate().take(players as usize) {
        let n = if p == player.as_index() {
            my_fresh
        } else {
            here as i32
        };
        if n > best {
            best = n;
            winner = Some(Player(p as u8));
            tied = false;
        } else if n == best {
            tied = true;
        }
    }
    if best <= 0 || tied { None } else { winner }
}

/// The exact control swing a move causes, in the 1-ply evaluation's terms.
/// Control is the only evaluation term a bare move changes (ships keep their
/// count, buildings stay put), so ordering moves by it *is* ordering them the
/// way a full apply_action scan would, at a fraction of the cost — the
/// coverage test measures exactly this agreement.
fn move_control_delta(
    s: &GameState,
    player: Player,
    from: SystemId,
    to: SystemId,
    ships: u8,
) -> f64 {
    let mut delta = 0.0f64;
    let from_st = &s.systems[from.as_index()];
    let to_st = &s.systems[to.as_index()];
    let cases = [
        (
            from_st,
            from_st.fresh[player.as_index()] as i32 - ships as i32,
        ),
        (to_st, to_st.fresh[player.as_index()] as i32 + ships as i32),
    ];
    for (st, my_after) in cases {
        let before = control_with(
            &st.fresh,
            s.players,
            player,
            st.fresh[player.as_index()] as i32,
        );
        let after = control_with(&st.fresh, s.players, player, my_after);
        if before != after {
            if before == Some(player) {
                delta -= 1.0;
            } else if before.is_some() {
                delta += 1.0; // a rival lost control
            }
            if after == Some(player) {
                delta += 1.0;
            } else if after.is_some() {
                delta -= 1.0; // a rival gained control
            }
        }
    }
    delta
}

/// Order one kind's actions so the best-looking come first. Unhandled kinds
/// keep enumeration order, which is exactly `narrow`'s behaviour.
fn order_kind(list: &mut [Action], s: &GameState, v: &VariantDef, player: Player, w: &Weights) {
    match list.first() {
        Some(Action::Move { .. }) => {
            let edges = edge_scores(
                s,
                v,
                player,
                list.iter().filter_map(|a| match a {
                    Action::Move { to, .. } => Some(*to),
                    _ => None,
                }),
            );
            ranked(list, |a| match a {
                Action::Move { from, to, ships } => {
                    // The exact control swing dominates; the positional edge
                    // score and a smallest-detachment preference only break
                    // its ties.
                    move_control_delta(s, player, *from, *to, *ships) * 100.0 + edges[to.as_index()]
                        - *ships as f64 * 0.01
                }
                _ => 0.0,
            });
        }
        Some(Action::Catapult { .. }) => {
            let edges = edge_scores(
                s,
                v,
                player,
                list.iter().filter_map(|a| match a {
                    Action::Catapult { to, .. } => Some(*to),
                    _ => None,
                }),
            );
            ranked(list, |a| match a {
                Action::Catapult { to, ships } => edges[to.as_index()] + *ships as f64 * 0.01,
                _ => 0.0,
            });
        }
        Some(Action::Battle { .. }) => {
            ranked(list, |a| match a {
                Action::Battle {
                    system,
                    defender,
                    assault,
                    skirmish,
                    raid,
                } => {
                    let e = expected_battle(*assault, *skirmish, *raid);
                    let defender_buildings = s.systems[system.as_index()]
                        .buildings
                        .iter()
                        .filter(|b| b.player() == *defender)
                        .count();
                    // Hits win fights, self-hits lose them, keys only matter
                    // with something to raid. A bigger pool of the right
                    // shape ranks higher.
                    e.hits + e.building_hits * 0.6 - 1.2 * e.self_hits
                        + if defender_buildings > 0 {
                            e.keys * 0.8
                        } else {
                            0.0
                        }
                }
                _ => 0.0,
            });
        }
        Some(Action::CardPrelude { .. }) => {
            ranked(list, |a| match a {
                Action::CardPrelude {
                    cards: Some(cards), ..
                } => {
                    // Farseers' recycle: prefer swapping many low-pip cards.
                    let mut score = 0.0f64;
                    for &card in cards.iter() {
                        score += 4.0 - action_card(card).pips as f64;
                    }
                    score * 0.1 + cards.len() as f64 * 0.01
                }
                _ => 0.0,
            });
        }
        Some(Action::CardAction { .. }) => {
            ranked(list, |a| match a {
                Action::CardAction {
                    gain: Some(gain), ..
                } => {
                    // Pressgang-style multisets: rank by what the resources
                    // are worth.
                    let mut score = 0.0f64;
                    for &r in gain.iter() {
                        score += w.resource_value[r.as_index()];
                    }
                    score
                }
                _ => 0.0,
            });
        }
        _ => {}
    }
}

/// Trim `actions` to at most `opts.max`, keeping the ones worth a search
/// budget. Same guarantees as [`crate::narrow`]: deterministic for a given
/// state and list, output ⊆ input, no kind ever disappears entirely.
/// (`generateCandidates` in candidates.ts.)
pub fn generate_candidates(
    s: &GameState,
    v: &VariantDef,
    player: Player,
    actions: &[Action],
    opts: &CandidateOpts,
) -> Vec<Action> {
    if actions.len() <= opts.max {
        return actions.to_vec();
    }
    let mut by_kind = group_by_kind(actions);
    for list in by_kind.iter_mut() {
        order_kind(list, s, v, player, &opts.weights);
    }
    // narrow()'s round-robin over kinds, now consuming informed orderings.
    round_robin(&by_kind, opts.max)
}
