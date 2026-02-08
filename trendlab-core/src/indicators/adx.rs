//! ADX — Average Directional Index (Wilder).
//!
//! Steps:
//! 1. Compute +DM and -DM from consecutive bars
//! 2. Smooth +DM, -DM, and TR using Wilder smoothing (alpha = 1/period)
//! 3. +DI = 100 * smoothed(+DM) / smoothed(TR)
//! 4. -DI = 100 * smoothed(-DM) / smoothed(TR)
//! 5. DX = 100 * |+DI - -DI| / (+DI + -DI)
//! 6. ADX = Wilder-smoothed DX
//!
//! Lookback: 2 * period (period for DI smoothing, then period for ADX smoothing).

use crate::components::indicator::Indicator;
use crate::domain::Bar;
use crate::indicators::atr::{true_range, wilder_smooth};

#[derive(Debug, Clone)]
pub struct Adx {
    period: usize,
    name: String,
}

impl Adx {
    pub fn new(period: usize) -> Self {
        assert!(period >= 1, "ADX period must be >= 1");
        Self {
            period,
            name: format!("adx_{period}"),
        }
    }
}

impl Indicator for Adx {
    fn name(&self) -> &str {
        &self.name
    }

    fn lookback(&self) -> usize {
        2 * self.period
    }

    fn compute(&self, bars: &[Bar]) -> Vec<f64> {
        let n = bars.len();
        let result = vec![f64::NAN; n];

        if n < 2 {
            return result;
        }

        // Step 1: Compute +DM and -DM
        let mut plus_dm = vec![f64::NAN; n];
        let mut minus_dm = vec![f64::NAN; n];

        for i in 1..n {
            let high_diff = bars[i].high - bars[i - 1].high;
            let low_diff = bars[i - 1].low - bars[i].low;

            if bars[i].high.is_nan()
                || bars[i].low.is_nan()
                || bars[i - 1].high.is_nan()
                || bars[i - 1].low.is_nan()
            {
                plus_dm[i] = f64::NAN;
                minus_dm[i] = f64::NAN;
                continue;
            }

            if high_diff > low_diff && high_diff > 0.0 {
                plus_dm[i] = high_diff;
            } else {
                plus_dm[i] = 0.0;
            }

            if low_diff > high_diff && low_diff > 0.0 {
                minus_dm[i] = low_diff;
            } else {
                minus_dm[i] = 0.0;
            }
        }

        // Step 2: Wilder smooth +DM, -DM, and TR
        let tr = true_range(bars);
        let smooth_tr = wilder_smooth(&tr, self.period);
        let smooth_plus_dm = wilder_smooth(&plus_dm, self.period);
        let smooth_minus_dm = wilder_smooth(&minus_dm, self.period);

        // Step 3-4: Compute +DI and -DI, then DX
        let mut dx = vec![f64::NAN; n];
        for i in 0..n {
            if smooth_tr[i].is_nan()
                || smooth_plus_dm[i].is_nan()
                || smooth_minus_dm[i].is_nan()
                || smooth_tr[i] == 0.0
            {
                continue;
            }

            let plus_di = 100.0 * smooth_plus_dm[i] / smooth_tr[i];
            let minus_di = 100.0 * smooth_minus_dm[i] / smooth_tr[i];
            let di_sum = plus_di + minus_di;

            if di_sum == 0.0 {
                dx[i] = 0.0;
            } else {
                dx[i] = 100.0 * (plus_di - minus_di).abs() / di_sum;
            }
        }

        // Step 5-6: Wilder smooth DX to get ADX
        wilder_smooth(&dx, self.period)
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
    fn adx_bounds() {
        // ADX should be between 0 and 100
        let bars = make_ohlc_bars(&[
            (100.0, 105.0, 95.0, 102.0),
            (102.0, 108.0, 100.0, 106.0),
            (106.0, 107.0, 98.0, 99.0),
            (99.0, 103.0, 97.0, 101.0),
            (101.0, 106.0, 100.0, 105.0),
            (105.0, 110.0, 103.0, 108.0),
            (108.0, 112.0, 106.0, 110.0),
            (110.0, 111.0, 104.0, 105.0),
            (105.0, 109.0, 103.0, 107.0),
            (107.0, 113.0, 105.0, 112.0),
        ]);
        let adx = Adx::new(3);
        let result = adx.compute(&bars);

        for (i, &v) in result.iter().enumerate() {
            if !v.is_nan() {
                assert!(v >= 0.0 && v <= 100.0, "ADX out of bounds at bar {i}: {v}");
            }
        }
    }

    #[test]
    fn adx_strong_trend_higher() {
        // Strong uptrend: ADX should increase
        let mut data = Vec::new();
        for i in 0..20 {
            let base = 100.0 + i as f64 * 5.0; // strong trend
            data.push((base - 1.0, base + 3.0, base - 3.0, base + 2.0));
        }
        let bars = make_ohlc_bars(&data);
        let adx = Adx::new(5);
        let result = adx.compute(&bars);

        // Find the last non-NaN value
        let last = result.iter().rev().find(|v| !v.is_nan());
        assert!(last.is_some());
        // In a strong trend, ADX should be elevated (typically > 20)
        // This is a soft check since exact values depend on the specific sequence
        if let Some(&v) = last {
            assert!(v > 10.0, "ADX should be elevated in strong trend, got {v}");
        }
    }

    #[test]
    fn adx_lookback() {
        assert_eq!(Adx::new(14).lookback(), 28);
        assert_eq!(Adx::new(7).lookback(), 14);
    }

    #[test]
    fn adx_too_few_bars() {
        let bars = make_ohlc_bars(&[(100.0, 105.0, 95.0, 102.0)]);
        let adx = Adx::new(3);
        let result = adx.compute(&bars);
        assert!(result.iter().all(|v| v.is_nan()));
    }
}
