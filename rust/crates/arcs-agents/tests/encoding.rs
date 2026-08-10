//! The global action encoding is total, in-range, and injective everywhere it
//! claims to be.
//!
//! All three properties are checked against a **sampled action corpus**: every
//! legal action offered at every decision node of a batch of seeded random
//! games across 2, 3 and 4 players. That is the only honest way to test a
//! total function over a parameterized action space — the flat space is
//! ~10^9 wide (see `arcs_agents::encoding`), while a real game reaches ~10^4
//! distinct actions.
//!
//! `corpus_shape_is_what_the_encoding_documents` prints and pins the numbers
//! the module's doc comment cites, so the documentation cannot drift away from
//! the engine it describes.

use std::collections::{HashMap, HashSet};

use arcs_agents::encoding::{
    ActionKindId, HEAD_SIZES, HeadTargets, action_targets, decode_global_index, global_index,
    is_well_formed,
};
use arcs_engine::{
    Action, Pending, SetupMode, SplitMix64, apply_action_mut, get_pending, legal_actions,
    make_variant, new_game, resolve_chance_mut,
};

/// Every action offered at every decision node of `games` random games per
/// player count. `random+` is not used: a uniform-random driver visits odd
/// corners of the rules (idle turns, doomed battles) that a policy-shaped one
/// never would, and those corners are exactly where an encoding gap hides.
fn corpus(games: u64) -> (Vec<Action>, CorpusStats) {
    let mut all: Vec<Action> = Vec::new();
    let mut stats = CorpusStats::default();
    for players in [2u8, 3, 4] {
        for seed in 0..games {
            let v = make_variant(players, seed, SetupMode::Draw);
            let mut rng = SplitMix64::new(seed ^ (players as u64) << 32);
            let mut s = new_game(&v, &mut rng, seed, SetupMode::Draw);
            let mut legal = Vec::new();
            let mut guard = 0;
            loop {
                match get_pending(&s, &v) {
                    Pending::Over => break,
                    Pending::Chance => resolve_chance_mut(&mut s, &v, &mut rng).unwrap(),
                    Pending::Decision { .. } => {
                        legal_actions(&s, &v, &mut legal);
                        assert!(!legal.is_empty(), "a decision node with no actions");
                        stats.nodes += 1;
                        stats.offered += legal.len() as u64;
                        stats.widest = stats.widest.max(legal.len());
                        all.extend_from_slice(&legal);
                        let pick = legal[arcs_engine::Rng::gen_range(&mut rng, legal.len())];
                        apply_action_mut(&mut s, &v, pick).unwrap();
                    }
                }
                guard += 1;
                assert!(guard < 200_000, "game failed to terminate");
            }
            stats.games += 1;
        }
    }
    (all, stats)
}

#[derive(Default, Debug)]
struct CorpusStats {
    games: u64,
    nodes: u64,
    offered: u64,
    widest: usize,
}

/// A cheaper corpus for the properties that do not need the full sweep.
fn small_corpus() -> Vec<Action> {
    corpus(12).0
}

/// `action_targets` is total: every action the engine can offer has a target,
/// and computing it never panics. The exhaustive `match` in `encoding.rs` is
/// what guarantees a *new* variant is a compile error; this checks the
/// arithmetic inside each arm over real parameters.
#[test]
fn action_targets_is_total_over_the_sampled_corpus() {
    let actions = small_corpus();
    assert!(
        actions.len() > 100_000,
        "corpus is too small to be evidence"
    );
    for a in &actions {
        let t = action_targets(*a);
        assert!(t.kind as usize <= ActionKindId::Reinforce as usize);
    }
}

/// Every field value stays inside its declared cardinality. This is what makes
/// `HEAD_SIZES` trustworthy as a versioned contract: a head one value too
/// narrow would silently alias two actions under a trained policy.
#[test]
fn field_values_stay_inside_head_sizes() {
    for a in small_corpus() {
        let t = action_targets(a);
        assert!(
            is_well_formed(t),
            "{t:?} escapes HEAD_SIZES for {}",
            arcs_engine::encode_action(a)
        );
    }
}

/// `global_index` is injective: distinct targets get distinct keys, and the
/// key decodes back to the target it came from.
#[test]
fn global_index_is_injective_over_the_sampled_corpus() {
    let mut seen: HashMap<u64, HeadTargets> = HashMap::new();
    for a in small_corpus() {
        let t = action_targets(a);
        let n = global_index(t);
        assert_eq!(decode_global_index(n), t, "index {n} does not round-trip");
        if let Some(prev) = seen.insert(n, t) {
            assert_eq!(prev, t, "index {n} collides: {prev:?} vs {t:?}");
        }
    }
    assert!(seen.len() > 1_000, "only {} distinct targets", seen.len());
}

/// `action_targets` is injective on `Action` for every kind except the two
/// that carry a *list* — Farseers' recycle (`CardPrelude::cards`) and
/// Pressgang's gain (`CardAction::gain`), which are summarised by their
/// length. That lossiness is deliberate and documented in `encoding.rs`;
/// pinning it here means it cannot silently spread to another kind.
#[test]
fn action_targets_is_injective_outside_the_list_variants() {
    let mut by_target: HashMap<HeadTargets, Action> = HashMap::new();
    let mut collisions: HashSet<u8> = HashSet::new();
    for a in small_corpus() {
        let t = action_targets(a);
        if let Some(prev) = by_target.insert(t, a)
            && prev != a
        {
            collisions.insert(t.kind);
        }
    }
    let allowed: HashSet<u8> = [
        ActionKindId::CardPrelude as u8,
        ActionKindId::CardAction as u8,
    ]
    .into_iter()
    .collect();
    assert!(
        collisions.is_subset(&allowed),
        "unexpected target collisions in kinds {:?}",
        collisions.difference(&allowed).collect::<Vec<_>>()
    );
}

/// The corpus statistics the `encoding` module's doc comment cites, measured
/// on this engine. Ignored by default because the full sweep takes about a
/// minute; run it when the numbers in the docs need re-checking:
///
/// ```text
/// cargo test -p arcs-agents --release --test encoding -- --ignored --nocapture
/// ```
#[test]
#[ignore = "benchmark"]
fn corpus_shape_is_what_the_encoding_documents() {
    let (actions, stats) = corpus(400);
    let distinct: HashSet<Action> = actions.iter().copied().collect();
    let kinds: HashSet<u8> = actions.iter().map(|a| action_targets(*a).kind).collect();
    let targets: HashSet<HeadTargets> = actions.iter().map(|a| action_targets(*a)).collect();
    println!(
        "{} games, {} decision nodes, {} distinct actions, {} distinct kinds, \
         {} distinct head targets, mean {:.1} / max {} legal actions per node",
        stats.games,
        stats.nodes,
        distinct.len(),
        kinds.len(),
        targets.len(),
        stats.offered as f64 / stats.nodes as f64,
        stats.widest,
    );
    println!(
        "head outputs {}, global span {}",
        HEAD_SIZES.total_outputs(),
        HEAD_SIZES.global_span()
    );
    // Loose bounds: the point is that the documented order of magnitude is
    // right, not that a random sweep reproduces a number exactly.
    assert!((5_000..50_000).contains(&distinct.len()));
    assert!(kinds.len() >= 30);
    assert!(stats.widest > 100);
}
