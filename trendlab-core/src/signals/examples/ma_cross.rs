//! Moving Average Crossover signal
//!
//! Classic trend-following signal:
//! - Long when fast MA crosses above slow MA
//! - Short when fast MA crosses below slow MA
//! - Flat when no clear cross signal

use crate::domain::Bar;
use crate::signals::{Signal, SignalFamily, SignalIntent};

/// Moving Average Crossover signal
///
/// # Parameters
/// - `fast_period`: Short MA period (e.g., 20)
/// - `slow_period`: Long MA period (e.g., 50)
///
/// # Signal Logic
/// - Long: fast MA crosses above slow MA (bullish)
/// - Short: fast MA crosses below slow MA (bearish)
/// - Flat: no recent cross
#[derive(Debug, Clone)]
pub struct MovingAverageCross {
    fast_period: usize,
    slow_period: usize,
}

impl MovingAverageCross {
    pub fn new(fast_period: usize, slow_period: usize) -> Self {
        assert!(fast_period > 0, "fast_period must be > 0");
        assert!(slow_period > fast_period, "slow_period must be > fast_period");
        Self {
            fast_period,
            slow_period,
        }
    }

    /// Calculate simple moving average
    fn sma(bars: &[Bar], period: usize) -> Option<f64> {
        if bars.len() < period {
            return None;
        }

        let recent = &bars[bars.len() - period..];
        let sum: f64 = recent.iter().map(|b| b.close).sum();
        let avg = sum / period as f64;
        Some(avg)
    }

    /// Detect MA crossover
    ///
    /// Returns:
    /// - Some(true): bullish cross (fast > slow)
    /// - Some(false): bearish cross (fast < slow)
    /// - None: no cross detected
    fn detect_cross(&self, bars: &[Bar]) -> Option<bool> {
        if bars.len() < self.slow_period + 1 {
            return None; // Not enough data
        }

        // Current MA values
        let fast_now = Self::sma(bars, self.fast_period)?;
        let slow_now = Self::sma(bars, self.slow_period)?;

        // Previous MA values (one bar ago)
        let bars_prev = &bars[..bars.len() - 1];
        let fast_prev = Self::sma(bars_prev, self.fast_period)?;
        let slow_prev = Self::sma(bars_prev, self.slow_period)?;

        // Detect cross
        if fast_prev <= slow_prev && fast_now > slow_now {
            Some(true) // Bullish cross
        } else if fast_prev >= slow_prev && fast_now < slow_now {
            Some(false) // Bearish cross
        } else {
            None // No cross
        }
    }
}

impl Signal for MovingAverageCross {
    fn generate(&self, bars: &[Bar]) -> SignalIntent {
        match self.detect_cross(bars) {
            Some(true) => SignalIntent::Long,
            Some(false) => SignalIntent::Short,
            None => {
                // No cross: maintain current trend direction
                // (or Flat if no trend established)
                if bars.len() >= self.slow_period {
                    let fast = Self::sma(bars, self.fast_period);
                    let slow = Self::sma(bars, self.slow_period);
                    match (fast, slow) {
                        (Some(f), Some(s)) if f > s => SignalIntent::Long,
                        (Some(f), Some(s)) if f < s => SignalIntent::Short,
                        _ => SignalIntent::Flat,
                    }
                } else {
                    SignalIntent::Flat
                }
            }
        }
    }

    fn name(&self) -> &str {
        "MA_Cross"
    }

    fn max_lookback(&self) -> usize {
        self.slow_period + 1 // Need one extra bar to detect cross
    }

    fn signal_family(&self) -> SignalFamily {
        SignalFamily::Trend
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64) -> Bar {
        Bar::new(
            Utc::now(),
            "SPY".into(),
            100.0,
            101.0,
            99.0,
            close,
            1000000.0,
        )
    }

    #[test]
    fn test_ma_cross_bullish() {
        let signal = MovingAverageCross::new(2, 3);

        // Create uptrend: fast MA crosses above slow MA
        let bars = vec![
            make_bar(100.0),
            make_bar(101.0),
            make_bar(102.0),
            make_bar(105.0), // Fast MA accelerates above slow
        ];

        let intent = signal.generate(&bars);
        assert_eq!(intent, SignalIntent::Long);
    }

    #[test]
    fn test_ma_cross_bearish() {
        let signal = MovingAverageCross::new(2, 3);

        // Create downtrend: fast MA crosses below slow MA
        let bars = vec![
            make_bar(105.0),
            make_bar(104.0),
            make_bar(103.0),
            make_bar(100.0), // Fast MA drops below slow
        ];

        let intent = signal.generate(&bars);
        assert_eq!(intent, SignalIntent::Short);
    }

    #[test]
    fn test_ma_cross_insufficient_data() {
        let signal = MovingAverageCross::new(20, 50);

        let bars = vec![make_bar(100.0), make_bar(101.0)];

        let intent = signal.generate(&bars);
        assert_eq!(intent, SignalIntent::Flat);
    }

    #[test]
    fn test_max_lookback() {
        let signal = MovingAverageCross::new(20, 50);
        assert_eq!(signal.max_lookback(), 51); // slow + 1
    }

    #[test]
    fn test_signal_family() {
        let signal = MovingAverageCross::new(20, 50);
        assert_eq!(signal.signal_family(), SignalFamily::Trend);
    }
}
