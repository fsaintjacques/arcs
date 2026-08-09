//! Clone/apply micro-benchmarks (plan §6, R1 verification): a plain timing
//! loop, no deps, ignored by default. Run with
//!
//! ```sh
//! cargo test --release --test bench -- --ignored --nocapture
//! ```
//!
//! Records: `GameState` memcpy throughput (the operation that replaces the
//! TS `cloneState` hot path), `apply_action_mut` throughput on a fixed
//! trick cycle, and full random-game throughput — complete games (board
//! actions, battles, scoring) as of R2.

use arcs_engine::game::{apply_action_mut, get_pending, legal_actions, resolve_chance_mut};
use arcs_engine::{Action, GameState, Pending, Rng, SetupMode, SplitMix64, make_variant, new_game};
use std::hint::black_box;
use std::time::Instant;

fn per_op(label: &str, ops: u64, elapsed: std::time::Duration) {
    let ns = elapsed.as_nanos() as f64 / ops as f64;
    let rate = ops as f64 / elapsed.as_secs_f64();
    println!("{label:<28} {ns:>10.1} ns/op  {rate:>12.0} ops/s  ({ops} ops)");
}

#[test]
#[ignore = "benchmark: run with --release -- --ignored --nocapture"]
fn clone_and_apply_throughput() {
    println!("GameState size: {} bytes", size_of::<GameState>());
    println!("Action size:    {} bytes", size_of::<Action>());

    let v = make_variant(3, 1, SetupMode::Deck);
    let mut rng = SplitMix64::new(42);
    let mut s = new_game(&v, &mut rng, 1, SetupMode::Deck);
    resolve_chance_mut(&mut s, &v, &mut rng).unwrap();

    // --- clone (memcpy) ---
    let ops = 2_000_000u64;
    let t = Instant::now();
    let mut acc = 0u64;
    for _ in 0..ops {
        let copy = black_box(s);
        acc = acc.wrapping_add(copy.chapter as u64);
    }
    black_box(acc);
    per_op("clone (memcpy)", ops, t.elapsed());

    // --- apply_action_mut on a fixed lead -> begin -> endTurn cycle ---
    // Each iteration re-copies the dealt state (cost measured above) and
    // applies 3 actions, so the loop never runs out of cards.
    let base = s;
    let lead = base.player_states[get_pending_player(&base, &v).as_index()]
        .hand
        .as_slice()[0];
    let iters = 300_000u64;
    let t = Instant::now();
    for _ in 0..iters {
        let mut w = black_box(base);
        apply_action_mut(&mut w, &v, Action::Lead { card: lead }).unwrap();
        apply_action_mut(&mut w, &v, Action::BeginActions).unwrap();
        apply_action_mut(&mut w, &v, Action::EndTurn).unwrap();
        black_box(&w);
    }
    per_op("apply (copy+3 actions)", iters * 3, t.elapsed());

    // --- full random games (complete rules as of R2) ---
    let games = 3_000u64;
    let mut decisions = 0u64;
    let mut acts = Vec::new();
    let t = Instant::now();
    for seed in 0..games {
        let v = make_variant(3, seed % 8, SetupMode::Deck);
        let mut rng = SplitMix64::new(seed);
        let mut g = new_game(&v, &mut rng, seed % 8, SetupMode::Deck);
        loop {
            match get_pending(&g, &v) {
                Pending::Over => break,
                Pending::Chance => resolve_chance_mut(&mut g, &v, &mut rng).unwrap(),
                Pending::Decision { .. } => {
                    legal_actions(&g, &v, &mut acts);
                    let a = acts[rng.gen_range(acts.len())];
                    apply_action_mut(&mut g, &v, a).unwrap();
                    decisions += 1;
                }
            }
        }
        black_box(&g);
    }
    let elapsed = t.elapsed();
    per_op("random-game decision", decisions, elapsed);
    println!(
        "random games: {games} in {:.2}s = {:.0} games/s ({} decisions/game avg)",
        elapsed.as_secs_f64(),
        games as f64 / elapsed.as_secs_f64(),
        decisions / games,
    );
}

fn get_pending_player(s: &GameState, v: &arcs_engine::VariantDef) -> arcs_engine::Player {
    match get_pending(s, v) {
        Pending::Decision { player } => player,
        other => panic!("expected decision, got {other:?}"),
    }
}
