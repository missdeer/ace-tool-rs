//! Adaptive upload strategy module
//!
//! Implements AIMD (Additive Increase, Multiplicative Decrease) algorithm
//! for dynamic upload parameter adjustment based on runtime metrics.

mod adaptive;
mod metrics;

pub use adaptive::{AdaptiveStrategy, StrategyAdjustment};
pub use metrics::{ErrorType, RequestOutcome, RuntimeMetrics};
