//! Momentum — simple lookback return (difference, not percentage).
//!
//! momentum[t] = close[t] - close[t-period]
//! Lookback: period.

use crate::components::indicator::Indicator;
use crate::domain::Bar;

#[derive(Debug, Clone)]
pub struct Momentum {
    period: usize,
    name: String,
}

impl Momentum {
    pub fn new(period: usize) -> Self {
        assert!(period >= 1, "Momentum period must be >= 1");
        Self {
            period,
            name: format!("momentum_{period}"),
        }
    }
}

impl Indicator for Momentum {
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
            if prev.is_nan() || curr.is_nan() {
                result[i] = f64::NAN;
            } else {
                result[i] = curr - prev;
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
    fn momentum_basic() {
        let bars = make_bars(&[100.0, 110.0, 105.0, 115.0]);
        let mom = Momentum::new(2);
        let result = mom.compute(&bars);

        assert!(result[0].is_nan());
        assert!(result[1].is_nan());
        // momentum[2] = 105 - 100 = 5
        assert_approx(result[2], 5.0, DEFAULT_EPSILON);
        // momentum[3] = 115 - 110 = 5
        assert_approx(result[3], 5.0, DEFAULT_EPSILON);
    }

    #[test]
    fn momentum_negative() {
        let bars = make_bars(&[100.0, 90.0]);
        let mom = Momentum::new(1);
        let result = mom.compute(&bars);
        assert_approx(result[1], -10.0, DEFAULT_EPSILON);
    }

    #[test]
    fn momentum_nan_propagation() {
        let mut bars = make_bars(&[100.0, 110.0, 120.0]);
        bars[1].close = f64::NAN;
        let mom = Momentum::new(1);
        let result = mom.compute(&bars);
        assert!(result[1].is_nan());
        assert!(result[2].is_nan());
    }

    #[test]
    fn momentum_lookback() {
        assert_eq!(Momentum::new(20).lookback(), 20);
    }
}
