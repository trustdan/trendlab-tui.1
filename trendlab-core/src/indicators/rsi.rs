//! Relative Strength Index (RSI).
//!
//! Uses Wilder smoothing of average gains and average losses.
//! RSI = 100 - 100 / (1 + avg_gain / avg_loss)
//! Lookback: period.
//! Edge cases: avg_loss == 0 → RSI = 100; avg_gain == 0 → RSI = 0.

use crate::components::indicator::Indicator;
use crate::domain::Bar;

#[derive(Debug, Clone)]
pub struct Rsi {
    period: usize,
    name: String,
}

impl Rsi {
    pub fn new(period: usize) -> Self {
        assert!(period >= 1, "RSI period must be >= 1");
        Self {
            period,
            name: format!("rsi_{period}"),
        }
    }
}

impl Indicator for Rsi {
    fn name(&self) -> &str {
        &self.name
    }

    fn lookback(&self) -> usize {
        self.period
    }

    fn compute(&self, bars: &[Bar]) -> Vec<f64> {
        let n = bars.len();
        let mut result = vec![f64::NAN; n];

        if n < self.period + 1 {
            return result;
        }

        // Compute price changes
        let mut changes = vec![f64::NAN; n];
        for i in 1..n {
            let curr = bars[i].close;
            let prev = bars[i - 1].close;
            if curr.is_nan() || prev.is_nan() {
                changes[i] = f64::NAN;
            } else {
                changes[i] = curr - prev;
            }
        }

        // Seed: average gain and average loss over first `period` changes
        let mut avg_gain = 0.0;
        let mut avg_loss = 0.0;
        for &ch in &changes[1..=self.period] {
            if ch.is_nan() {
                return result;
            }
            if ch > 0.0 {
                avg_gain += ch;
            } else {
                avg_loss -= ch;
            }
        }
        avg_gain /= self.period as f64;
        avg_loss /= self.period as f64;

        // First RSI value
        result[self.period] = compute_rsi(avg_gain, avg_loss);

        // Wilder smoothing for subsequent values
        let alpha = 1.0 / self.period as f64;
        for i in (self.period + 1)..n {
            if changes[i].is_nan() {
                for val in result.iter_mut().skip(i) {
                    *val = f64::NAN;
                }
                return result;
            }

            let gain = if changes[i] > 0.0 { changes[i] } else { 0.0 };
            let loss = if changes[i] < 0.0 { -changes[i] } else { 0.0 };

            avg_gain = alpha * gain + (1.0 - alpha) * avg_gain;
            avg_loss = alpha * loss + (1.0 - alpha) * avg_loss;

            result[i] = compute_rsi(avg_gain, avg_loss);
        }

        result
    }
}

fn compute_rsi(avg_gain: f64, avg_loss: f64) -> f64 {
    if avg_loss == 0.0 && avg_gain == 0.0 {
        50.0 // no movement
    } else if avg_loss == 0.0 {
        100.0
    } else if avg_gain == 0.0 {
        0.0
    } else {
        100.0 - 100.0 / (1.0 + avg_gain / avg_loss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::{assert_approx, make_bars};

    #[test]
    fn rsi_all_gains() {
        let bars = make_bars(&[100.0, 101.0, 102.0, 103.0, 104.0, 105.0]);
        let rsi = Rsi::new(3);
        let result = rsi.compute(&bars);
        // All positive changes → RSI = 100
        assert_approx(result[3], 100.0, 1e-6);
    }

    #[test]
    fn rsi_all_losses() {
        let bars = make_bars(&[105.0, 104.0, 103.0, 102.0, 101.0, 100.0]);
        let rsi = Rsi::new(3);
        let result = rsi.compute(&bars);
        // All negative changes → RSI = 0
        assert_approx(result[3], 0.0, 1e-6);
    }

    #[test]
    fn rsi_mixed() {
        // Closes: 44, 44.34, 44.09, 43.61, 44.33
        // Changes: +0.34, -0.25, -0.48, +0.72
        // period=3, seed from changes[1..=3]: gains=0.34, losses=0.25+0.48=0.73
        // avg_gain = 0.34/3, avg_loss = 0.73/3
        // RSI[3] = 100 - 100/(1 + (0.34/3)/(0.73/3)) = 100 - 100/(1 + 0.34/0.73)
        //        = 100 - 100/(1 + 0.4657534...) = 100 - 100/1.4657534
        //        = 100 - 68.224 = 31.776
        let bars = make_bars(&[44.0, 44.34, 44.09, 43.61, 44.33]);
        let rsi = Rsi::new(3);
        let result = rsi.compute(&bars);

        assert!(result[0].is_nan());
        assert!(result[1].is_nan());
        assert!(result[2].is_nan());
        assert!(result[3] > 0.0 && result[3] < 100.0);
    }

    #[test]
    fn rsi_bounds() {
        // RSI should always be between 0 and 100
        let bars = make_bars(&[100.0, 105.0, 98.0, 110.0, 95.0, 115.0, 90.0, 120.0]);
        let rsi = Rsi::new(3);
        let result = rsi.compute(&bars);
        for (i, &v) in result.iter().enumerate() {
            if !v.is_nan() {
                assert!(
                    (0.0..=100.0).contains(&v),
                    "RSI out of bounds at bar {i}: {v}"
                );
            }
        }
    }

    #[test]
    fn rsi_nan_propagation() {
        let mut bars = make_bars(&[100.0, 101.0, 102.0, 103.0, 104.0]);
        bars[2].close = f64::NAN;
        let rsi = Rsi::new(3);
        let result = rsi.compute(&bars);
        // NaN in seed window → all NaN
        assert!(result.iter().all(|v| v.is_nan()));
    }

    #[test]
    fn rsi_lookback() {
        assert_eq!(Rsi::new(14).lookback(), 14);
    }
}
