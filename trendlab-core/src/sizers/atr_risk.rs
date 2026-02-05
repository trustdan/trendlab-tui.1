//! ATR Risk Sizer
//!
//! Position size based on volatility (ATR) and fixed risk per trade.
//! Classic risk management: risk X% of equity per trade, with stop at Y * ATR.

use crate::domain::Bar;
use crate::signals::SignalIntent;
use crate::sizers::Sizer;

/// ATR-based risk sizer
///
/// # Formula
/// ```text
/// risk_dollars = equity * risk_pct
/// stop_distance = atr_multiplier * ATR
/// quantity = risk_dollars / stop_distance
/// ```
///
/// # Example
/// - Equity: $100,000
/// - Risk per trade: 1% ($1,000)
/// - ATR: $2.00
/// - ATR multiplier: 2x (stop at 2 * ATR = $4.00)
/// - Quantity: $1,000 / $4.00 = 250 shares
#[derive(Debug, Clone)]
pub struct AtrRiskSizer {
    /// Risk percentage per trade (e.g., 0.01 = 1%)
    risk_pct: f64,

    /// ATR multiplier for stop distance (e.g., 2.0 = 2x ATR)
    atr_multiplier: f64,

    /// ATR period (e.g., 14 bars)
    atr_period: usize,
}

impl AtrRiskSizer {
    pub fn new(risk_pct: f64, atr_multiplier: f64, atr_period: usize) -> Self {
        assert!(risk_pct > 0.0 && risk_pct < 1.0, "risk_pct must be in (0, 1)");
        assert!(atr_multiplier > 0.0, "atr_multiplier must be > 0");
        assert!(atr_period > 0, "atr_period must be > 0");

        Self {
            risk_pct,
            atr_multiplier,
            atr_period,
        }
    }

    /// Calculate True Range for a single bar
    fn true_range(&self, bar: &Bar, prev_close: Option<f64>) -> f64 {
        let high_low = bar.high - bar.low;

        match prev_close {
            Some(pc) => {
                let high_prev = (bar.high - pc).abs();
                let low_prev = (bar.low - pc).abs();
                high_low.max(high_prev).max(low_prev)
            }
            None => high_low,
        }
    }

    /// Calculate ATR from bar history
    ///
    /// Uses simple moving average of True Range.
    /// Returns None if insufficient bars.
    fn calculate_atr(&self, bars: &[Bar]) -> Option<f64> {
        if bars.len() < self.atr_period {
            return None;
        }

        let recent = &bars[bars.len() - self.atr_period..];
        let mut sum_tr = 0.0;

        for (i, bar) in recent.iter().enumerate() {
            let prev_close = if i > 0 { Some(recent[i - 1].close) } else { None };
            sum_tr += self.true_range(bar, prev_close);
        }

        let atr = sum_tr / self.atr_period as f64;
        Some(atr)
    }
}

impl Sizer for AtrRiskSizer {
    fn size(&self, equity: f64, intent: SignalIntent, bar: &Bar) -> f64 {
        // No position for flat intent
        if intent == SignalIntent::Flat {
            return 0.0;
        }

        // Insufficient equity
        if equity <= 0.0 {
            return 0.0;
        }

        // Need bar history for ATR (pass via Bar's symbol context in real impl)
        // For now, use a simple ATR estimate from current bar's range
        // In production, this would be calculated from historical bars
        let atr_estimate = bar.range(); // Placeholder: use bar range as ATR proxy

        if atr_estimate <= 0.0 {
            return 0.0; // Can't size without volatility
        }

        let risk_dollars = equity * self.risk_pct;
        let stop_distance = self.atr_multiplier * atr_estimate;

        if stop_distance <= 0.0 {
            return 0.0;
        }

        risk_dollars / stop_distance
    }

    fn name(&self) -> &str {
        "AtrRisk"
    }
}

/// Extended sizer with bar history context
///
/// This variant accepts full bar history for proper ATR calculation.
/// Use this for real backtests; the base Sizer trait is simplified for composition.
impl AtrRiskSizer {
    pub fn size_with_history(&self, equity: f64, intent: SignalIntent, bars: &[Bar]) -> f64 {
        if intent == SignalIntent::Flat || equity <= 0.0 {
            return 0.0;
        }

        let atr = match self.calculate_atr(bars) {
            Some(a) => a,
            None => return 0.0, // Insufficient data
        };

        if atr <= 0.0 {
            return 0.0;
        }

        let risk_dollars = equity * self.risk_pct;
        let stop_distance = self.atr_multiplier * atr;

        risk_dollars / stop_distance
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64) -> Bar {
        Bar::new(Utc::now(), "SPY".into(), 100.0, high, low, close, 1000000.0)
    }

    #[test]
    fn test_atr_risk_sizer_basic() {
        let sizer = AtrRiskSizer::new(0.01, 2.0, 14);
        let bar = make_bar(102.0, 98.0, 100.0); // Range = 4.0

        // Equity: $100,000
        // Risk: 1% = $1,000
        // ATR estimate: 4.0 (bar range)
        // Stop distance: 2 * 4.0 = 8.0
        // Qty: $1,000 / $8.0 = 125 shares

        let qty = sizer.size(100000.0, SignalIntent::Long, &bar);
        assert_eq!(qty, 125.0);
    }

    #[test]
    fn test_atr_risk_sizer_with_history() {
        let sizer = AtrRiskSizer::new(0.01, 2.0, 3);

        let bars = vec![
            make_bar(103.0, 97.0, 100.0),  // TR = 6.0
            make_bar(105.0, 99.0, 102.0),  // TR = 6.0
            make_bar(104.0, 100.0, 101.0), // TR = 4.0
        ];

        // ATR = (6.0 + 6.0 + 4.0) / 3 = 5.33
        // Risk: 1% of $100k = $1,000
        // Stop distance: 2 * 5.33 = 10.66
        // Qty: $1,000 / 10.66 = ~93.8

        let qty = sizer.size_with_history(100000.0, SignalIntent::Long, &bars);
        assert!((qty - 93.8).abs() < 0.1);
    }

    #[test]
    fn test_flat_intent_returns_zero() {
        let sizer = AtrRiskSizer::new(0.01, 2.0, 14);
        let bar = make_bar(102.0, 98.0, 100.0);

        let qty = sizer.size(100000.0, SignalIntent::Flat, &bar);
        assert_eq!(qty, 0.0);
    }

    #[test]
    fn test_insufficient_bars_returns_zero() {
        let sizer = AtrRiskSizer::new(0.01, 2.0, 10);
        let bars = vec![make_bar(102.0, 98.0, 100.0)]; // Only 1 bar, need 10

        let qty = sizer.size_with_history(100000.0, SignalIntent::Long, &bars);
        assert_eq!(qty, 0.0);
    }
}
