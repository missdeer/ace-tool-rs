//! Runtime metrics collection with EWMA smoothing

use std::collections::VecDeque;
use tracing::debug;

/// Error types for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    Timeout,
    RateLimit,
    ServerError,
    ClientError,
    NetworkError,
}

/// Outcome of a single HTTP request
#[derive(Debug, Clone)]
pub struct RequestOutcome {
    pub success: bool,
    pub latency_ms: u64,
    pub error_type: Option<ErrorType>,
}

/// Runtime metrics with EWMA smoothing
pub struct RuntimeMetrics {
    /// EWMA smoothing factor (0.0-1.0, higher = more responsive to recent values)
    alpha: f64,
    /// Current EWMA latency in milliseconds
    ewma_latency_ms: f64,
    /// Fixed baseline for latency comparison (from initial heuristic)
    baseline_latency_ms: f64,
    /// Recent request outcomes for success rate calculation
    outcomes: VecDeque<RequestOutcome>,
    /// Maximum window size for outcomes
    window_size: usize,
    /// Count of requests since last adjustment
    requests_since_adjustment: usize,
    /// Count of 429 errors in current window
    rate_limit_count: usize,
    /// Whether metrics have been initialized with first sample
    initialized: bool,
}

impl RuntimeMetrics {
    /// Create new metrics with given baseline and EWMA alpha
    pub fn new(baseline_timeout_ms: u64, alpha: f64, window_size: usize) -> Self {
        let baseline_latency_ms = (baseline_timeout_ms as f64 * 0.3).max(1.0);
        Self {
            alpha,
            ewma_latency_ms: baseline_latency_ms,
            baseline_latency_ms,
            outcomes: VecDeque::with_capacity(window_size),
            window_size,
            requests_since_adjustment: 0,
            rate_limit_count: 0,
            initialized: false,
        }
    }

    /// Record a request outcome
    pub fn record(&mut self, outcome: RequestOutcome) {
        if let Some(ErrorType::ServerError) = outcome.error_type {
            debug!(
                "Excluding 5xx error from metrics (latency={}ms)",
                outcome.latency_ms
            );
            return;
        }

        if outcome.success || outcome.error_type.is_some() {
            self.update_ewma(outcome.latency_ms);
        }

        if matches!(outcome.error_type, Some(ErrorType::RateLimit)) {
            self.rate_limit_count += 1;
        }

        if self.outcomes.len() >= self.window_size {
            let removed = self.outcomes.pop_front();
            if let Some(ref o) = removed {
                if matches!(o.error_type, Some(ErrorType::RateLimit)) {
                    self.rate_limit_count = self.rate_limit_count.saturating_sub(1);
                }
            }
        }

        self.outcomes.push_back(outcome);
        self.requests_since_adjustment += 1;
    }

    /// Update EWMA with new latency sample
    fn update_ewma(&mut self, latency_ms: u64) {
        let latency = latency_ms as f64;
        if !self.initialized {
            self.ewma_latency_ms = latency;
            self.initialized = true;
        } else {
            self.ewma_latency_ms = self.alpha * latency + (1.0 - self.alpha) * self.ewma_latency_ms;
        }
    }

    /// Get current EWMA latency
    pub fn ewma_latency_ms(&self) -> f64 {
        self.ewma_latency_ms
    }

    /// Get baseline latency
    pub fn baseline_latency_ms(&self) -> f64 {
        self.baseline_latency_ms
    }

    /// Calculate success rate (excluding 5xx errors which are filtered in record())
    pub fn success_rate(&self) -> f64 {
        if self.outcomes.is_empty() {
            return 1.0;
        }
        let success_count = self.outcomes.iter().filter(|o| o.success).count();
        success_count as f64 / self.outcomes.len() as f64
    }

    /// Check if we have enough samples for reliable metrics
    pub fn has_minimum_samples(&self, min_samples: usize) -> bool {
        self.outcomes.len() >= min_samples
    }

    /// Get sample count
    pub fn sample_count(&self) -> usize {
        self.outcomes.len()
    }

