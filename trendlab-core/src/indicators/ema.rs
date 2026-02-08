//! Exponential Moving Average (EMA).
//!
//! Recursive: EMA[t] = alpha * close[t] + (1 - alpha) * EMA[t-1]
//! Seed: EMA[period-1] = SMA of first `period` close values.
//! Lookback: period - 1.

use crate::components::indicator::Indicator;
use crate::domain::Bar;

#[derive(Debug, Clone)]
pub struct Ema {
    period: usize,
    name: String,
}

impl Ema {
    pub fn new(period: usize) -> Self {
        assert!(period >= 1, "EMA period must be >= 1");
        Self {
            period,
            name: format!("ema_{period}"),
        }
    }
}

impl Indicator for Ema {
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

        let alpha = 2.0 / (self.period as f64 + 1.0);

        // Seed: SMA of first `period` values
        let mut sum = 0.0;
        for bar in bars.iter().take(self.period) {
            if bar.close.is_nan() {
                return result; // NaN in seed window → all NaN after seed
            }
            sum += bar.close;
        }
        let seed = sum / self.period as f64;
        result[self.period - 1] = seed;

        // Recursive EMA
        let mut prev = seed;
        for i in self.period..n {
            if bars[i].close.is_nan() {
                // NaN propagates: once we see NaN, subsequent values are tainted
                for val in result.iter_mut().skip(i) {
                    *val = f64::NAN;
                }
                return result;
            }
            let ema = alpha * bars[i].close + (1.0 - alpha) * prev;
            result[i] = ema;
            prev = ema;
        }

        result
    }
}

/// Compute raw EMA values from a pre-extracted f64 slice.
/// Used internally by composed indicators (Keltner, ADX) that need EMA of arbitrary series.
pub fn ema_of_series(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    let mut result = vec![f64::NAN; n];

    if n < period || period == 0 {
        return result;
    }

    let alpha = 2.0 / (period as f64 + 1.0);

    // Seed: SMA of first `period` values
    let mut sum = 0.0;
    for &v in values.iter().take(period) {
        if v.is_nan() {
            return result;
        }
        sum += v;
    }
    let seed = sum / period as f64;
    result[period - 1] = seed;

    let mut prev = seed;
    for i in period..n {
        if values[i].is_nan() {
            for val in result.iter_mut().skip(i) {
                *val = f64::NAN;
            }
            return result;
        }
        let ema = alpha * values[i] + (1.0 - alpha) * prev;
        result[i] = ema;
        prev = ema;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::{assert_approx, make_bars, DEFAULT_EPSILON};

    #[test]
    fn ema_period_1_equals_close() {
        let bars = make_bars(&[100.0, 200.0, 300.0]);
        let ema = Ema::new(1);
        let result = ema.compute(&bars);
        assert_approx(result[0], 100.0, DEFAULT_EPSILON);
        assert_approx(result[1], 200.0, DEFAULT_EPSILON);
        assert_approx(result[2], 300.0, DEFAULT_EPSILON);
    }

    #[test]
    fn ema_3_known_values() {
        // Closes: 10, 11, 12, 13, 14
        // alpha = 2/(3+1) = 0.5
        // Seed at index 2: SMA(10,11,12) = 11.0
        // EMA[3] = 0.5*13 + 0.5*11.0 = 12.0
        // EMA[4] = 0.5*14 + 0.5*12.0 = 13.0
        let bars = make_bars(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        let ema = Ema::new(3);
        let result = ema.compute(&bars);

        assert!(result[0].is_nan());
        assert!(result[1].is_nan());
        assert_approx(result[2], 11.0, DEFAULT_EPSILON);
        assert_approx(result[3], 12.0, DEFAULT_EPSILON);
        assert_approx(result[4], 13.0, DEFAULT_EPSILON);
    }

    #[test]
    fn ema_nan_in_seed_produces_all_nan() {
        let mut bars = make_bars(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        bars[1].close = f64::NAN;
        let ema = Ema::new(3);
        let result = ema.compute(&bars);
        assert!(result.iter().all(|v| v.is_nan()));
    }

    #[test]
    fn ema_nan_after_seed_propagates() {
        let mut bars = make_bars(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        bars[3].close = f64::NAN;
        let ema = Ema::new(3);
        let result = ema.compute(&bars);
        // Seed at 2 is valid
        assert_approx(result[2], 11.0, DEFAULT_EPSILON);
        // Index 3 is NaN → rest are NaN
        assert!(result[3].is_nan());
        assert!(result[4].is_nan());
    }

    #[test]
    fn ema_lookback() {
        assert_eq!(Ema::new(20).lookback(), 19);
        assert_eq!(Ema::new(1).lookback(), 0);
    }

    #[test]
    fn ema_of_series_matches_indicator() {
        let bars = make_bars(&[10.0, 11.0, 12.0, 13.0, 14.0, 15.0]);
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        let ema = Ema::new(3);
        let indicator_result = ema.compute(&bars);
        let series_result = ema_of_series(&closes, 3);
        for i in 0..6 {
            if indicator_result[i].is_nan() {
                assert!(series_result[i].is_nan());
            } else {
                assert_approx(indicator_result[i], series_result[i], DEFAULT_EPSILON);
            }
        }
    }
}
