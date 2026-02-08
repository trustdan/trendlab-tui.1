//! Concrete indicator implementations.
//!
//! All 13 indicators implement the `Indicator` trait from `components::indicator`.
//! They are precomputed once before the bar loop and fed per-bar into the event loop
//! via `IndicatorValues`.
//!
//! Multi-series indicators (Donchian, Bollinger, Keltner, Aroon) are exposed as
//! separate named instances per band, keeping the single-series `Indicator` trait
//! unchanged.

pub mod adx;
pub mod aroon;
pub mod atr;
pub mod bollinger;
pub mod donchian;
pub mod ema;
pub mod keltner;
pub mod momentum;
pub mod parabolic_sar;
pub mod roc;
pub mod rsi;
pub mod sma;
pub mod supertrend;

pub use adx::Adx;
pub use aroon::{Aroon, AroonBand};
pub use atr::Atr;
pub use bollinger::{Bollinger, BollingerBand};
pub use donchian::{Donchian, DonchianBand};
pub use ema::Ema;
pub use keltner::{Keltner, KeltnerBand};
pub use momentum::Momentum;
pub use parabolic_sar::ParabolicSar;
pub use roc::Roc;
pub use rsi::Rsi;
pub use sma::Sma;
pub use supertrend::Supertrend;

/// Create synthetic bars from close prices for testing.
///
/// Generates plausible OHLV: open = prev_close (or close for first bar),
/// high = max(open,close) + 1.0, low = min(open,close) - 1.0, volume = 1000.
#[cfg(test)]
pub fn make_bars(closes: &[f64]) -> Vec<crate::domain::Bar> {
    use crate::domain::Bar;
    let base_date = chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    closes
        .iter()
        .enumerate()
        .map(|(i, &close)| {
            let open = if i == 0 { close } else { closes[i - 1] };
            let high = open.max(close) + 1.0;
            let low = open.min(close) - 1.0;
            Bar {
                symbol: "TEST".to_string(),
                date: base_date + chrono::Duration::days(i as i64),
                open,
                high,
                low,
                close,
                volume: 1000,
                adj_close: close,
            }
        })
        .collect()
}

/// Assert two f64 values are approximately equal (within epsilon).
#[cfg(test)]
pub fn assert_approx(actual: f64, expected: f64, epsilon: f64) {
    assert!(
        (actual - expected).abs() < epsilon,
        "assert_approx failed: actual={actual}, expected={expected}, diff={}, epsilon={epsilon}",
        (actual - expected).abs()
    );
}

/// Default epsilon for indicator tests.
#[cfg(test)]
pub const DEFAULT_EPSILON: f64 = 1e-10;
