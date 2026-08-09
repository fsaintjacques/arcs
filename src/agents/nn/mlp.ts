/**
 * A dependency-free multilayer perceptron, sized for this repo's needs:
 * a few hundred inputs, one or two hidden layers, trained by TD(λ) in a
 * node script and evaluated tens of thousands of times per decision inside
 * a search. Flat Float32Array storage, hand-rolled loops, no allocation in
 * the forward path when the caller reuses buffers.
 *
 * Weights serialize to plain number[][] so trained generations can ship as
 * generated TypeScript modules — no JSON loader configuration, and the
 * zero-runtime-dependency rule stays trivially true.
 */
import { mulberry32 } from '../../engine';

export interface MlpShape {
  inputs: number;
  hidden: number[];
  outputs: number;
}

/** Activations kept alive for one backward pass. */
export interface Trace {
  input: Float32Array;
  /** Post-ReLU activations per hidden layer, then the raw output. */
  layers: Float32Array[];
  output: Float32Array;
}

export class Mlp {
  readonly shape: MlpShape;
  /** Per layer: weights row-major [out × in], then biases [out]. */
  private weights: Float32Array[];
  private biases: Float32Array[];
  private gradW: Float32Array[];
  private gradB: Float32Array[];
  /** Scratch buffers so forward() allocates nothing. */
  private scratch: Float32Array[];

  constructor(shape: MlpShape, init: number[][] | 'he' = 'he', seed = 1) {
    this.shape = shape;
    const sizes = [shape.inputs, ...shape.hidden, shape.outputs];
    this.weights = [];
    this.biases = [];
    this.gradW = [];
    this.gradB = [];
    this.scratch = [];

    const rng = mulberry32(seed);
    const gauss = () => {
      const u = Math.max(rng(), 1e-12);
      return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * rng());
    };

    for (let l = 0; l < sizes.length - 1; l++) {
      const nIn = sizes[l];
      const nOut = sizes[l + 1];
      const w = new Float32Array(nOut * nIn);
      const b = new Float32Array(nOut);
      if (init === 'he') {
        const scale = Math.sqrt(2 / nIn);
        for (let i = 0; i < w.length; i++) w[i] = gauss() * scale;
      } else {
        w.set(init[l * 2]);
        b.set(init[l * 2 + 1]);
      }
      this.weights.push(w);
      this.biases.push(b);
      this.gradW.push(new Float32Array(nOut * nIn));
      this.gradB.push(new Float32Array(nOut));
      this.scratch.push(new Float32Array(nOut));
    }
  }

  /**
   * ReLU hidden layers, tanh output. `out` may be provided to avoid the
   * allocation; it must be `shape.outputs` long.
   */
  forward(x: Float32Array, out?: Float32Array): Float32Array {
    let cur: Float32Array = x;
    const last = this.weights.length - 1;
    for (let l = 0; l <= last; l++) {
      const w = this.weights[l];
      const b = this.biases[l];
      const nOut = b.length;
      const nIn = cur.length;
      const dst = l === last && out ? out : this.scratch[l];
      for (let o = 0; o < nOut; o++) {
        let sum = b[o];
        const row = o * nIn;
        for (let i = 0; i < nIn; i++) sum += w[row + i] * cur[i];
        dst[o] = l === last ? Math.tanh(sum) : sum > 0 ? sum : 0;
      }
      cur = dst;
    }
    return cur;
  }

  /** Forward pass that keeps activations for `backward`. Allocates. */
  forwardTrace(x: Float32Array): Trace {
    const layers: Float32Array[] = [];
    let cur = x;
    const last = this.weights.length - 1;
    for (let l = 0; l <= last; l++) {
      const w = this.weights[l];
      const b = this.biases[l];
      const nOut = b.length;
      const nIn = cur.length;
      const dst = new Float32Array(nOut);
      for (let o = 0; o < nOut; o++) {
        let sum = b[o];
        const row = o * nIn;
        for (let i = 0; i < nIn; i++) sum += w[row + i] * cur[i];
        dst[o] = l === last ? Math.tanh(sum) : sum > 0 ? sum : 0;
      }
      layers.push(dst);
      cur = dst;
    }
    return { input: x, layers, output: layers[layers.length - 1] };
  }

  /**
   * Accumulate gradients for one trace. `gradOut` is dLoss/dOutput at the
   * tanh output (the caller owns the loss; for TD, target − prediction
   * negated as appropriate).
   */
  backward(trace: Trace, gradOut: Float32Array): void {
    const last = this.weights.length - 1;
    // d/dz through tanh at the output layer.
    let delta = new Float32Array(trace.output.length);
    for (let o = 0; o < delta.length; o++) {
      const y = trace.output[o];
      delta[o] = gradOut[o] * (1 - y * y);
    }

    for (let l = last; l >= 0; l--) {
      const below = l === 0 ? trace.input : trace.layers[l - 1];
      const w = this.weights[l];
      const gw = this.gradW[l];
      const gb = this.gradB[l];
      const nIn = below.length;
      for (let o = 0; o < delta.length; o++) {
        const row = o * nIn;
        const d = delta[o];
        gb[o] += d;
        for (let i = 0; i < nIn; i++) gw[row + i] += d * below[i];
      }
      if (l > 0) {
        const next = new Float32Array(nIn);
        for (let i = 0; i < nIn; i++) {
          // ReLU gate on the layer below.
          if (below[i] <= 0) continue;
          let sum = 0;
          for (let o = 0; o < delta.length; o++) sum += w[o * nIn + i] * delta[o];
          next[i] = sum;
        }
        delta = next;
      }
    }
  }

  /** SGD step over the accumulated gradients, then zero them. */
  step(lr: number, l2 = 0): void {
    for (let l = 0; l < this.weights.length; l++) {
      const w = this.weights[l];
      const gw = this.gradW[l];
      for (let i = 0; i < w.length; i++) {
        w[i] -= lr * (gw[i] + l2 * w[i]);
        gw[i] = 0;
      }
      const b = this.biases[l];
      const gb = this.gradB[l];
      for (let i = 0; i < b.length; i++) {
        b[i] -= lr * gb[i];
        gb[i] = 0;
      }
    }
  }

  /** Accumulated gradients in toArrays() layout — for gradient checking. */
  gradArrays(): number[][] {
    const out: number[][] = [];
    for (let l = 0; l < this.gradW.length; l++) {
      out.push(Array.from(this.gradW[l]));
      out.push(Array.from(this.gradB[l]));
    }
    return out;
  }

  /** [w0, b0, w1, b1, ...] as plain arrays, for generated weight modules. */
  toArrays(): number[][] {
    const out: number[][] = [];
    for (let l = 0; l < this.weights.length; l++) {
      out.push(Array.from(this.weights[l]));
      out.push(Array.from(this.biases[l]));
    }
    return out;
  }

  static fromArrays(shape: MlpShape, arrays: number[][]): Mlp {
    return new Mlp(shape, arrays);
  }
}
