//! AIMD (Additive Increase, Multiplicative Decrease) adaptive strategy

use crate::config::{get_upload_strategy, CliOverrides, UploadStrategy};
use crate::strategy::metrics::{ErrorType, LatencyHealth, RequestOutcome, RuntimeMetrics};
use tracing::info;

/// Minimum concurrency allowed
const MIN_CONCURRENCY: usize = 1;
/// Maximum concurrency allowed
const MAX_CONCURRENCY: usize = 8;
/// Minimum timeout in milliseconds
const MIN_TIMEOUT_MS: u64 = 15_000;
/// Maximum timeout in milliseconds
const MAX_TIMEOUT_MS: u64 = 180_000;
/// Minimum samples before making adjustments
const MIN_SAMPLES: usize = 20;
/// Cooldown period (requests between adjustments)
const COOLDOWN_REQUESTS: usize = 5;
/// Success rate threshold for downgrade
const DOWNGRADE_SUCCESS_THRESHOLD: f64 = 0.70;
/// Success rate threshold for upgrade
const UPGRADE_SUCCESS_THRESHOLD: f64 = 0.95;
/// Warmup request count
const WARMUP_REQUESTS: usize = 5;
/// Warmup success threshold
const WARMUP_SUCCESS_THRESHOLD: f64 = 0.90;
/// Maximum warmup requests before forced exit
const MAX_WARMUP_REQUESTS: usize = 10;
/// EWMA alpha for smoothing
const EWMA_ALPHA: f64 = 0.2;
/// Metrics window size
const METRICS_WINDOW_SIZE: usize = 20;

/// Strategy adjustment direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyAdjustment {
    Upgrade,
    Downgrade,
    NoChange,
}

/// Warmup phase state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WarmupState {
    Active,
    Completed,
}

/// Adaptive upload strategy manager
pub struct AdaptiveStrategy {
    /// Current concurrency
    concurrency: usize,
    /// Current timeout in milliseconds
    timeout_ms: u64,
    /// Target concurrency (from heuristic)
    target_concurrency: usize,
    /// Target timeout (from heuristic)
    target_timeout_ms: u64,
    /// Batch size (from heuristic, not adaptive)
    batch_size: usize,
    /// Runtime metrics
    metrics: RuntimeMetrics,
    /// Whether adaptive mode is enabled
    adaptive_enabled: bool,
    /// CLI overrides
    cli_overrides: CliOverrides,
    /// Warmup state
    warmup_state: WarmupState,
    /// Warmup request count
    warmup_request_count: usize,
}

impl AdaptiveStrategy {
    /// Create a new adaptive strategy based on blob count and CLI overrides
    pub fn new(blob_count: usize, cli_overrides: CliOverrides, adaptive_enabled: bool) -> Self {
        let heuristic = get_upload_strategy(blob_count);

        let target_concurrency = cli_overrides
            .upload_concurrency
            .unwrap_or(heuristic.concurrency)
            .max(MIN_CONCURRENCY);

        let target_timeout_ms = cli_overrides
            .upload_timeout_secs
            .map(|s| s * 1000)
            .unwrap_or(heuristic.timeout_ms);

        let initial_concurrency = if adaptive_enabled && cli_overrides.upload_concurrency.is_none()
        {
            MIN_CONCURRENCY
        } else {
            target_concurrency
        };

        let initial_timeout_ms = cli_overrides
            .upload_timeout_secs
            .map(|s| s * 1000)
            .unwrap_or(target_timeout_ms);

        let metrics = RuntimeMetrics::new(target_timeout_ms, EWMA_ALPHA, METRICS_WINDOW_SIZE);

        info!(
            "Strategy initialized: concurrency={}, timeout={}s, adaptive={}, warmup={}",
            initial_concurrency,
            initial_timeout_ms / 1000,
            adaptive_enabled,
            adaptive_enabled && cli_overrides.upload_concurrency.is_none()
        );

        Self {
            concurrency: initial_concurrency,
            timeout_ms: initial_timeout_ms,
            target_concurrency,
            target_timeout_ms,
            batch_size: heuristic.batch_size,
            metrics,
            adaptive_enabled,
            cli_overrides: cli_overrides.clone(),
            warmup_state: if adaptive_enabled && cli_overrides.upload_concurrency.is_none() {
                WarmupState::Active
            } else {
                WarmupState::Completed
            },
            warmup_request_count: 0,
        }
    }

