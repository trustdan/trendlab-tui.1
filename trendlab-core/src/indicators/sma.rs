//! Simple Moving Average (SMA).
//!
//! Rolling mean of close prices over a lookback window.
//! Lookback: period - 1 (first valid value at index period-1).

use crate::components::indicator::Indicator;
use crate::domain::Bar;

#[derive(Debug, Clone)]
pub struct Sma {
    period: usize,
    name: String,
}

impl Sma {
    pub fn new(period: usize) -> Self {
        assert!(period >= 1, "SMA period must be >= 1");
        Self {
            period,
            name: format!("sma_{period}"),
        }
    }
}

impl Indicator for Sma {
    fn name(&self) -> &str {
        &self.name
    }

    fn lookback(&self) -> usize {
        self.period.saturating_sub(1)
    }

    fn compute(&self, bars: &[Bar]) -> Vec<f64> {
        let n = bars.len();
        let mut result = vec![f64::NAN; n];

        if n < self.period {
            return result;
        }

        // Compute initial window sum
        let mut sum = 0.0;
        let mut nan_in_window = false;
        for bar in bars.iter().take(self.period) {
            if bar.close.is_nan() {
                nan_in_window = true;
            }
            sum += bar.close;
        }

        if !nan_in_window {
            result[self.period - 1] = sum / self.period as f64;
        }

        // Roll the window forward
        for i in self.period..n {
            let leaving = bars[i - self.period].close;
            let entering = bars[i].close;
            sum = sum - leaving + entering;

            // Check for NaN in the current window. Since we track add/remove,
            // we need to check if any value in the window is NaN.
            // For correctness with NaN propagation, recompute if we suspect NaN.
            if entering.is_nan() || leaving.is_nan() || nan_in_window {
                // Recompute: scan window for NaN
                nan_in_window = false;
                sum = 0.0;
                for bar in &bars[(i + 1 - self.period)..=i] {
                    if bar.close.is_nan() {
                        nan_in_window = true;
                    }
                    sum += bar.close;
                }
                if nan_in_window {
                    result[i] = f64::NAN;
                    continue;
                }
            }

            result[i] = sum / self.period as f64;
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::{assert_approx, make_bars, DEFAULT_EPSILON};

    #[test]
    fn sma_5_basic() {
        let bars = make_bars(&[10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0]);
        let sma = Sma::new(5);
        let result = sma.compute(&bars);

        assert_eq!(result.len(), 7);
        for i in 0..4 {
            assert!(result[i].is_nan(), "expected NaN at index {i}");
        }
        // SMA[4] = mean(10,11,12,13,14) = 12.0
        assert_approx(result[4], 12.0, DEFAULT_EPSILON);
        // SMA[5] = mean(11,12,13,14,15) = 13.0
        assert_approx(result[5], 13.0, DEFAULT_EPSILON);
        // SMA[6] = mean(12,13,14,15,16) = 14.0
        assert_approx(result[6], 14.0, DEFAULT_EPSILON);
    }

    #[test]
    fn sma_1_is_close() {
        let bars = make_bars(&[100.0, 200.0, 300.0]);
        let sma = Sma::new(1);
        let result = sma.compute(&bars);
        assert_approx(result[0], 100.0, DEFAULT_EPSILON);
        assert_approx(result[1], 200.0, DEFAULT_EPSILON);
        assert_approx(result[2], 300.0, DEFAULT_EPSILON);
    }

    #[test]
    fn sma_nan_propagation() {
        let mut bars = make_bars(&[10.0, 11.0, 12.0, 13.0, 14.0, 15.0]);
        bars[2].close = f64::NAN;
        let sma = Sma::new(3);
        let result = sma.compute(&bars);
        // lookback = 2, first valid at index 2
        // Index 2 window [10,11,NaN] → NaN
        assert!(result[2].is_nan());
        // Index 3 window [11,NaN,13] → NaN
        assert!(result[3].is_nan());
        // Index 4 window [NaN,13,14] → NaN
        assert!(result[4].is_nan());
        // Index 5 window [13,14,15] → 14.0
        assert_approx(result[5], 14.0, DEFAULT_EPSILON);
    }

    #[test]
    fn sma_lookback() {
        assert_eq!(Sma::new(20).lookback(), 19);
        assert_eq!(Sma::new(1).lookback(), 0);
    }

    #[test]
    fn sma_too_few_bars() {
        let bars = make_bars(&[10.0, 11.0]);
        let sma = Sma::new(5);
        let result = sma.compute(&bars);
        assert!(result.iter().all(|v| v.is_nan()));
    }
}
