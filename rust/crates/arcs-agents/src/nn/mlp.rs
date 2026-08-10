//! A dependency-free multilayer perceptron, sized for this repo's needs: a
//! few hundred inputs, one or two hidden layers, trained by TD(λ) and
//! evaluated tens of thousands of times per decision inside a search. Ported
//! from `src/agents/nn/mlp.ts`.
//!
//! Flat `f32` storage, hand-rolled loops, and **no allocation in the forward
//! path** — [`Mlp::forward`] writes into internal scratch buffers sized at
//! construction and hands back a borrow of the last one.
//!
//! Weights serialize to and from flat `Vec<f32>` arrays in the layer order
//! `[w0, b0, w1, b1, …]`, so a trained generation can ship as generated source
//! or a plain binary blob with no serialization dependency — the crate's
//! zero-dependency rule stays trivially true. That is the same layout
//! `mlp.ts`'s `toArrays`/`fromArrays` use, so a net trained on either side
//! loads on the other.
//!
//! Initialization uses [`SplitMix64`] rather than the TS `mulberry32`: the
//! port keeps **statistical parity only** (plan §5), and He-initialised
//! weights are exchangeable draws from the same distribution, not a sequence
//! anything should depend on.

use arcs_engine::{Rng, SplitMix64};

/// Layer widths. `hidden` may be empty for a linear model.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MlpShape {
    pub inputs: usize,
    pub hidden: Vec<usize>,
    pub outputs: usize,
}

impl MlpShape {
    pub fn new(inputs: usize, hidden: &[usize], outputs: usize) -> Self {
        MlpShape {
            inputs,
            hidden: hidden.to_vec(),
            outputs,
        }
    }

    /// `[inputs, ...hidden, outputs]`.
    fn sizes(&self) -> Vec<usize> {
        let mut sizes = Vec::with_capacity(self.hidden.len() + 2);
        sizes.push(self.inputs);
        sizes.extend_from_slice(&self.hidden);
        sizes.push(self.outputs);
        sizes
    }
}

/// Activations kept alive for one backward pass.
#[derive(Clone, PartialEq, Debug)]
pub struct Trace {
    pub input: Vec<f32>,
    /// Post-ReLU activations per hidden layer, then the tanh output.
    pub layers: Vec<Vec<f32>>,
}

impl Trace {
    /// The net's output for this trace.
    pub fn output(&self) -> &[f32] {
        self.layers.last().expect("a net has at least one layer")
    }
}

pub struct Mlp {
    shape: MlpShape,
    /// Per layer: weights row-major `[out x in]`.
    weights: Vec<Vec<f32>>,
    biases: Vec<Vec<f32>>,
    grad_w: Vec<Vec<f32>>,
    grad_b: Vec<Vec<f32>>,
    /// Scratch buffers so `forward` allocates nothing.
    scratch: Vec<Vec<f32>>,
    /// `scratch_in[l]` is layer `l`'s input width, cached at construction.
    scratch_in: Vec<usize>,
}

impl Mlp {
    /// He-initialised weights from `seed`.
    pub fn new(shape: MlpShape, seed: u64) -> Self {
        let mut net = Self::empty(shape);
        let mut rng = SplitMix64::new(seed);
        for l in 0..net.weights.len() {
            let scale = (2.0 / net.scratch_in[l] as f64).sqrt();
            for x in net.weights[l].iter_mut() {
                *x = (gauss(&mut rng) * scale) as f32;
            }
        }
        net
    }

    /// Weights straight from the flat `[w0, b0, w1, b1, …]` layout.
    pub fn from_arrays(shape: MlpShape, arrays: &[Vec<f32>]) -> Self {
        let mut net = Self::empty(shape);
        assert_eq!(
            arrays.len(),
            net.weights.len() * 2,
            "expected one weight array and one bias array per layer"
        );
        for l in 0..net.weights.len() {
            assert_eq!(
                arrays[l * 2].len(),
                net.weights[l].len(),
                "layer {l} weights"
            );
            assert_eq!(
                arrays[l * 2 + 1].len(),
                net.biases[l].len(),
                "layer {l} biases"
            );
            net.weights[l].copy_from_slice(&arrays[l * 2]);
            net.biases[l].copy_from_slice(&arrays[l * 2 + 1]);
        }
        net
    }

    pub fn shape(&self) -> &MlpShape {
        &self.shape
    }

    /// ReLU hidden layers, tanh output. Returns a borrow of the internal
    /// output buffer — no allocation.
    pub fn forward(&mut self, x: &[f32]) -> &[f32] {
        debug_assert_eq!(x.len(), self.shape.inputs);
        let last = self.weights.len() - 1;
        for l in 0..=last {
            // Split the scratch stack so the source layer and the destination
            // layer can be borrowed at once.
            let (below, above) = self.scratch.split_at_mut(l);
            let cur: &[f32] = if l == 0 { x } else { &below[l - 1] };
            layer_forward(
                &self.weights[l],
                &self.biases[l],
                cur,
                &mut above[0],
                l == last,
            );
        }
        &self.scratch[last]
    }