    /// Get current upload strategy
    pub fn current_strategy(&self) -> UploadStrategy {
        UploadStrategy {
            batch_size: self.batch_size,
            concurrency: self.concurrency,
            timeout_ms: self.timeout_ms,
            scale_name: "",
        }
    }

    /// Get current concurrency
    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// Get current timeout in milliseconds
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Get batch size
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Record a request outcome and potentially adjust strategy
    pub fn record_outcome(
        &mut self,
        success: bool,
        latency_ms: u64,
        error_type: Option<ErrorType>,
    ) -> StrategyAdjustment {
        let outcome = RequestOutcome {
            success,
            latency_ms,
            error_type,
        };
        self.metrics.record(outcome);

        if self.warmup_state == WarmupState::Active {
            self.warmup_request_count += 1;
            return self.check_warmup_exit();
        }

        if !self.adaptive_enabled {
            return StrategyAdjustment::NoChange;
        }

        if self.cli_overrides.upload_concurrency.is_some()
            && self.cli_overrides.upload_timeout_secs.is_some()
        {
            return StrategyAdjustment::NoChange;
        }

        self.evaluate_adjustment()
    }

    /// Check warmup exit conditions
    fn check_warmup_exit(&mut self) -> StrategyAdjustment {
        if self.warmup_request_count < WARMUP_REQUESTS {
            return StrategyAdjustment::NoChange;
        }

        if self.warmup_request_count >= MAX_WARMUP_REQUESTS {
            let success_rate = self.metrics.success_rate();
            info!(
                "Warmup forced exit after {} requests (success_rate={:.1}%)",
                self.warmup_request_count,
                success_rate * 100.0
            );
            self.warmup_state = WarmupState::Completed;
            return StrategyAdjustment::NoChange;
        }

        if self.metrics.sample_count() == 0 {
            return StrategyAdjustment::NoChange;
        }

        let success_rate = self.metrics.success_rate();

        if success_rate >= WARMUP_SUCCESS_THRESHOLD
            && self.metrics.latency_health() != LatencyHealth::High
        {
            info!(
                "Warmup success: jumping to target concurrency {} (success_rate={:.1}%)",
                self.target_concurrency,
                success_rate * 100.0
            );
            self.warmup_state = WarmupState::Completed;
            if self.cli_overrides.upload_concurrency.is_none() {
                self.concurrency = self.target_concurrency;
            }
            self.metrics.reset_adjustment_counter();
            return StrategyAdjustment::Upgrade;
        }

        if success_rate < DOWNGRADE_SUCCESS_THRESHOLD {
            info!(
                "Warmup failed: keeping concurrency=1 (success_rate={:.1}%)",
                success_rate * 100.0
            );
            self.warmup_state = WarmupState::Completed;
            return StrategyAdjustment::NoChange;
        }

        StrategyAdjustment::NoChange
    }

    /// Evaluate if strategy adjustment is needed (AIMD algorithm)
    fn evaluate_adjustment(&mut self) -> StrategyAdjustment {
        if !self.metrics.has_minimum_samples(MIN_SAMPLES) {
            return StrategyAdjustment::NoChange;
        }

        if self.metrics.requests_since_adjustment() < COOLDOWN_REQUESTS {
            return StrategyAdjustment::NoChange;
        }

        let success_rate = self.metrics.success_rate();
        let latency_health = self.metrics.latency_health();
        let has_rate_limit = self.metrics.has_rate_limit_errors();

        let adjustment = if success_rate < DOWNGRADE_SUCCESS_THRESHOLD
            || has_rate_limit
            || latency_health == LatencyHealth::High
        {
            self.apply_downgrade(success_rate, has_rate_limit, latency_health)
        } else if success_rate > UPGRADE_SUCCESS_THRESHOLD
            && latency_health == LatencyHealth::Healthy
        {
            self.apply_upgrade(success_rate)
        } else {
            StrategyAdjustment::NoChange
        };

        if adjustment != StrategyAdjustment::NoChange {
            self.metrics.reset_adjustment_counter();
        }

        adjustment
    }

