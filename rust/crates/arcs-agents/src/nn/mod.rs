//! Neural-network scaffolding: the state encoder and a dependency-free MLP.
//! Ported from `src/agents/nn/`.
//!
//! This is the substrate for the planned TD(λ) afterstate net, not an agent:
//! R5 lands the pieces and their tests so the training loop (R7, in Python
//! over the PyO3 bindings) has something to call. The two halves are
//! deliberately independent — [`features`] turns a position into a fixed
//! vector, [`mlp`] turns a fixed vector into a value — so either can be
//! replaced without touching the other.

pub mod features;
pub mod mlp;

pub use features::{FEATURE_SIZE, FeatureVec, extract_features};
pub use mlp::{Mlp, MlpShape, Trace};