    /// Forward pass that keeps activations for [`Mlp::backward`]. Allocates —
    /// it is the training path, not the search path.
    pub fn forward_trace(&self, x: &[f32]) -> Trace {
        debug_assert_eq!(x.len(), self.shape.inputs);
        let last = self.weights.len() - 1;
        let mut layers: Vec<Vec<f32>> = Vec::with_capacity(last + 1);
        for l in 0..=last {
            let cur: &[f32] = if l == 0 { x } else { &layers[l - 1] };
            let mut dst = vec![0.0f32; self.biases[l].len()];
            layer_forward(&self.weights[l], &self.biases[l], cur, &mut dst, l == last);
            layers.push(dst);
        }
        Trace {
            input: x.to_vec(),
            layers,
        }
    }

    /// Accumulate gradients for one trace. `grad_out` is dLoss/dOutput at the
    /// tanh output (the caller owns the loss; for TD, target − prediction
    /// negated as appropriate).
    pub fn backward(&mut self, trace: &Trace, grad_out: &[f32]) {
        let last = self.weights.len() - 1;
        // d/dz through tanh at the output layer.
        let output = trace.output();
        let mut delta: Vec<f32> = output
            .iter()
            .zip(grad_out)
            .map(|(&y, &g)| g * (1.0 - y * y))
            .collect();

        for l in (0..=last).rev() {
            let below: &[f32] = if l == 0 {
                &trace.input
            } else {
                &trace.layers[l - 1]
            };
            let n_in = below.len();
            let w = &self.weights[l];
            let gw = &mut self.grad_w[l];
            let gb = &mut self.grad_b[l];
            for (o, &d) in delta.iter().enumerate() {
                let row = o * n_in;
                gb[o] += d;
                for i in 0..n_in {
                    gw[row + i] += d * below[i];
                }
            }
            if l > 0 {
                let mut next = vec![0.0f32; n_in];
                for (i, slot) in next.iter_mut().enumerate() {
                    // ReLU gate on the layer below.
                    if below[i] <= 0.0 {
                        continue;
                    }
                    let mut sum = 0.0f32;
                    for (o, &d) in delta.iter().enumerate() {
                        sum += w[o * n_in + i] * d;
                    }
                    *slot = sum;
                }
                delta = next;
            }
        }
    }

    /// SGD step over the accumulated gradients, then zero them.
    pub fn step(&mut self, lr: f32, l2: f32) {
        for l in 0..self.weights.len() {
            let w = &mut self.weights[l];
            let gw = &mut self.grad_w[l];
            for (x, g) in w.iter_mut().zip(gw.iter_mut()) {
                *x -= lr * (*g + l2 * *x);
                *g = 0.0;
            }
            let b = &mut self.biases[l];
            let gb = &mut self.grad_b[l];
            for (x, g) in b.iter_mut().zip(gb.iter_mut()) {
                *x -= lr * *g;
                *g = 0.0;
            }
        }
    }

    /// Accumulated gradients in [`Mlp::to_arrays`] layout — for gradient
    /// checking.
    pub fn grad_arrays(&self) -> Vec<Vec<f32>> {
        interleave(&self.grad_w, &self.grad_b)
    }

    /// `[w0, b0, w1, b1, …]`, for generated weight modules and checkpoints.
    pub fn to_arrays(&self) -> Vec<Vec<f32>> {
        interleave(&self.weights, &self.biases)
    }

    /// Zeroed buffers of the right shapes.
    fn empty(shape: MlpShape) -> Self {
        let sizes = shape.sizes();
        let layers = sizes.len() - 1;
        let mut net = Mlp {
            shape,
            weights: Vec::with_capacity(layers),
            biases: Vec::with_capacity(layers),
            grad_w: Vec::with_capacity(layers),
            grad_b: Vec::with_capacity(layers),
            scratch: Vec::with_capacity(layers),
            scratch_in: Vec::with_capacity(layers),
        };
        for l in 0..layers {
            let (n_in, n_out) = (sizes[l], sizes[l + 1]);
            net.weights.push(vec![0.0; n_out * n_in]);
            net.biases.push(vec![0.0; n_out]);
            net.grad_w.push(vec![0.0; n_out * n_in]);
            net.grad_b.push(vec![0.0; n_out]);
            net.scratch.push(vec![0.0; n_out]);
            net.scratch_in.push(n_in);
        }
        net
    }
}

/// One layer: `dst = act(W · cur + b)`, ReLU except at the output, which is
/// tanh.
#[inline]
fn layer_forward(w: &[f32], b: &[f32], cur: &[f32], dst: &mut [f32], is_output: bool) {
    let n_in = cur.len();
    for (o, &bias) in b.iter().enumerate() {
        let row = &w[o * n_in..(o + 1) * n_in];
        let mut sum = bias;
        for (x, y) in row.iter().zip(cur) {
            sum += x * y;
        }
        dst[o] = if is_output {
            sum.tanh()
        } else if sum > 0.0 {
            sum
        } else {
            0.0
        };
    }
}

fn interleave(w: &[Vec<f32>], b: &[Vec<f32>]) -> Vec<Vec<f32>> {
    let mut out = Vec::with_capacity(w.len() * 2);
    for (wl, bl) in w.iter().zip(b) {
        out.push(wl.clone());
        out.push(bl.clone());
    }
    out
}

/// Box-Muller, matching the TS `gauss()` helper.
fn gauss(rng: &mut SplitMix64) -> f64 {
    let u = rng.next_f64().max(1e-12);
    (-2.0 * u.ln()).sqrt() * (2.0 * core::f64::consts::PI * rng.next_f64()).cos()
}
