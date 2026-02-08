//! Rate of Change (ROC).
//!
//! Percentage price change over N bars.
//! ROC[t] = (close[t] - close[t-period]) / close[t-period] * 100
//! Lookback: period.

use crate::components::indicator::Indicator;
use crate::domain::Bar;

#[derive(Debug, Clone)]
pub struct Roc {
    period: usize,
    name: String,
}

impl Roc {
    pub fn new(period: usize) -> Self {
        assert!(period >= 1, "ROC period must be >= 1");
        Self {
            period,
            name: format!("roc_{period}"),
        }
    }
}

impl Indicator for Roc {
    fn name(&self) -> &str {
        &self.name
    }

    fn lookback(&self) -> usize {
        self.period
    }

    fn compute(&self, bars: &[Bar]) -> Vec<f64> {
        let n = bars.len();
        let mut result = vec![f64::NAN; n];

        for i in self.period..n {
            let prev = bars[i - self.period].close;
            let curr = bars[i].close;
            if prev.is_nan() || curr.is_nan() || prev == 0.0 {
                result[i] = f64::NAN;
            } else {
                result[i] = (curr - prev) / prev * 100.0;
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
    fn roc_basic() {
        // Closes: 100, 110, 121
        // ROC[1] with period=1: (110-100)/100*100 = 10%
        // ROC[2] with period=1: (121-110)/110*100 = 10%
        let bars = make_bars(&[100.0, 110.0, 121.0]);
        let roc = Roc::new(1);
        let result = roc.compute(&bars);

        assert!(result[0].is_nan());
        assert_approx(result[1], 10.0, DEFAULT_EPSILON);
        assert_approx(result[2], 10.0, DEFAULT_EPSILON);
    }

    #[test]
    fn roc_period_2() {
        // Closes: 100, 110, 121
        // ROC[2] with period=2: (121-100)/100*100 = 21%
        let bars = make_bars(&[100.0, 110.0, 121.0]);
        let roc = Roc::new(2);
        let result = roc.compute(&bars);

        assert!(result[0].is_nan());
        assert!(result[1].is_nan());
        assert_approx(result[2], 21.0, DEFAULT_EPSILON);
    }

    #[test]
    fn roc_negative() {
        let bars = make_bars(&[100.0, 90.0]);
        let roc = Roc::new(1);
        let result = roc.compute(&bars);
        assert_approx(result[1], -10.0, DEFAULT_EPSILON);
    }

    #[test]
    fn roc_nan_propagation() {
        let mut bars = make_bars(&[100.0, 110.0, 120.0]);
        bars[1].close = f64::NAN;
        let roc = Roc::new(1);
        let result = roc.compute(&bars);
        assert!(result[1].is_nan()); // curr NaN
        assert!(result[2].is_nan()); // prev NaN
    }

    #[test]
    fn roc_lookback() {
        assert_eq!(Roc::new(14).lookback(), 14);
    }
}
