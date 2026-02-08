//! Supertrend — ATR-based directional indicator.
//!
//! Inherently sequential/stateful: direction flips between support and resistance
//! based on close vs band comparisons.
//!
//! Lookback: atr_period (same as ATR lookback since it depends on ATR).
//!
//! Output: the active band value — lower band (support) when trending up,
//! upper band (resistance) when trending down.

use crate::components::indicator::Indicator;
use crate::domain::Bar;
use crate::indicators::atr::{true_range, wilder_smooth};

#[derive(Debug, Clone)]
pub struct Supertrend {
    period: usize,
    multiplier: f64,
    name: String,
}

impl Supertrend {
    pub fn new(period: usize, multiplier: f64) -> Self {
        assert!(period >= 1, "Supertrend period must be >= 1");
        Self {
            period,
            multiplier,
            name: format!("supertrend_{period}_{multiplier}"),
        }
    }
}

impl Indicator for Supertrend {
    fn name(&self) -> &str {
        &self.name
    }

    fn lookback(&self) -> usize {
        self.period
    }

    fn compute(&self, bars: &[Bar]) -> Vec<f64> {
        let n = bars.len();
        let mut result = vec![f64::NAN; n];

        // Compute ATR
        let tr = true_range(bars);
        let atr = wilder_smooth(&tr, self.period);

        // Find the first bar where ATR is valid
        let start = match atr.iter().position(|v| !v.is_nan()) {
            Some(idx) => idx,
            None => return result,
        };

        if start >= n {
            return result;
        }

        // Initialize direction and bands
        let hl2 = (bars[start].high + bars[start].low) / 2.0;
        let mut upper_band = hl2 + self.multiplier * atr[start];
        let mut lower_band = hl2 - self.multiplier * atr[start];
        // Start trending up (support)
        let mut trending_up = true;
        result[start] = lower_band;

        for i in (start + 1)..n {
            if atr[i].is_nan()
                || bars[i].close.is_nan()
                || bars[i].high.is_nan()
                || bars[i].low.is_nan()
            {
                result[i] = f64::NAN;
                continue;
            }

            let hl2 = (bars[i].high + bars[i].low) / 2.0;
            let basic_upper = hl2 + self.multiplier * atr[i];
            let basic_lower = hl2 - self.multiplier * atr[i];

            // Upper band: can only decrease (tighten resistance)
            let prev_close = bars[i - 1].close;
            let new_upper = if !prev_close.is_nan() && prev_close <= upper_band {
                basic_upper.min(upper_band)
            } else {
                basic_upper
            };

            // Lower band: can only increase (tighten support)
            let new_lower = if !prev_close.is_nan() && prev_close >= lower_band {
                basic_lower.max(lower_band)
            } else {
                basic_lower
            };

            upper_band = new_upper;
            lower_band = new_lower;

            // Direction flip
            if trending_up && bars[i].close < lower_band {
                trending_up = false;
            } else if !trending_up && bars[i].close > upper_band {
                trending_up = true;
            }

            result[i] = if trending_up { lower_band } else { upper_band };
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn supertrend_uptrend_below_price() {
        // In an uptrend, supertrend (lower band) should be below the close
        let mut data = Vec::new();
        for i in 0..15 {
            let base = 100.0 + i as f64 * 2.0;
            data.push((base - 1.0, base + 3.0, base - 3.0, base + 1.0));
        }
        let bars = make_ohlc_bars(&data);
        let st = Supertrend::new(3, 2.0);
        let result = st.compute(&bars);

        // After warmup, supertrend should be below close (trending up)
        for i in 5..15 {
            if !result[i].is_nan() {
                assert!(
                    result[i] < bars[i].close,
                    "supertrend ({}) should be below close ({}) at bar {i} in uptrend",
                    result[i],
                    bars[i].close
                );
            }
        }
    }

    #[test]
    fn supertrend_downtrend_above_price() {
        // In a downtrend, supertrend (upper band) should be above the close
        let mut data = Vec::new();
        for i in 0..15 {
            let base = 200.0 - i as f64 * 3.0;
            data.push((base + 1.0, base + 3.0, base - 3.0, base - 1.0));
        }
        let bars = make_ohlc_bars(&data);
        let st = Supertrend::new(3, 2.0);
        let result = st.compute(&bars);

        // After initial period, should flip to downtrend eventually
        let mut found_above = false;
        for i in 5..15 {
            if !result[i].is_nan() && result[i] > bars[i].close {
                found_above = true;
            }
        }
        assert!(
            found_above,
            "supertrend should be above close at some point in a downtrend"
        );
    }

    #[test]
    fn supertrend_lookback() {
        assert_eq!(Supertrend::new(14, 3.0).lookback(), 14);
    }

    #[test]
    fn supertrend_too_few_bars() {
        let bars = make_ohlc_bars(&[(100.0, 105.0, 95.0, 102.0)]);
        let st = Supertrend::new(3, 2.0);
        let result = st.compute(&bars);
        assert!(result.iter().all(|v| v.is_nan()));
    }
}
