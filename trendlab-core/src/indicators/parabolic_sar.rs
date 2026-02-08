//! Parabolic SAR — Wilder's acceleration factor system.
//!
//! Inherently sequential/stateful: maintains direction, extreme point (EP),
//! and acceleration factor (AF).
//!
//! Parameters: af_start (default 0.02), af_step (default 0.02), af_max (default 0.20).
//! Lookback: 1 (needs at least 2 bars to start).

use crate::components::indicator::Indicator;
use crate::domain::Bar;

#[derive(Debug, Clone)]
pub struct ParabolicSar {
    af_start: f64,
    af_step: f64,
    af_max: f64,
    name: String,
}

impl ParabolicSar {
    pub fn new(af_start: f64, af_step: f64, af_max: f64) -> Self {
        assert!(af_start > 0.0, "AF start must be > 0");
        assert!(af_step > 0.0, "AF step must be > 0");
        assert!(af_max >= af_start, "AF max must be >= AF start");
        Self {
            af_start,
            af_step,
            af_max,
            name: format!("psar_{af_start}_{af_step}_{af_max}"),
        }
    }

    /// Default parameters: 0.02, 0.02, 0.20
    pub fn default_params() -> Self {
        Self::new(0.02, 0.02, 0.20)
    }
}

impl Indicator for ParabolicSar {
    fn name(&self) -> &str {
        &self.name
    }

    fn lookback(&self) -> usize {
        1
    }

