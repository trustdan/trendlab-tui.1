//! Donchian Breakout signal
//!
//! Classic breakout/momentum signal:
//! - Long when price breaks above N-day high
//! - Short when price breaks below N-day low
//! - Flat otherwise

use crate::domain::Bar;
use crate::signals::{Signal, SignalFamily, SignalIntent};

/// Donchian Channel Breakout signal
///
/// # Parameters
/// - `period`: Lookback period for channel (e.g., 20)
///
/// # Signal Logic
/// - Long: Close breaks above highest high of past N bars
/// - Short: Close breaks below lowest low of past N bars
/// - Flat: No breakout
#[derive(Debug, Clone)]
pub struct DonchianBreakout {
    period: usize,
}

impl DonchianBreakout {
    pub fn new(period: usize) -> Self {
        assert!(period > 0, "period must be > 0");
        Self { period }
    }

    /// Calculate highest high over lookback period
    fn highest_high(&self, bars: &[Bar]) -> Option<f64> {
        if bars.len() < self.period {
            return None;
        }

        let lookback = &bars[bars.len() - self.period..];
        lookback
            .iter()
            .map(|b| b.high)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
    }

    /// Calculate lowest low over lookback period
    fn lowest_low(&self, bars: &[Bar]) -> Option<f64> {
        if bars.len() < self.period {
            return None;
        }

        let lookback = &bars[bars.len() - self.period..];
        lookback
            .iter()
            .map(|b| b.low)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
    }

    /// Get current bar close
    fn current_close(&self, bars: &[Bar]) -> Option<f64> {
        bars.last().map(|b| b.close)
    }
}

impl Signal for DonchianBreakout {
    fn generate(&self, bars: &[Bar]) -> SignalIntent {
        if bars.len() < self.period + 1 {
            return SignalIntent::Flat; // Not enough data
        }

        // Get previous N bars (excluding current bar for channel calculation)
        let prev_bars = &bars[..bars.len() - 1];
        let current_close = match self.current_close(bars) {
            Some(c) => c,
            None => return SignalIntent::Flat,
        };

        let high = match self.highest_high(prev_bars) {
            Some(h) => h,
            None => return SignalIntent::Flat,
        };

        let low = match self.lowest_low(prev_bars) {
            Some(l) => l,
            None => return SignalIntent::Flat,
        };

        // Breakout detection
        if current_close > high {
            SignalIntent::Long // Upside breakout
        } else if current_close < low {
            SignalIntent::Short // Downside breakout
        } else {
            SignalIntent::Flat // Inside channel
        }
    }

    fn name(&self) -> &str {
        "Donchian"
    }

    fn max_lookback(&self) -> usize {
        self.period + 1 // Need period bars + current bar
    }

    fn signal_family(&self) -> SignalFamily {
        SignalFamily::Breakout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64) -> Bar {
        Bar::new(
            Utc::now(),
            "SPY".into(),
            (high + low) / 2.0,
            high,
            low,
            close,
            1000000.0,
        )
    }

    #[test]
    fn test_donchian_upside_breakout() {
        let signal = DonchianBreakout::new(3);

        let bars = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(103.0, 99.0, 101.0),
            make_bar(104.0, 100.0, 102.0),
            make_bar(110.0, 105.0, 108.0), // Close above previous 3-bar high
        ];

        let intent = signal.generate(&bars);
        assert_eq!(intent, SignalIntent::Long);
    }

    #[test]
    fn test_donchian_downside_breakout() {
        let signal = DonchianBreakout::new(3);

        let bars = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(103.0, 99.0, 101.0),
            make_bar(104.0, 100.0, 102.0),
            make_bar(97.0, 92.0, 94.0), // Close below previous 3-bar low
        ];

        let intent = signal.generate(&bars);
        assert_eq!(intent, SignalIntent::Short);
    }

    #[test]
    fn test_donchian_inside_channel() {
        let signal = DonchianBreakout::new(3);

        let bars = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(103.0, 99.0, 101.0),
            make_bar(104.0, 100.0, 102.0),
            make_bar(103.0, 99.0, 101.0), // Inside previous range
        ];

        let intent = signal.generate(&bars);
        assert_eq!(intent, SignalIntent::Flat);
    }

    #[test]
    fn test_donchian_insufficient_data() {
        let signal = DonchianBreakout::new(20);

        let bars = vec![make_bar(100.0, 98.0, 99.0), make_bar(101.0, 99.0, 100.0)];

        let intent = signal.generate(&bars);
        assert_eq!(intent, SignalIntent::Flat);
    }

    #[test]
    fn test_max_lookback() {
        let signal = DonchianBreakout::new(20);
        assert_eq!(signal.max_lookback(), 21); // period + 1
    }

    #[test]
    fn test_signal_family() {
        let signal = DonchianBreakout::new(20);
        assert_eq!(signal.signal_family(), SignalFamily::Breakout);
    }
}
