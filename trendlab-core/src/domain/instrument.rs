//! Instrument metadata and tick rounding.

use serde::{Deserialize, Serialize};

/// Asset classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetClass {
    Equity,
    Etf,
    Future,
    Option,
}

/// Instrument metadata for a tradable symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instrument {
    pub symbol: String,
    pub tick_size: f64,
    pub lot_size: f64,
    pub currency: String,
    pub asset_class: AssetClass,
}

impl Instrument {
    /// Default US equity: 0.01 tick, 1-share lot, USD.
    pub fn us_equity(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size: 0.01,
            lot_size: 1.0,
            currency: "USD".into(),
            asset_class: AssetClass::Equity,
        }
    }

    /// Default US ETF: 0.01 tick, 1-share lot, USD.
    pub fn us_etf(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            tick_size: 0.01,
            lot_size: 1.0,
            currency: "USD".into(),
            asset_class: AssetClass::Etf,
        }
    }
}

/// Side-aware tick rounding.
///
/// Buy orders round UP to the next tick (pay more, ensures fill).
/// Sell orders round DOWN to the previous tick (receive less, ensures fill).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

pub fn round_to_tick(price: f64, tick_size: f64, side: OrderSide) -> f64 {
    if tick_size <= 0.0 || price.is_nan() {
        return price;
    }
    match side {
        OrderSide::Buy => (price / tick_size).ceil() * tick_size,
        OrderSide::Sell => (price / tick_size).floor() * tick_size,
    }
}

/// Round quantity down to the nearest lot size.
pub fn round_to_lot(quantity: f64, lot_size: f64) -> f64 {
    if lot_size <= 0.0 {
        return quantity;
    }
    (quantity / lot_size).floor() * lot_size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buy_rounds_up() {
        assert_eq!(round_to_tick(100.013, 0.01, OrderSide::Buy), 100.02);
    }

    #[test]
    fn sell_rounds_down() {
        assert_eq!(round_to_tick(100.017, 0.01, OrderSide::Sell), 100.01);
    }

    #[test]
    fn exact_tick_is_unchanged() {
        assert_eq!(round_to_tick(100.05, 0.01, OrderSide::Buy), 100.05);
        assert_eq!(round_to_tick(100.05, 0.01, OrderSide::Sell), 100.05);
    }

    #[test]
    fn nan_price_passes_through() {
        assert!(round_to_tick(f64::NAN, 0.01, OrderSide::Buy).is_nan());
    }

    #[test]
    fn lot_rounding() {
        assert_eq!(round_to_lot(153.7, 1.0), 153.0);
        assert_eq!(round_to_lot(153.7, 100.0), 100.0);
    }

    #[test]
    fn instrument_serialization_roundtrip() {
        let inst = Instrument::us_equity("AAPL");
        let json = serde_json::to_string(&inst).unwrap();
        let deser: Instrument = serde_json::from_str(&json).unwrap();
        assert_eq!(inst.symbol, deser.symbol);
        assert_eq!(inst.tick_size, deser.tick_size);
    }
}
