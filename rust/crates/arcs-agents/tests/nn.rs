//! The MLP's gradients are real, its weights round-trip, and the feature
//! extractor is finite and bounded. Ported from `tests/nn.test.ts`.

mod common;

use std::time::Instant;

use arcs_agents::nn::{FEATURE_SIZE, FeatureVec, Mlp, MlpShape, extract_features};
use arcs_agents::{DEFAULT_WEIGHTS, relative_evaluate};
use arcs_engine::{GameState, Player, Rng, SetupMode, SplitMix64, make_variant};

use common::Flow;

// Ported from tests/nn.test.ts "backward matches finite differences".
#[test]
fn backward_matches_finite_differences() {
    let shape = MlpShape::new(6, &[5], 1);
    let mut net = Mlp::new(shape.clone(), 7);
    let mut rng = SplitMix64::new(11);
    let x: Vec<f32> = (0..6)
        .map(|_| (rng.next_f64() * 2.0 - 1.0) as f32)
        .collect();
    let target = 0.3f32;

    let loss = |m: &mut Mlp| {
        let y = m.forward(&x)[0];
        0.5 * (y - target).powi(2)
    };

    let trace = net.forward_trace(&x);
    let grad_out = [trace.output()[0] - target];
    net.backward(&trace, &grad_out);
    let grads = net.grad_arrays();
    let arrays = net.to_arrays();

    // f32 weights make a smaller epsilon lose the difference to rounding; the
    // TS version uses 1e-3 over f64 for the same reason.
    let eps = 1e-3f32;
    let mut checked = 0;
    for layer in 0..arrays.len() {
        for idx in [0, arrays[layer].len() / 2] {
            let mut bumped = arrays.clone();
            bumped[layer][idx] += eps;
            let up = loss(&mut Mlp::from_arrays(shape.clone(), &bumped));
            bumped[layer][idx] -= 2.0 * eps;
            let down = loss(&mut Mlp::from_arrays(shape.clone(), &bumped));
            let numeric = (up - down) / (2.0 * eps);
            assert!(
                (grads[layer][idx] - numeric).abs() < 1e-3,
                "layer {layer}[{idx}]: analytic {} vs numeric {numeric}",
                grads[layer][idx]
            );
            checked += 1;
        }
    }
    assert!(checked >= 8);
}

// Ported from tests/nn.test.ts "weights round-trip through plain arrays
// exactly".
#[test]
fn weights_round_trip_through_plain_arrays_exactly() {
    let shape = MlpShape::new(8, &[6, 4], 2);
    let mut a = Mlp::new(shape.clone(), 3);
    let mut b = Mlp::from_arrays(shape, &a.to_arrays());
    let x: Vec<f32> = (0..8).map(|i| (i as f32 - 4.0) / 4.0).collect();
    assert_eq!(a.forward(&x).to_vec(), b.forward(&x).to_vec());
}

/// Collect `n` (features, label) samples from a seeded `random+` game, the
/// way the TS overfit test does: labels are the heuristic evaluation on real
/// positions, squashed into tanh range.
fn labelled_samples(n: usize, seed: u64, setup_index: u64) -> Vec<(FeatureVec, f32)> {
    let v = make_variant(3, setup_index, SetupMode::Draw);
    let mut samples: Vec<(FeatureVec, f32)> = Vec::with_capacity(n);
    common::play_game(
        &["random+", "random+", "random+"],
        3,
        seed,
        setup_index,
        SetupMode::Draw,
        &mut |state: &GameState, player: Player| {
            if samples.len() >= n {
                return Flow::Stop;
            }
            let mut x = [0.0f32; FEATURE_SIZE];
            extract_features(state, &v, player, &mut x);
            let y = (relative_evaluate(state, &v, player, &DEFAULT_WEIGHTS) / 20.0).tanh() as f32;
            samples.push((x, y));
            Flow::Continue
        },
    );
    samples
}

// Ported from tests/nn.test.ts "overfits a small labelled set — the training
// loop works end to end".
#[test]
fn overfits_a_small_labelled_set() {
    let samples = labelled_samples(60, 41, 1);
    assert_eq!(samples.len(), 60);

    let mut net = Mlp::new(MlpShape::new(FEATURE_SIZE, &[16], 1), 5);
    let mse = |net: &mut Mlp| {
        samples
            .iter()
            .map(|(x, y)| (net.forward(x)[0] - y).powi(2))
            .sum::<f32>()
            / samples.len() as f32
    };
    let before = mse(&mut net);
    for _ in 0..150 {
        for (x, y) in &samples {
            let trace = net.forward_trace(x);
            let grad = [trace.output()[0] - y];
            net.backward(&trace, &grad);
            net.step(0.02, 0.0);
        }
    }
    let after = mse(&mut net);
    assert!(
        after < before / 10.0,
        "MSE only fell from {before} to {after}"
    );
    assert!(after < 0.01, "MSE {after} is not a fit");
}

