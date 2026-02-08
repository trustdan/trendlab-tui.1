//! Donchian Channel — highest high / lowest low over a lookback window.
//!
//! Produces two series (exposed as separate Indicator instances):
//! - Upper: max(high[t-period+1..=t])
//! - Lower: min(low[t-period+1..=t])
//!
//! Lookback: period - 1.

use crate::components::indicator::Indicator;
use crate::domain::Bar;

/// Which band of the Donchian channel to compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DonchianBand {
    Upper,
    Lower,
}

#[derive(Debug, Clone)]
pub struct Donchian {
    period: usize,
    band: DonchianBand,
    name: String,
}

impl Donchian {
    pub fn upper(period: usize) -> Self {
        assert!(period >= 1, "Donchian period must be >= 1");
        Self {
            period,
            band: DonchianBand::Upper,
            name: format!("donchian_upper_{period}"),
        }
    }

    pub fn lower(period: usize) -> Self {
        assert!(period >= 1, "Donchian period must be >= 1");
        Self {
            period,
            band: DonchianBand::Lower,
            name: format!("donchian_lower_{period}"),
        }
    }
}

impl Indicator for Donchian {
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

            match self.band {
                DonchianBand::Upper => {
                    let mut max_val = f64::NEG_INFINITY;
                    let mut has_nan = false;
                    for bar in window {
                        if bar.high.is_nan() {
                            has_nan = true;
                            break;
                        }
                        if bar.high > max_val {
                            max_val = bar.high;
                        }
                    }
                    result[i] = if has_nan { f64::NAN } else { max_val };
                }
                DonchianBand::Lower => {
                    let mut min_val = f64::INFINITY;
                    let mut has_nan = false;
                    for bar in window {
                        if bar.low.is_nan() {
                            has_nan = true;
                            break;
                        }
                        if bar.low < min_val {
                            min_val = bar.low;
                        }
                    }
                    result[i] = if has_nan { f64::NAN } else { min_val };
                }
            }
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
    fn donchian_upper_3() {
        let bars = make_ohlc_bars(&[
            (10.0, 12.0, 9.0, 11.0),
            (11.0, 15.0, 10.0, 14.0),
            (14.0, 14.0, 13.0, 13.5),
            (13.5, 16.0, 12.0, 15.0),
            (15.0, 15.5, 14.0, 14.5),
        ]);
        let dc = Donchian::upper(3);
        let result = dc.compute(&bars);

        assert!(result[0].is_nan());
        assert!(result[1].is_nan());
        // [2] = max(12, 15, 14) = 15
        assert_approx(result[2], 15.0, DEFAULT_EPSILON);
        // [3] = max(15, 14, 16) = 16
        assert_approx(result[3], 16.0, DEFAULT_EPSILON);
        // [4] = max(14, 16, 15.5) = 16
        assert_approx(result[4], 16.0, DEFAULT_EPSILON);
    }

    #[test]
    fn donchian_lower_3() {
        let bars = make_ohlc_bars(&[
            (10.0, 12.0, 9.0, 11.0),
            (11.0, 15.0, 10.0, 14.0),
            (14.0, 14.0, 13.0, 13.5),
            (13.5, 16.0, 12.0, 15.0),
            (15.0, 15.5, 14.0, 14.5),
        ]);
        let dc = Donchian::lower(3);
        let result = dc.compute(&bars);

        assert!(result[0].is_nan());
        assert!(result[1].is_nan());
        // [2] = min(9, 10, 13) = 9
        assert_approx(result[2], 9.0, DEFAULT_EPSILON);
        // [3] = min(10, 13, 12) = 10
        assert_approx(result[3], 10.0, DEFAULT_EPSILON);
        // [4] = min(13, 12, 14) = 12
        assert_approx(result[4], 12.0, DEFAULT_EPSILON);
    }

    #[test]
    fn donchian_nan_propagation() {
        let mut bars = make_ohlc_bars(&[
            (10.0, 12.0, 9.0, 11.0),
            (11.0, 15.0, 10.0, 14.0),
            (14.0, 14.0, 13.0, 13.5),
        ]);
        bars[1].high = f64::NAN;
        bars[1].low = f64::NAN;

        let upper = Donchian::upper(3);
        let lower = Donchian::lower(3);
        assert!(upper.compute(&bars)[2].is_nan());
        assert!(lower.compute(&bars)[2].is_nan());
    }

    #[test]
    fn donchian_lookback() {
        assert_eq!(Donchian::upper(20).lookback(), 19);
        assert_eq!(Donchian::lower(1).lookback(), 0);
    }
}
