//! Aroon — measures time since highest high and lowest low as a percentage.
//!
//! Aroon Up = 100 * (period - bars_since_highest_high) / period
//! Aroon Down = 100 * (period - bars_since_lowest_low) / period
//! Two bands (separate Indicator instances).
//! Lookback: period.

use crate::components::indicator::Indicator;
use crate::domain::Bar;

/// Which band of the Aroon oscillator to compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AroonBand {
    Up,
    Down,
}

#[derive(Debug, Clone)]
pub struct Aroon {
    period: usize,
    band: AroonBand,
    name: String,
}

impl Aroon {
    pub fn up(period: usize) -> Self {
        assert!(period >= 1, "Aroon period must be >= 1");
        Self {
            period,
            band: AroonBand::Up,
            name: format!("aroon_up_{period}"),
        }
    }

    pub fn down(period: usize) -> Self {
        assert!(period >= 1, "Aroon period must be >= 1");
        Self {
            period,
            band: AroonBand::Down,
            name: format!("aroon_down_{period}"),
        }
    }
}

impl Indicator for Aroon {
    fn name(&self) -> &str {
        &self.name
    }

    fn lookback(&self) -> usize {
        self.period
    }

    fn compute(&self, bars: &[Bar]) -> Vec<f64> {
        let n = bars.len();
        let mut result = vec![f64::NAN; n];

        if n <= self.period {
            return result;
        }

        for i in self.period..n {
            let start = i - self.period;
            let window = &bars[start..=i];

            // Check for NaN
            let has_nan = match self.band {
                AroonBand::Up => window.iter().any(|b| b.high.is_nan()),
                AroonBand::Down => window.iter().any(|b| b.low.is_nan()),
            };

            if has_nan {
                result[i] = f64::NAN;
                continue;
            }

            match self.band {
                AroonBand::Up => {
                    // Find the index of the highest high in the window
                    // (most recent if tied)
                    let mut max_val = f64::NEG_INFINITY;
                    let mut max_offset = 0;
                    for (j, bar) in window.iter().enumerate() {
                        if bar.high >= max_val {
                            max_val = bar.high;
                            max_offset = j;
                        }
                    }
                    let bars_since = self.period - max_offset;
                    result[i] = 100.0 * (self.period - bars_since) as f64 / self.period as f64;
                }
                AroonBand::Down => {
                    // Find the index of the lowest low in the window
                    // (most recent if tied)
                    let mut min_val = f64::INFINITY;
                    let mut min_offset = 0;
                    for (j, bar) in window.iter().enumerate() {
                        if bar.low <= min_val {
                            min_val = bar.low;
                            min_offset = j;
                        }
                    }
                    let bars_since = self.period - min_offset;
                    result[i] = 100.0 * (self.period - bars_since) as f64 / self.period as f64;
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
    fn aroon_up_highest_at_end() {
        // Period=3, window=[bars[0..=3]]
        // Highs: 10, 11, 12, 13 (highest at index 3, the last bar)
        // bars_since = 3 - 3 = 0
        // aroon_up = 100 * (3 - 0) / 3 = 100
        let bars = make_ohlc_bars(&[
            (9.0, 10.0, 8.0, 9.5),
            (9.5, 11.0, 9.0, 10.5),
            (10.5, 12.0, 10.0, 11.5),
            (11.5, 13.0, 11.0, 12.5),
        ]);
        let aroon = Aroon::up(3);
        let result = aroon.compute(&bars);
        assert_approx(result[3], 100.0, DEFAULT_EPSILON);
    }

    #[test]
    fn aroon_up_highest_at_start() {
        // Period=3, window=[bars[0..=3]]
        // Highs: 20, 11, 12, 13 (highest at index 0)
        // bars_since = 3 - 0 = 3
        // aroon_up = 100 * (3 - 3) / 3 = 0
        let bars = make_ohlc_bars(&[
            (19.0, 20.0, 18.0, 19.5),
            (9.5, 11.0, 9.0, 10.5),
            (10.5, 12.0, 10.0, 11.5),
            (11.5, 13.0, 11.0, 12.5),
        ]);
        let aroon = Aroon::up(3);
        let result = aroon.compute(&bars);
        assert_approx(result[3], 0.0, DEFAULT_EPSILON);
    }

    #[test]
    fn aroon_down_lowest_at_end() {
        // Period=3, lowest at end → aroon_down = 100
        let bars = make_ohlc_bars(&[
            (9.0, 10.0, 8.0, 9.5),
            (9.5, 11.0, 7.0, 10.5),
            (10.5, 12.0, 6.0, 11.5),
            (11.5, 13.0, 5.0, 12.5),
        ]);
        let aroon = Aroon::down(3);
        let result = aroon.compute(&bars);
        assert_approx(result[3], 100.0, DEFAULT_EPSILON);
    }

    #[test]
    fn aroon_bounds() {
        let bars = make_ohlc_bars(&[
            (10.0, 15.0, 5.0, 12.0),
            (12.0, 14.0, 8.0, 10.0),
            (10.0, 16.0, 7.0, 13.0),
            (13.0, 13.5, 9.0, 11.0),
            (11.0, 17.0, 6.0, 14.0),
        ]);
        let aroon_up = Aroon::up(3);
        let aroon_down = Aroon::down(3);
        let up = aroon_up.compute(&bars);
        let down = aroon_down.compute(&bars);

        for i in 3..5 {
            assert!(
                (0.0..=100.0).contains(&up[i]),
                "Aroon Up out of bounds at {i}: {}",
                up[i]
            );
            assert!(
                (0.0..=100.0).contains(&down[i]),
                "Aroon Down out of bounds at {i}: {}",
                down[i]
            );
        }
    }

    #[test]
    fn aroon_nan_propagation() {
        let mut bars = make_ohlc_bars(&[
            (10.0, 15.0, 5.0, 12.0),
            (12.0, 14.0, 8.0, 10.0),
            (10.0, 16.0, 7.0, 13.0),
        ]);
        bars[1].high = f64::NAN;
        let aroon = Aroon::up(2);
        let result = aroon.compute(&bars);
        assert!(result[2].is_nan());
    }

    #[test]
    fn aroon_lookback() {
        assert_eq!(Aroon::up(25).lookback(), 25);
    }
}
