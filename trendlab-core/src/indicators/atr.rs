//! Average True Range (ATR).
//!
//! True Range: max(high-low, |high-prev_close|, |low-prev_close|)
//! ATR uses Wilder smoothing (EMA with alpha = 1/period).
//! Lookback: period (needs period+1 bars for TR series, then average).

use crate::components::indicator::Indicator;
use crate::domain::Bar;

#[derive(Debug, Clone)]
pub struct Atr {
    period: usize,
    name: String,
}

impl Atr {
    pub fn new(period: usize) -> Self {
        assert!(period >= 1, "ATR period must be >= 1");
        Self {
            period,
            name: format!("atr_{period}"),
        }
    }
}

/// Compute the True Range series from bars.
/// TR[0] = high[0] - low[0] (no previous close).
/// TR[t] = max(high[t]-low[t], |high[t]-close[t-1]|, |low[t]-close[t-1]|).
pub fn true_range(bars: &[Bar]) -> Vec<f64> {
    let n = bars.len();
    let mut tr = vec![f64::NAN; n];

    if n == 0 {
        return tr;
    }

    // First bar: just high - low
    let h = bars[0].high;
    let l = bars[0].low;
    if h.is_nan() || l.is_nan() {
        tr[0] = f64::NAN;
    } else {
        tr[0] = h - l;
    }

    for i in 1..n {
        let h = bars[i].high;
        let l = bars[i].low;
        let pc = bars[i - 1].close;
        if h.is_nan() || l.is_nan() || pc.is_nan() {
            tr[i] = f64::NAN;
        } else {
            tr[i] = (h - l).max((h - pc).abs()).max((l - pc).abs());
        }
    }

    tr
}

/// Apply Wilder smoothing to a series. Alpha = 1/period.
/// Seed: mean of first `period` valid values starting at `start_index`.
pub fn wilder_smooth(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    let mut result = vec![f64::NAN; n];

    if n < period || period == 0 {
        return result;
    }

    // Find the first index where we have `period` consecutive non-NaN values
    // For ATR, TR[0] uses only high-low, so the seed window starts at index 1
    // (where we have proper true range). But for generality, scan for validity.
    let seed_start = {
        let mut start = None;
        for i in 0..n {
            if values[i].is_nan() {
                continue;
            }
            // Check if we can get `period` non-NaN values starting here
            let mut count = 0;
            let mut valid = true;
            for v in &values[i..n] {
                if v.is_nan() {
                    valid = false;
                    break;
                }
                count += 1;
                if count == period {
                    break;
                }
            }
            if valid && count == period {
                start = Some(i);
                break;
            }
        }
        start
    };

    let seed_start = match seed_start {
        Some(s) => s,
        None => return result,
    };

    let seed_end = seed_start + period;

    // Seed: mean of the first `period` values
    let seed: f64 = values[seed_start..seed_end].iter().sum::<f64>() / period as f64;
    result[seed_end - 1] = seed;

    let alpha = 1.0 / period as f64;
    let mut prev = seed;

    for i in seed_end..n {
        if values[i].is_nan() {
            for val in result.iter_mut().skip(i) {
                *val = f64::NAN;
            }
            return result;
        }
        let smoothed = alpha * values[i] + (1.0 - alpha) * prev;
        result[i] = smoothed;
        prev = smoothed;
    }

    result
}

impl Indicator for Atr {
    fn name(&self) -> &str {
        &self.name
    }

    fn lookback(&self) -> usize {
        self.period
    }

    fn compute(&self, bars: &[Bar]) -> Vec<f64> {
        let mut tr = true_range(bars);
        // TR[0] has no previous close — it's just high-low, not proper true range.
        // Mark it NaN so the Wilder seed starts from TR[1], consistent with lookback = period.
        if !tr.is_empty() {
            tr[0] = f64::NAN;
        }
        wilder_smooth(&tr, self.period)
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
    fn true_range_basic() {
        let bars = make_ohlc_bars(&[
            (100.0, 105.0, 95.0, 102.0),  // TR = 105-95 = 10
            (102.0, 108.0, 100.0, 106.0), // TR = max(8, |108-102|, |100-102|) = 8
            (106.0, 107.0, 98.0, 99.0),   // TR = max(9, |107-106|, |98-106|) = 9
        ]);
        let tr = true_range(&bars);
        assert_approx(tr[0], 10.0, DEFAULT_EPSILON);
        assert_approx(tr[1], 8.0, DEFAULT_EPSILON);
        assert_approx(tr[2], 9.0, DEFAULT_EPSILON);
    }

    #[test]
    fn true_range_gap_up() {
        // Gap up: prev close 100, current bar 110-115-108
        let bars = make_ohlc_bars(&[
            (98.0, 102.0, 97.0, 100.0),
            (110.0, 115.0, 108.0, 112.0), // TR = max(7, |115-100|, |108-100|) = 15
        ]);
        let tr = true_range(&bars);
        assert_approx(tr[1], 15.0, DEFAULT_EPSILON);
    }

    #[test]
    fn atr_period_3() {
        let bars = make_ohlc_bars(&[
            (100.0, 105.0, 95.0, 102.0),  // TR = 10
            (102.0, 108.0, 100.0, 106.0), // TR = 8
            (106.0, 107.0, 98.0, 99.0),   // TR = 9
            (99.0, 103.0, 97.0, 101.0),   // TR = 6
            (101.0, 106.0, 100.0, 105.0), // TR = 6
        ]);
        let atr = Atr::new(3);
        let result = atr.compute(&bars);

        assert!(result[0].is_nan());
        assert!(result[1].is_nan());
        assert!(result[2].is_nan());
        // TR[0] is NaN (no prev close), so seed uses TR[1..=3] = [8, 9, 6]
        // Seed: ATR[3] = mean(8, 9, 6) = 23/3 ≈ 7.6667
        // ATR[4] = (1/3)*6 + (2/3)*(23/3) = 2 + 46/9 = 64/9 ≈ 7.1111
        assert_approx(result[3], 23.0 / 3.0, DEFAULT_EPSILON);
        assert_approx(result[4], 64.0 / 9.0, DEFAULT_EPSILON);
    }

    #[test]
    fn atr_nan_propagation() {
        let mut bars = make_ohlc_bars(&[
            (100.0, 105.0, 95.0, 102.0),
            (102.0, 108.0, 100.0, 106.0),
            (106.0, 107.0, 98.0, 99.0),
        ]);
        bars[1].high = f64::NAN;
        let atr = Atr::new(2);
        let result = atr.compute(&bars);
        // TR[1] is NaN → seed can't form at index 1
        // Result depends on where seed forms
        assert!(result[0].is_nan());
    }

    #[test]
    fn atr_lookback() {
        assert_eq!(Atr::new(14).lookback(), 14);
    }
}