    /// Apply downgrade (Multiplicative Decrease)
    fn apply_downgrade(
        &mut self,
        success_rate: f64,
        has_rate_limit: bool,
        latency_health: LatencyHealth,
    ) -> StrategyAdjustment {
        let old_concurrency = self.concurrency;
        let old_timeout = self.timeout_ms;

        if self.cli_overrides.upload_concurrency.is_none() {
            self.concurrency = (self.concurrency / 2).max(MIN_CONCURRENCY);
        }

        if self.cli_overrides.upload_timeout_secs.is_none() {
            self.timeout_ms = ((self.timeout_ms as f64 * 1.5) as u64).min(MAX_TIMEOUT_MS);
        }

        let reason = if has_rate_limit {
            "rate_limited"
        } else if latency_health == LatencyHealth::High {
            "high_latency"
        } else {
            "low_success_rate"
        };

        info!(
            "Strategy DOWNGRADE ({}): concurrency {}→{}, timeout {}s→{}s, success_rate={:.1}%, ewma={:.0}ms",
            reason,
            old_concurrency,
            self.concurrency,
            old_timeout / 1000,
            self.timeout_ms / 1000,
            success_rate * 100.0,
            self.metrics.ewma_latency_ms()
        );

        StrategyAdjustment::Downgrade
    }

