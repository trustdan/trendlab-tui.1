//! Keltner Channel — EMA +/- ATR multiplier.
//!
//! Three bands (separate Indicator instances):
//! - Middle: EMA(close, ema_period)
//! - Upper: middle + mult * ATR(atr_period)
//! - Lower: middle - mult * ATR(atr_period)
//!
//! Lookback: max(ema_period - 1, atr_period).

use crate::components::indicator::Indicator;
use crate::domain::Bar;
use crate::indicators::atr::{true_range, wilder_smooth};
use crate::indicators::ema::ema_of_series;

/// Which band of the Keltner Channel to compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeltnerBand {
    Upper,
    Middle,
    Lower,
}

#[derive(Debug, Clone)]
pub struct Keltner {
    ema_period: usize,
    atr_period: usize,
    multiplier: f64,
    band: KeltnerBand,
    name: String,
}

impl Keltner {
    pub fn upper(ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        Self {
            ema_period,
            atr_period,
            multiplier,
            band: KeltnerBand::Upper,
            name: format!("keltner_upper_{ema_period}_{atr_period}_{multiplier}"),
        }
    }

    pub fn middle(ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        Self {
            ema_period,
            atr_period,
            multiplier,
            band: KeltnerBand::Middle,
            name: format!("keltner_middle_{ema_period}_{atr_period}_{multiplier}"),
        }
    }

    pub fn lower(ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        Self {
            ema_period,
            atr_period,
            multiplier,
            band: KeltnerBand::Lower,
            name: format!("keltner_lower_{ema_period}_{atr_period}_{multiplier}"),
        }
    }
}

impl Indicator for Keltner {
    fn name(&self) -> &str {
        &self.name
    }

    fn lookback(&self) -> usize {
        (self.ema_period.saturating_sub(1)).max(self.atr_period)
    }

    fn compute(&self, bars: &[Bar]) -> Vec<f64> {
        let n = bars.len();

        // Compute EMA of closes
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        let ema_values = ema_of_series(&closes, self.ema_period);

        // For middle band, just return EMA
        if self.band == KeltnerBand::Middle {
            return ema_values;
        }

        // Compute ATR
        let tr = true_range(bars);
        let atr_values = wilder_smooth(&tr, self.atr_period);

        // Combine: middle +/- mult * ATR
        let mut result = vec![f64::NAN; n];
        for i in 0..n {
            if ema_values[i].is_nan() || atr_values[i].is_nan() {
                continue;
            }
            result[i] = match self.band {
                KeltnerBand::Upper => ema_values[i] + self.multiplier * atr_values[i],
                KeltnerBand::Lower => ema_values[i] - self.multiplier * atr_values[i],
                KeltnerBand::Middle => unreachable!(),
            };
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::{assert_approx, DEFAULT_EPSILON};
    use chrono::NaiveDate;

    fn make_ohlc_bars(data: &[(f64, f64, f64, f64)]) -> Vec<Bar> {
        let base_date = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        data.iter()
            .enumerate()
            .map(|(i, &(open, high, low, close))| Bar {
                symbol: "TEST".to_string(),
                date: base_date + chrono::Duration::days(i as i64),
                open,
                high,
                low,
                close,
                volume: 1000,
                adj_close: close,
            })
            .collect()
    }

    #[test]
    fn keltner_middle_is_ema() {
        let bars = make_ohlc_bars(&[
            (10.0, 12.0, 9.0, 11.0),
            (11.0, 13.0, 10.0, 12.0),
            (12.0, 14.0, 11.0, 13.0),
            (13.0, 15.0, 12.0, 14.0),
        ]);
        let kc = Keltner::middle(3, 3, 1.5);
        let result = kc.compute(&bars);

        // EMA(3) seed at index 2: mean(11,12,13) = 12.0
        assert_approx(result[2], 12.0, DEFAULT_EPSILON);
    }

    #[test]
    fn keltner_upper_gt_middle_gt_lower() {
        let bars = make_ohlc_bars(&[
            (10.0, 12.0, 9.0, 11.0),
            (11.0, 13.0, 10.0, 12.0),
            (12.0, 14.0, 11.0, 13.0),
            (13.0, 15.0, 12.0, 14.0),
            (14.0, 16.0, 13.0, 15.0),
        ]);
        let upper = Keltner::upper(3, 3, 1.5);
        let middle = Keltner::middle(3, 3, 1.5);
        let lower = Keltner::lower(3, 3, 1.5);

        let u = upper.compute(&bars);
        let m = middle.compute(&bars);
        let l = lower.compute(&bars);

        // Check at indices where all are valid
        for i in 0..5 {
            if !u[i].is_nan() && !m[i].is_nan() && !l[i].is_nan() {
                assert!(
                    u[i] > m[i] && m[i] > l[i],
                    "bands not ordered at {i}: upper={}, middle={}, lower={}",
                    u[i],
                    m[i],
                    l[i]
                );
            }
        }
    }

    #[test]
    fn keltner_symmetric_bands() {
        let bars = make_ohlc_bars(&[
            (10.0, 12.0, 9.0, 11.0),
            (11.0, 13.0, 10.0, 12.0),
            (12.0, 14.0, 11.0, 13.0),
            (13.0, 15.0, 12.0, 14.0),
            (14.0, 16.0, 13.0, 15.0),
        ]);
        let upper = Keltner::upper(3, 3, 1.5);
        let middle = Keltner::middle(3, 3, 1.5);
        let lower = Keltner::lower(3, 3, 1.5);

        let u = upper.compute(&bars);
        let m = middle.compute(&bars);
        let l = lower.compute(&bars);

        for i in 0..5 {
            if !u[i].is_nan() && !m[i].is_nan() && !l[i].is_nan() {
                let upper_dist = u[i] - m[i];
                let lower_dist = m[i] - l[i];
                assert_approx(upper_dist, lower_dist, DEFAULT_EPSILON);
            }
        }
    }

    #[test]
    fn keltner_lookback() {
        // ema_period=20, atr_period=14 → lookback = max(19, 14) = 19
        assert_eq!(Keltner::upper(20, 14, 1.5).lookback(), 19);
        // ema_period=10, atr_period=14 → lookback = max(9, 14) = 14
        assert_eq!(Keltner::upper(10, 14, 1.5).lookback(), 14);
    }
}
