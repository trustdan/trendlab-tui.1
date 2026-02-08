//! Circuit breaker for data provider rate limiting and IP bans.
//!
//! When the provider returns HTTP 403 (IP ban) or repeated 429 (rate limit),
//! the circuit breaker trips and refuses all subsequent requests for a cooldown
//! period (default 30 minutes).

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// State of the circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    /// Normal operation — requests are allowed.
    Closed,
    /// Tripped — all requests are refused until cooldown expires.
    Open { tripped_at: Instant },
}

/// Circuit breaker that prevents hammering a provider after a ban or rate limit.
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Mutex<BreakerState>,
    cooldown: Duration,
    consecutive_failures: Mutex<u32>,
    failure_threshold: u32,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given cooldown duration.
    pub fn new(cooldown: Duration) -> Self {
        Self {
            state: Mutex::new(BreakerState::Closed),
            cooldown,
            consecutive_failures: Mutex::new(0),
            failure_threshold: 3,
        }
    }

    /// Default circuit breaker: 30-minute cooldown, trips after 3 consecutive failures.
    pub fn default_provider() -> Self {
        Self::new(Duration::from_secs(30 * 60))
    }

    /// Check if requests are currently allowed.
    pub fn is_allowed(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        match *state {
            BreakerState::Closed => true,
            BreakerState::Open { tripped_at } => {
                if tripped_at.elapsed() >= self.cooldown {
                    // Cooldown expired — reset to closed
                    *state = BreakerState::Closed;
                    *self.consecutive_failures.lock().unwrap() = 0;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a successful request — resets the failure counter.
    pub fn record_success(&self) {
        *self.consecutive_failures.lock().unwrap() = 0;
    }

    /// Record a failure. If the failure count exceeds the threshold, trip the breaker.
    pub fn record_failure(&self) {
        let mut failures = self.consecutive_failures.lock().unwrap();
        *failures += 1;
        if *failures >= self.failure_threshold {
            *self.state.lock().unwrap() = BreakerState::Open {
                tripped_at: Instant::now(),
            };
        }
    }

    /// Immediately trip the breaker (for 403 Forbidden / IP ban).
    pub fn trip(&self) {
        *self.state.lock().unwrap() = BreakerState::Open {
            tripped_at: Instant::now(),
        };
    }

    /// Remaining cooldown time (zero if not tripped).
    pub fn remaining_cooldown(&self) -> Duration {
        let state = self.state.lock().unwrap();
        match *state {
            BreakerState::Closed => Duration::ZERO,
            BreakerState::Open { tripped_at } => self.cooldown.saturating_sub(tripped_at.elapsed()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_closed() {
        let cb = CircuitBreaker::new(Duration::from_secs(60));
        assert!(cb.is_allowed());
    }

    #[test]
    fn trips_after_threshold_failures() {
        let cb = CircuitBreaker::new(Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_allowed()); // 2 < 3
        cb.record_failure();
        assert!(!cb.is_allowed()); // 3 >= 3 → tripped
    }

    #[test]
    fn immediate_trip() {
        let cb = CircuitBreaker::new(Duration::from_secs(60));
        cb.trip();
        assert!(!cb.is_allowed());
    }

    #[test]
    fn success_resets_counter() {
        let cb = CircuitBreaker::new(Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        cb.record_failure(); // 1 failure after reset
        assert!(cb.is_allowed()); // still below threshold
    }

    #[test]
    fn expires_after_cooldown() {
        let cb = CircuitBreaker::new(Duration::from_millis(10));
        cb.trip();
        assert!(!cb.is_allowed());
        std::thread::sleep(Duration::from_millis(15));
        assert!(cb.is_allowed()); // cooldown expired
    }
}
