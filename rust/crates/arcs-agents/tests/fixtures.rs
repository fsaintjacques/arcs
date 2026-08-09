//! Cross-language agreement with the TypeScript agents.
//!
//! The fixtures are printed straight from the TS modules (read-only `npx tsx`
//! one-liners, recorded in the PR that added them) and checked in, so these
//! tests fail if either side drifts.
//!
//! Two of the values here are contracts rather than conveniences. The frozen
//! anchor weights are what every past `docs/GAUNTLET.md` row was measured
//! against — an anchor that evaluates differently after the port silently
//! re-baselines the whole ladder — and the battle distribution is claimed
//! *exact*, so it is compared at 1e-12 rather than eyeballed.

use arcs_agents::{
    ANCHOR_LADDER, ANCHOR_MCTS2_V2_CONFIG, ANCHOR_WEIGHTS_V0, ANCHOR_WEIGHTS_V1, DEFAULT_WEIGHTS,
    Weights, battle_distribution,
};
use arcs_engine::types::ResourceType;
use serde_json::Value;

/// The tolerance for values the port claims are exact. Both sides compute in
/// f64; only summation order can differ, so anything above this is a real
/// disagreement rather than rounding.
const EXACT: f64 = 1e-12;

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

fn f(v: &Value, key: &str) -> f64 {
    v.get(key)
        .unwrap_or_else(|| panic!("fixture is missing '{key}'"))
        .as_f64()
        .unwrap_or_else(|| panic!("'{key}' is not a number"))
}

/// Compare a `Weights` struct against its TS object field by field, so a
/// mismatch names the term instead of just failing.
fn assert_weights_match(rust: &Weights, ts: &Value, what: &str) {
    let terms: [(&str, f64); 20] = [
        ("power", rust.power),
        ("declaredLead", rust.declared_lead),
        ("declaredContest", rust.declared_contest),
        ("latentAmbition", rust.latent_ambition),
        ("freshShip", rust.fresh_ship),
        ("damagedShip", rust.damaged_ship),
        ("starport", rust.starport),
        ("city", rust.city),
        ("control", rust.control),
        ("resourceSlot", rust.resource_slot),
        ("courtAgent", rust.court_agent),
        ("courtLead", rust.court_lead),
        ("guildCard", rust.guild_card),
        ("initiative", rust.initiative),
        ("handCard", rust.hand_card),
        ("handPips", rust.hand_pips),
        ("handActionable", rust.hand_actionable),
        ("handHighCard", rust.hand_high_card),
        ("declarableLead", rust.declarable_lead),
        ("outrage", rust.outrage),
    ];
    for (name, value) in terms {
        assert_eq!(
            value,
            f(ts, name),
            "{what}.{name} disagrees with TypeScript"
        );
    }

    let rv = ts.get("resourceValue").expect("resourceValue");
    for (r, key) in [
        (ResourceType::Material, "material"),
        (ResourceType::Fuel, "fuel"),
        (ResourceType::Weapon, "weapon"),
        (ResourceType::Relic, "relic"),
        (ResourceType::Psionic, "psionic"),
    ] {
        assert_eq!(
            rust.resource_value[r.as_index()],
            f(rv, key),
            "{what}.resourceValue.{key} disagrees with TypeScript"
        );
    }
}

#[test]
fn default_weights_match_typescript() {
    let fx = fixture("weights.json");
    assert_weights_match(&DEFAULT_WEIGHTS, &fx["defaultWeights"], "defaultWeights");
}

/// The frozen anchors are the gauntlet's yardsticks. These must be exact:
/// `docs/GAUNTLET.md` records that an anchor's weights are a literal copy
/// taken on freeze day precisely so live tuning cannot move past results.
#[test]
fn frozen_anchor_weights_match_typescript() {
    let fx = fixture("weights.json");
    assert_weights_match(
        &ANCHOR_WEIGHTS_V0,
        &fx["anchorWeightsV0"],
        "anchorWeightsV0",
    );
    assert_weights_match(
        &ANCHOR_WEIGHTS_V1,
        &fx["anchorWeightsV1"],
        "anchorWeightsV1",
    );
}

/// The ladder is the promotion contract; shortening or reordering it would
/// weaken every future strength claim without any test failing.
#[test]
fn the_anchor_ladder_matches_typescript() {
    let fx = fixture("weights.json");
    let ts: Vec<&str> = fx["anchorLadder"]
        .as_array()
        .expect("anchorLadder")
        .iter()
        .map(|v| v.as_str().expect("anchor name"))
        .collect();
    assert_eq!(ANCHOR_LADDER.to_vec(), ts);
}