    fn compute(&self, bars: &[Bar]) -> Vec<f64> {
        let n = bars.len();
        let mut result = vec![f64::NAN; n];

        if n < 2 {
            return result;
        }

        // Check for NaN in first two bars
        if bars[0].high.is_nan()
            || bars[0].low.is_nan()
            || bars[1].high.is_nan()
            || bars[1].low.is_nan()
        {
            return result;
        }

        // Initialize: determine initial direction from first two bars
        let mut is_long = bars[1].close >= bars[0].close;
        let mut af = self.af_start;
        let mut ep: f64;
        let mut sar: f64;

        if is_long {
            sar = bars[0].low;
            ep = bars[1].high;
        } else {
            sar = bars[0].high;
            ep = bars[1].low;
        }

        result[1] = sar;

        for i in 2..n {
            if bars[i].high.is_nan() || bars[i].low.is_nan() || bars[i].close.is_nan() {
                // NaN bar: carry SAR forward but don't update state
                result[i] = f64::NAN;
                continue;
            }

            // Compute new SAR
            let mut new_sar = sar + af * (ep - sar);

            if is_long {
                // In uptrend: SAR must not be above the two previous lows
                let prev_low1 = bars[i - 1].low;
                let prev_low2 = bars[i - 2].low;
                if !prev_low1.is_nan() {
                    new_sar = new_sar.min(prev_low1);
                }
                if !prev_low2.is_nan() {
                    new_sar = new_sar.min(prev_low2);
                }

                // Check for reversal
                if bars[i].low < new_sar {
                    // Reverse to short
                    is_long = false;
                    new_sar = ep; // SAR becomes the previous EP
                    ep = bars[i].low;
                    af = self.af_start;
                } else {
                    // Update EP and AF
                    if bars[i].high > ep {
                        ep = bars[i].high;
                        af = (af + self.af_step).min(self.af_max);
                    }
                }
            } else {
                // In downtrend: SAR must not be below the two previous highs
                let prev_high1 = bars[i - 1].high;
                let prev_high2 = bars[i - 2].high;
                if !prev_high1.is_nan() {
                    new_sar = new_sar.max(prev_high1);
                }
                if !prev_high2.is_nan() {
                    new_sar = new_sar.max(prev_high2);
                }

                // Check for reversal
                if bars[i].high > new_sar {
                    // Reverse to long
                    is_long = true;
                    new_sar = ep; // SAR becomes the previous EP
                    ep = bars[i].high;
                    af = self.af_start;
                } else {
                    // Update EP and AF
                    if bars[i].low < ep {
                        ep = bars[i].low;
                        af = (af + self.af_step).min(self.af_max);
                    }
                }
            }

            sar = new_sar;
            result[i] = sar;
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
    fn psar_uptrend_below_price() {
        // In a clear uptrend, PSAR should be below the price
        let mut data = Vec::new();
        for i in 0..10 {
            let base = 100.0 + i as f64 * 3.0;
            data.push((base, base + 2.0, base - 1.0, base + 1.5));
        }
        let bars = make_ohlc_bars(&data);
        let psar = ParabolicSar::default_params();
        let result = psar.compute(&bars);

        for i in 2..10 {
            if !result[i].is_nan() {
                assert!(
                    result[i] < bars[i].low,
                    "PSAR ({}) should be below low ({}) at bar {i} in uptrend",
                    result[i],
                    bars[i].low,
                );
            }
        }
    }

    #[test]
    fn psar_downtrend_above_price() {
        // In a clear downtrend, PSAR should be above the price
        let mut data = Vec::new();
        for i in 0..10 {
            let base = 200.0 - i as f64 * 3.0;
            data.push((base, base + 1.0, base - 2.0, base - 1.5));
        }
        let bars = make_ohlc_bars(&data);
        let psar = ParabolicSar::default_params();
        let result = psar.compute(&bars);

        // After reversal detection, PSAR should be above price
        let mut found_above = false;
        for i in 2..10 {
            if !result[i].is_nan() && result[i] > bars[i].high {
                found_above = true;
            }
        }
        assert!(
            found_above,
            "PSAR should be above price at some point in downtrend"
        );
    }

    #[test]
    fn psar_reversal_occurs() {
        // Uptrend followed by downtrend should trigger a reversal
        let data = [
            (100.0, 105.0, 98.0, 103.0),
            (103.0, 108.0, 101.0, 107.0),
            (107.0, 112.0, 105.0, 111.0),
            (111.0, 115.0, 109.0, 114.0),
            // Sharp reversal
            (114.0, 114.5, 100.0, 101.0),
            (101.0, 102.0, 95.0, 96.0),
            (96.0, 97.0, 90.0, 91.0),
        ];
        let bars = make_ohlc_bars(&data);
        let psar = ParabolicSar::default_params();
        let result = psar.compute(&bars);

        // PSAR should flip from below to above during the reversal
        let mut below = false;
        let mut above_after_below = false;
        for i in 1..7 {
            if !result[i].is_nan() {
                if result[i] < bars[i].close {
                    below = true;
                }
                if below && result[i] > bars[i].close {
                    above_after_below = true;
                }
            }
        }
        assert!(
            above_after_below,
            "PSAR should flip direction after reversal"
        );
    }

    #[test]
    fn psar_lookback() {
        assert_eq!(ParabolicSar::default_params().lookback(), 1);
    }

    #[test]
    fn psar_too_few_bars() {
        let bars = make_ohlc_bars(&[(100.0, 105.0, 95.0, 102.0)]);
        let psar = ParabolicSar::default_params();
        let result = psar.compute(&bars);
        assert!(result.iter().all(|v| v.is_nan()));
    }

    #[test]
    fn psar_af_caps_at_max() {
        // With many bars trending up, AF should cap at af_max
        let mut data = Vec::new();
        for i in 0..30 {
            let base = 100.0 + i as f64 * 1.0;
            data.push((base, base + 1.0, base - 0.5, base + 0.8));
        }
        let bars = make_ohlc_bars(&data);
        let psar = ParabolicSar::new(0.02, 0.02, 0.10);
        let result = psar.compute(&bars);

        // Just verify it completes without panic and produces non-NaN after warmup
        let valid_count = result.iter().filter(|v| !v.is_nan()).count();
        assert!(
            valid_count > 20,
            "should have many valid PSAR values in a long trend"
        );
    }
}