// Ported from tests/nn.test.ts "is finite, bounded and stable across a full
// game".
#[test]
fn features_are_finite_and_bounded_across_a_full_game() {
    let v = make_variant(3, 2, SetupMode::Draw);
    let mut x = [0.0f32; FEATURE_SIZE];
    let mut nodes = 0usize;
    common::play_game(
        &["random+", "random+", "random+"],
        3,
        42,
        2,
        SetupMode::Draw,
        &mut |state, player| {
            nodes += 1;
            extract_features(state, &v, player, &mut x);
            for (i, &value) in x.iter().enumerate() {
                assert!(value.is_finite(), "feature {i} is not finite");
                assert!(
                    (-0.001..=3.0).contains(&value),
                    "feature {i} is {value}, outside the [0,1]-ish scaling the layout promises"
                );
            }
            Flow::Continue
        },
    );
    assert!(nodes > 100, "only {nodes} decision nodes");
}

/// The extractor writes every slot it owns: a caller reusing one buffer across
/// positions must never see a stale value from the previous position. (TS
/// relies on the same `out.fill(0)`, untested there.)
#[test]
fn extraction_overwrites_the_whole_buffer() {
    let v = make_variant(3, 3, SetupMode::Draw);
    let mut rng = SplitMix64::new(3);
    let s = arcs_engine::new_game(&v, &mut rng, 3, SetupMode::Draw);
    let mut fresh = [0.0f32; FEATURE_SIZE];
    let mut dirty = [7.0f32; FEATURE_SIZE];
    extract_features(&s, &v, Player(0), &mut fresh);
    extract_features(&s, &v, Player(0), &mut dirty);
    assert_eq!(fresh, dirty);
}

/// Seat rotation is the property that lets one net serve every seat: the same
/// position seen by two seats must differ, and "me" must always be block 0.
#[test]
fn features_are_seat_rotated() {
    let v = make_variant(3, 1, SetupMode::Draw);
    let mut captured: Option<GameState> = None;
    common::play_game(
        &["random+", "random+", "random+"],
        3,
        44,
        1,
        SetupMode::Draw,
        &mut |state, _| {
            // Deep enough that the seats have diverged on the board.
            if state.chapter >= 2 {
                captured = Some(*state);
                Flow::Stop
            } else {
                Flow::Continue
            }
        },
    );
    let s = captured.expect("a chapter-2 position");
    let mut a = [0.0f32; FEATURE_SIZE];
    let mut b = [0.0f32; FEATURE_SIZE];
    extract_features(&s, &v, Player(0), &mut a);
    extract_features(&s, &v, Player(1), &mut b);
    assert_ne!(
        a, b,
        "two seats must not see the identical vector, or the rotation is a no-op"
    );
}

/// The feature/forward pair is the leaf cost of any NN-valued search, so it is
/// measured serially and reported rather than asserted — `docs/FINDINGS.md`
/// records timings taken inside saturated workers reporting an agent 90x
/// slower than it was.
///
/// ```text
/// cargo test -p arcs-agents --release --test nn -- --ignored --nocapture
/// ```
#[test]
#[ignore = "benchmark"]
fn extract_and_forward_stay_inside_the_leaf_budget() {
    let v = make_variant(3, 1, SetupMode::Draw);
    let mut net = Mlp::new(MlpShape::new(FEATURE_SIZE, &[128], 1), 9);
    let mut x = [0.0f32; FEATURE_SIZE];
    let mut states: Vec<(GameState, Player)> = Vec::new();
    common::play_game(
        &["random+", "random+", "random+"],
        3,
        43,
        1,
        SetupMode::Draw,
        &mut |state, player| {
            if states.len() < 50 {
                states.push((*state, player));
                Flow::Continue
            } else {
                Flow::Stop
            }
        },
    );

    for (s, p) in &states {
        extract_features(s, &v, *p, &mut x);
        net.forward(&x);
    }
    let reps = 40;
    let start = Instant::now();
    for _ in 0..reps {
        for (s, p) in &states {
            extract_features(s, &v, *p, &mut x);
            net.forward(&x);
        }
    }
    let us = start.elapsed().as_secs_f64() / (reps * states.len()) as f64 * 1e6;
    println!("extract+forward: {us:.2} us at {FEATURE_SIZE}x128x1");
    assert!(us < 200.0, "extract+forward took {us:.1} us");
}