    /// Get requests since last adjustment
    pub fn requests_since_adjustment(&self) -> usize {
        self.requests_since_adjustment
    }

    /// Reset the adjustment counter (called after strategy adjustment)
    pub fn reset_adjustment_counter(&mut self) {
        self.requests_since_adjustment = 0;
    }

    /// Check if any rate limit errors occurred in the window
    pub fn has_rate_limit_errors(&self) -> bool {
        self.rate_limit_count > 0
    }

    /// Latency health status relative to baseline
    pub fn latency_health(&self) -> LatencyHealth {
        let ratio = self.ewma_latency_ms / self.baseline_latency_ms;
        if ratio <= 0.8 {
            LatencyHealth::Healthy
        } else if ratio <= 1.5 {
            LatencyHealth::Normal
        } else {
            LatencyHealth::High
        }
    }
}

/// Latency health classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatencyHealth {
    Healthy,
    Normal,
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ewma_initial() {
        let mut metrics = RuntimeMetrics::new(30000, 0.2, 20);
        assert!(!metrics.initialized);

        metrics.record(RequestOutcome {
            success: true,
            latency_ms: 1000,
            error_type: None,
        });

        assert!(metrics.initialized);
        assert!((metrics.ewma_latency_ms() - 1000.0).abs() < 0.01);
    }

    #[test]
    fn test_ewma_smoothing() {
        let mut metrics = RuntimeMetrics::new(30000, 0.2, 20);

        metrics.record(RequestOutcome {
            success: true,
            latency_ms: 1000,
            error_type: None,
        });
        metrics.record(RequestOutcome {
            success: true,
            latency_ms: 2000,
            error_type: None,
        });

        let expected = 0.2 * 2000.0 + 0.8 * 1000.0;
        assert!((metrics.ewma_latency_ms() - expected).abs() < 0.01);
    }

    #[test]
    fn test_success_rate() {
        let mut metrics = RuntimeMetrics::new(30000, 0.2, 20);

        for _ in 0..8 {
            metrics.record(RequestOutcome {
                success: true,
                latency_ms: 100,
                error_type: None,
            });
        }
        for _ in 0..2 {
            metrics.record(RequestOutcome {
                success: false,
                latency_ms: 100,
                error_type: Some(ErrorType::Timeout),
            });
        }

        assert!((metrics.success_rate() - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_5xx_excluded() {
        let mut metrics = RuntimeMetrics::new(30000, 0.2, 20);

        metrics.record(RequestOutcome {
            success: false,
            latency_ms: 100,
            error_type: Some(ErrorType::ServerError),
        });

        assert_eq!(metrics.sample_count(), 0);
    }

    #[test]
    fn test_latency_health() {
        let mut metrics = RuntimeMetrics::new(30000, 0.2, 20);

        metrics.record(RequestOutcome {
            success: true,
            latency_ms: 5000,
            error_type: None,
        });
        assert_eq!(metrics.latency_health(), LatencyHealth::Healthy);

        let mut metrics2 = RuntimeMetrics::new(30000, 0.2, 20);
        metrics2.record(RequestOutcome {
            success: true,
            latency_ms: 9000,
            error_type: None,
        });
        assert_eq!(metrics2.latency_health(), LatencyHealth::Normal);

        let mut metrics3 = RuntimeMetrics::new(30000, 0.2, 20);
        metrics3.record(RequestOutcome {
            success: true,
            latency_ms: 20000,
            error_type: None,
        });
        assert_eq!(metrics3.latency_health(), LatencyHealth::High);
    }

    #[test]
    fn test_rate_limit_tracking() {
        let mut metrics = RuntimeMetrics::new(30000, 0.2, 20);

        assert!(!metrics.has_rate_limit_errors());

        metrics.record(RequestOutcome {
            success: false,
            latency_ms: 100,
            error_type: Some(ErrorType::RateLimit),
        });

        assert!(metrics.has_rate_limit_errors());
    }
}