    /// Apply upgrade (Additive Increase)
    fn apply_upgrade(&mut self, success_rate: f64) -> StrategyAdjustment {
        let old_concurrency = self.concurrency;
        let old_timeout = self.timeout_ms;

        let at_max_concurrency =
            self.concurrency >= MAX_CONCURRENCY || self.concurrency >= self.target_concurrency;
        let at_min_timeout =
            self.timeout_ms <= MIN_TIMEOUT_MS || self.timeout_ms <= self.target_timeout_ms;

        if at_max_concurrency && at_min_timeout {
            return StrategyAdjustment::NoChange;
        }

        if self.cli_overrides.upload_concurrency.is_none() && !at_max_concurrency {
            self.concurrency = (self.concurrency + 1).min(MAX_CONCURRENCY);
        }

        if self.cli_overrides.upload_timeout_secs.is_none() && !at_min_timeout {
            self.timeout_ms = ((self.timeout_ms as f64 * 0.8) as u64).max(MIN_TIMEOUT_MS);
        }

        if self.concurrency == old_concurrency && self.timeout_ms == old_timeout {
            return StrategyAdjustment::NoChange;
        }

        info!(
            "Strategy UPGRADE: concurrency {}→{}, timeout {}s→{}s, success_rate={:.1}%, ewma={:.0}ms",
            old_concurrency,
            self.concurrency,
            old_timeout / 1000,
            self.timeout_ms / 1000,
            success_rate * 100.0,
            self.metrics.ewma_latency_ms()
        );

        StrategyAdjustment::Upgrade
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::metrics::ErrorType;

    fn default_overrides() -> CliOverrides {
        CliOverrides::default()
    }

    #[test]
    fn test_new_with_adaptive_enabled() {
        let strategy = AdaptiveStrategy::new(100, default_overrides(), true);
        assert_eq!(strategy.concurrency(), MIN_CONCURRENCY);
        assert!(strategy.adaptive_enabled);
    }

    #[test]
    fn test_new_with_adaptive_disabled() {
        let strategy = AdaptiveStrategy::new(100, default_overrides(), false);
        let heuristic = get_upload_strategy(100);
        assert_eq!(strategy.concurrency(), heuristic.concurrency);
    }

    #[test]
    fn test_cli_override_concurrency() {
        let overrides = CliOverrides {
            upload_concurrency: Some(5),
            upload_timeout_secs: None,
        };
        let strategy = AdaptiveStrategy::new(100, overrides, true);
        assert_eq!(strategy.concurrency(), 5);
    }

    #[test]
    fn test_cli_override_timeout() {
        let overrides = CliOverrides {
            upload_concurrency: None,
            upload_timeout_secs: Some(120),
        };
        let strategy = AdaptiveStrategy::new(100, overrides, true);
        assert_eq!(strategy.timeout_ms(), 120_000);
    }

    #[test]
    fn test_warmup_success_jumps_to_target() {
        let mut strategy = AdaptiveStrategy::new(100, default_overrides(), true);
        assert_eq!(strategy.concurrency(), MIN_CONCURRENCY);

        for _ in 0..WARMUP_REQUESTS {
            let adjustment = strategy.record_outcome(true, 1000, None);
            if adjustment == StrategyAdjustment::Upgrade {
                break;
            }
        }

        assert!(strategy.concurrency() > MIN_CONCURRENCY);
    }

    #[test]
    fn test_warmup_failure_stays_at_min() {
        let mut strategy = AdaptiveStrategy::new(100, default_overrides(), true);

        for i in 0..MAX_WARMUP_REQUESTS {
            let success = i < 2;
            strategy.record_outcome(
                success,
                1000,
                if success {
                    None
                } else {
                    Some(ErrorType::Timeout)
                },
            );
        }

        assert_eq!(strategy.concurrency(), MIN_CONCURRENCY);
    }

    #[test]
    fn test_downgrade_on_low_success_rate() {
        let mut strategy = AdaptiveStrategy::new(1000, default_overrides(), true);
        strategy.warmup_state = WarmupState::Completed;
        strategy.concurrency = 4;

        for _ in 0..(MIN_SAMPLES + COOLDOWN_REQUESTS) {
            strategy.record_outcome(false, 5000, Some(ErrorType::Timeout));
        }

        assert!(strategy.concurrency() < 4);
    }

    #[test]
    fn test_downgrade_on_rate_limit() {
        let mut strategy = AdaptiveStrategy::new(1000, default_overrides(), true);
        strategy.warmup_state = WarmupState::Completed;
        strategy.concurrency = 4;

        for _ in 0..MIN_SAMPLES {
            strategy.record_outcome(true, 1000, None);
        }

        for _ in 0..COOLDOWN_REQUESTS {
            strategy.record_outcome(false, 100, Some(ErrorType::RateLimit));
        }

        assert!(strategy.concurrency() < 4);
    }

    #[test]
    fn test_no_adjustment_during_cooldown() {
        let mut strategy = AdaptiveStrategy::new(100, default_overrides(), true);
        strategy.warmup_state = WarmupState::Completed;
        strategy.concurrency = 4;

        for _ in 0..MIN_SAMPLES {
            strategy.record_outcome(true, 1000, None);
        }
        strategy.metrics.reset_adjustment_counter();

        for _ in 0..(COOLDOWN_REQUESTS - 1) {
            let adjustment = strategy.record_outcome(true, 100, None);
            assert_eq!(adjustment, StrategyAdjustment::NoChange);
        }
    }

    #[test]
    fn test_5xx_does_not_affect_strategy() {
        let mut strategy = AdaptiveStrategy::new(100, default_overrides(), true);
        strategy.warmup_state = WarmupState::Completed;
        let initial_concurrency = strategy.concurrency();

        for _ in 0..50 {
            strategy.record_outcome(false, 5000, Some(ErrorType::ServerError));
        }

        assert_eq!(strategy.concurrency(), initial_concurrency);
    }
}