/// `anchor-mcts2-v2`'s configuration is pinned value by value in TS so later
/// default changes cannot move the yardstick. The search agent lands in R5;
/// freezing the constant now keeps the two from drifting apart meanwhile.
#[test]
fn the_pinned_mcts2_anchor_config_matches_typescript() {
    let fx = fixture("weights.json");
    let ts = &fx["anchorMcts2V2Config"];
    let c = ANCHOR_MCTS2_V2_CONFIG;
    assert_eq!(c.iterations as f64, f(ts, "iterations"));
    assert_eq!(c.c_puct, f(ts, "cPuct"));
    assert_eq!(c.max_actions as f64, f(ts, "maxActions"));
    assert_eq!(c.worlds as f64, f(ts, "worlds"));
    assert_eq!(c.prior_temp, f(ts, "priorTemp"));
    assert_eq!(c.battle_mass, f(ts, "battleMass"));
    assert_eq!(c.max_depth as f64, f(ts, "maxDepth"));
    assert_eq!(c.priors, ts["priors"].as_bool().expect("priors"));
    assert_eq!(
        c.rollout_leaf,
        ts["rolloutLeaf"].as_bool().expect("rolloutLeaf")
    );
}

/// One outcome's totals, as the key identifying it across languages.
type Totals = (u8, u8, u8, u8, u8, u8);

fn ts_totals(t: &Value) -> Totals {
    (
        t["selfHits"].as_u64().unwrap() as u8,
        t["intercept"].as_u64().unwrap() as u8,
        t["hits"].as_u64().unwrap() as u8,
        t["buildingHits"].as_u64().unwrap() as u8,
        t["keys"].as_u64().unwrap() as u8,
        t["skirmishBlanks"].as_u64().unwrap() as u8,
    )
}

/// The exact battle convolution, against TS for 80 pool shapes.
///
/// A distribution is a map from totals to probability, so that is how it is
/// compared: matching by index would fail on nothing worse than two equally
/// probable outcomes sorting differently, which is what a first attempt at
/// this test did. Ordering is checked separately, as its own property.
///
/// The fixture carries every outcome for pools with at most 400 of them and the
/// 12 most probable ones otherwise, plus each pool's outcome count and total
/// mass — the full distributions of all 80 pools occupy 62 MB.
#[test]
fn battle_distributions_match_typescript() {
    let fx = fixture("battle_distribution.json");
    let pools = fx.as_array().expect("an array of pools");
    assert!(pools.len() >= 80, "the fixture should cover many pools");

    let mut checked_in_full = 0;
    for entry in pools {
        let pool = entry["pool"].as_array().expect("pool");
        let (a, s, r) = (
            pool[0].as_u64().unwrap() as u8,
            pool[1].as_u64().unwrap() as u8,
            pool[2].as_u64().unwrap() as u8,
        );
        let what = format!("pool {a}a/{s}s/{r}r");
        let dist = battle_distribution(a, s, r);

        assert_eq!(
            dist.len(),
            entry["n"].as_u64().unwrap() as usize,
            "{what}: distinct outcome count disagrees"
        );

        let sum: f64 = dist.iter().map(|o| o.p).sum();
        assert!(
            (sum - f(entry, "sum")).abs() < EXACT && (sum - 1.0).abs() < EXACT,
            "{what}: probabilities sum to {sum}, not 1"
        );

        // Most probable first — the property the index comparison assumed.
        for pair in dist.windows(2) {
            assert!(
                pair[0].p >= pair[1].p,
                "{what}: distribution is not sorted by probability"
            );
        }

        let by_totals: std::collections::HashMap<Totals, f64> = dist
            .iter()
            .map(|o| {
                (
                    (
                        o.totals.self_hits,
                        o.totals.intercept,
                        o.totals.hits,
                        o.totals.building_hits,
                        o.totals.keys,
                        o.totals.skirmish_blanks,
                    ),
                    o.p,
                )
            })
            .collect();
        assert_eq!(by_totals.len(), dist.len(), "{what}: duplicate outcomes");

        let full = entry["full"].as_array();
        if full.is_some() {
            checked_in_full += 1;
        }
        for want in full.unwrap_or_else(|| entry["top"].as_array().expect("top")) {
            let key = ts_totals(&want["t"]);
            let got = by_totals.get(&key).unwrap_or_else(|| {
                panic!("{what}: TypeScript outcome {key:?} is missing from the Rust distribution")
            });
            assert!(
                (got - f(want, "p")).abs() < EXACT,
                "{what}: outcome {key:?} probability {got} != {}",
                f(want, "p")
            );
        }
    }
    assert!(
        checked_in_full >= 38,
        "at least some pools should be compared outcome for outcome, got {checked_in_full}"
    );
}
