/** The MLP's gradients are real, its weights round-trip, and it's fast. */
import { describe, expect, it } from 'vitest';
import { cloneState, getPending, makeVariant, mulberry32 } from '../src/engine';
import { defaultWeights, makeAgent, relativeEvaluate } from '../src/agents';
import { extractFeatures, FEATURE_SIZE } from '../src/agents/nn/features';
import { Mlp } from '../src/agents/nn/mlp';
import { playGame } from '../src/sim/runner';

describe('Mlp', () => {
  it('backward matches finite differences', () => {
    const shape = { inputs: 6, hidden: [5], outputs: 1 };
    const net = new Mlp(shape, 'he', 7);
    const rng = mulberry32(11);
    const x = Float32Array.from({ length: 6 }, () => rng() * 2 - 1);
    const target = 0.3;

    const loss = (m: Mlp) => {
      const y = m.forward(x)[0];
      return 0.5 * (y - target) ** 2;
    };

    const trace = net.forwardTrace(x);
    net.backward(trace, Float32Array.of(trace.output[0] - target));
    const grads = net.gradArrays();
    const arrays = net.toArrays();

    const eps = 1e-3;
    let checked = 0;
    for (let layer = 0; layer < arrays.length; layer++) {
      for (const idx of [0, Math.floor(arrays[layer].length / 2)]) {
        const bumped = arrays.map((a) => a.slice());
        bumped[layer][idx] += eps;
        const up = loss(Mlp.fromArrays(shape, bumped));
        bumped[layer][idx] -= 2 * eps;
        const down = loss(Mlp.fromArrays(shape, bumped));
        const numeric = (up - down) / (2 * eps);
        expect(grads[layer][idx]).toBeCloseTo(numeric, 3);
        checked++;
      }
    }
    expect(checked).toBeGreaterThanOrEqual(8);
  });

  it('weights round-trip through plain arrays exactly', () => {
    const shape = { inputs: 8, hidden: [6, 4], outputs: 2 };
    const a = new Mlp(shape, 'he', 3);
    const b = Mlp.fromArrays(shape, a.toArrays());
    const x = Float32Array.from({ length: 8 }, (_, i) => (i - 4) / 4);
    expect(Array.from(b.forward(x))).toEqual(Array.from(a.forward(x)));
  });

  it('overfits a small labelled set — the training loop works end to end', () => {
    // Labels come from the heuristic evaluation on real positions, squashed
    // into tanh range: if SGD cannot fit 60 of these, nothing downstream can.
    const samples: { x: Float32Array; y: number }[] = [];
    const v = makeVariant(3, 1);
    playGame(Array.from({ length: 3 }, () => makeAgent('random+')), {
      players: 3,
      seed: 41,
      setupIndex: 1,
      onDecision: (state, player) => {
        if (samples.length >= 60 || getPending(state, v).kind !== 'decision') return;
        const x = new Float32Array(FEATURE_SIZE);
        extractFeatures(state, v, player, x);
        samples.push({ x, y: Math.tanh(relativeEvaluate(state, v, player, defaultWeights) / 20) });
      },
    });
    expect(samples.length).toBe(60);

    const net = new Mlp({ inputs: FEATURE_SIZE, hidden: [16], outputs: 1 }, 'he', 5);
    const mse = () =>
      samples.reduce((sum, s) => sum + (net.forward(s.x)[0] - s.y) ** 2, 0) / samples.length;
    const before = mse();
    for (let epoch = 0; epoch < 150; epoch++) {
      for (const s of samples) {
        const trace = net.forwardTrace(s.x);
        net.backward(trace, Float32Array.of(trace.output[0] - s.y));
        net.step(0.02);
      }
    }
    const after = mse();
    expect(after).toBeLessThan(before / 10);
    expect(after).toBeLessThan(0.01);
  });
});

describe('extractFeatures', () => {
  it('is finite, bounded and stable across a full game', () => {
    const v = makeVariant(3, 2);
    const x = new Float32Array(FEATURE_SIZE);
    let nodes = 0;
    playGame(Array.from({ length: 3 }, () => makeAgent('random+')), {
      players: 3,
      seed: 42,
      setupIndex: 2,
      onDecision: (state, player) => {
        nodes++;
        extractFeatures(state, v, player, x);
        for (let i = 0; i < x.length; i++) {
          expect(Number.isFinite(x[i])).toBe(true);
          expect(x[i]).toBeGreaterThanOrEqual(-0.001);
          expect(x[i]).toBeLessThanOrEqual(3);
        }
      },
    });
    expect(nodes).toBeGreaterThan(100);
  });

  it('extract + forward stays comfortably inside the leaf budget', () => {
    const v = makeVariant(3, 1);
    const net = new Mlp({ inputs: FEATURE_SIZE, hidden: [128], outputs: 1 }, 'he', 9);
    const x = new Float32Array(FEATURE_SIZE);
    const states: { s: Parameters<typeof extractFeatures>[0]; p: number }[] = [];
    playGame(Array.from({ length: 3 }, () => makeAgent('random+')), {
      players: 3,
      seed: 43,
      setupIndex: 1,
      onDecision: (state, player) => {
        // onDecision hands the live, mutating state — keep copies.
        if (states.length < 50) states.push({ s: cloneState(state), p: player });
      },
    });

    // Warm up, then time.
    for (const { s, p } of states) {
      extractFeatures(s, v, p, x);
      net.forward(x);
    }
    const t0 = performance.now();
    const reps = 40;
    for (let r = 0; r < reps; r++) {
      for (const { s, p } of states) {
        extractFeatures(s, v, p, x);
        net.forward(x);
      }
    }
    const usPerCall = ((performance.now() - t0) / (reps * states.length)) * 1000;
    console.log(`extract+forward: ${usPerCall.toFixed(1)} µs at ${FEATURE_SIZE}×128×1`);
    // Generous CI bound; the real number is recorded in FINDINGS.
    expect(usPerCall).toBeLessThan(200);
  });
});
