//! Bollinger Bands — moving average +/- standard deviation multiplier.
//!
//! Three bands (separate Indicator instances):
//! - Middle: SMA(close, period)
//! - Upper: middle + mult * stddev(close, period)
//! - Lower: middle - mult * stddev(close, period)
//!
//! Uses population stddev (divide by N).
//! Lookback: period - 1.

use crate::components::indicator::Indicator;
use crate::domain::Bar;

/// Which band of the Bollinger Bands to compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BollingerBand {
    Upper,
    Middle,
    Lower,
}

#[derive(Debug, Clone)]
pub struct Bollinger {
    period: usize,
    multiplier: f64,
    band: BollingerBand,
    name: String,
}

impl Bollinger {
    pub fn upper(period: usize, multiplier: f64) -> Self {
        assert!(period >= 1, "Bollinger period must be >= 1");
        Self {
            period,
            multiplier,
            band: BollingerBand::Upper,
            name: format!("bollinger_upper_{period}_{multiplier}"),
        }
    }

    pub fn middle(period: usize, multiplier: f64) -> Self {
        assert!(period >= 1, "Bollinger period must be >= 1");
        Self {
            period,
            multiplier,
            band: BollingerBand::Middle,
            name: format!("bollinger_middle_{period}_{multiplier}"),
        }
    }

    pub fn lower(period: usize, multiplier: f64) -> Self {
        assert!(period >= 1, "Bollinger period must be >= 1");
        Self {
            period,
            multiplier,
            band: BollingerBand::Lower,
            name: format!("bollinger_lower_{period}_{multiplier}"),
        }
    }
}

impl Indicator for Bollinger {
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

        for i in (self.period - 1)..n {
            let start = i + 1 - self.period;
            let window = &bars[start..=i];

            // Check for NaN in window
            let mut has_nan = false;
            let mut sum = 0.0;
            for bar in window {
                if bar.close.is_nan() {
                    has_nan = true;
                    break;
                }
                sum += bar.close;
            }

            if has_nan {
                result[i] = f64::NAN;
                continue;
            }

            let mean = sum / self.period as f64;

            match self.band {
                BollingerBand::Middle => {
                    result[i] = mean;
                }
                BollingerBand::Upper | BollingerBand::Lower => {
                    // Population stddev
                    let variance: f64 = window
                        .iter()
                        .map(|bar| {
                            let diff = bar.close - mean;
                            diff * diff
                        })
                        .sum::<f64>()
                        / self.period as f64;
                    let stddev = variance.sqrt();

                    result[i] = match self.band {
                        BollingerBand::Upper => mean + self.multiplier * stddev,
                        BollingerBand::Lower => mean - self.multiplier * stddev,
                        _ => unreachable!(),
                    };
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::{assert_approx, make_bars, DEFAULT_EPSILON};

    #[test]
    fn bollinger_middle_is_sma() {
        let bars = make_bars(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        let bb_mid = Bollinger::middle(3, 2.0);
        let result = bb_mid.compute(&bars);

        assert!(result[0].is_nan());
        assert!(result[1].is_nan());
        // SMA[2] = mean(10,11,12) = 11.0
        assert_approx(result[2], 11.0, DEFAULT_EPSILON);
        // SMA[3] = mean(11,12,13) = 12.0
        assert_approx(result[3], 12.0, DEFAULT_EPSILON);
    }

    #[test]
    fn bollinger_bands_symmetric() {
        let bars = make_bars(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        let bb_upper = Bollinger::upper(3, 2.0);
        let bb_middle = Bollinger::middle(3, 2.0);
        let bb_lower = Bollinger::lower(3, 2.0);

        let upper = bb_upper.compute(&bars);
        let middle = bb_middle.compute(&bars);
        let lower = bb_lower.compute(&bars);

        for i in 2..5 {
            let half_width = upper[i] - middle[i];
            assert_approx(middle[i] - lower[i], half_width, DEFAULT_EPSILON);
        }
    }

    #[test]
    fn bollinger_constant_price_zero_width() {
        let bars = make_bars(&[100.0, 100.0, 100.0, 100.0]);
        let bb_upper = Bollinger::upper(3, 2.0);
        let bb_lower = Bollinger::lower(3, 2.0);

        let upper = bb_upper.compute(&bars);
        let lower = bb_lower.compute(&bars);

        // Constant price → stddev = 0 → bands collapse to SMA
        assert_approx(upper[2], 100.0, DEFAULT_EPSILON);
        assert_approx(lower[2], 100.0, DEFAULT_EPSILON);
    }

    #[test]
    fn bollinger_nan_propagation() {
        let mut bars = make_bars(&[10.0, 11.0, 12.0, 13.0]);
        bars[2].close = f64::NAN;
        let bb = Bollinger::upper(3, 2.0);
        let result = bb.compute(&bars);
        assert!(result[2].is_nan());
        assert!(result[3].is_nan()); // window includes NaN bar 2
    }

    #[test]
    fn bollinger_lookback() {
        assert_eq!(Bollinger::upper(20, 2.0).lookback(), 19);
    }
}
