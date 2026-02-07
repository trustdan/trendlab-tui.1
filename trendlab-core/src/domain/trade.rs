//! TradeRecord — a completed round-trip trade with full traceability.

use super::ids::SignalEventId;
use super::position::PositionSide;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// A complete round-trip trade record: entry → exit.
///
/// Includes signal traceability fields for isolating component effects
/// (which signal triggered this trade, which PM managed it, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    // ── Identification ──
    pub symbol: String,
    pub side: PositionSide,

    // ── Entry ──
    pub entry_bar: usize,
    pub entry_date: NaiveDate,
    pub entry_price: f64,

    // ── Exit ──
    pub exit_bar: usize,
    pub exit_date: NaiveDate,
    pub exit_price: f64,

    // ── Size ──
    pub quantity: f64,

    // ── PnL ──
    pub gross_pnl: f64,
    pub commission: f64,
    pub slippage: f64,
    pub net_pnl: f64,

    // ── Duration ──
    pub bars_held: usize,

    // ── Excursion ──
    /// Maximum adverse excursion (worst unrealized loss during the trade).
    pub mae: f64,
    /// Maximum favorable excursion (best unrealized gain during the trade).
    pub mfe: f64,

    // ── Signal traceability ──
    pub signal_id: Option<SignalEventId>,
    pub signal_type: Option<String>,
    pub pm_type: Option<String>,
    pub execution_model: Option<String>,
    pub filter_type: Option<String>,
}

impl TradeRecord {
    /// Return on the trade as a fraction of entry cost.
    pub fn return_pct(&self) -> f64 {
        if self.entry_price == 0.0 || self.quantity == 0.0 {
            return 0.0;
        }
        self.net_pnl / (self.entry_price * self.quantity)
    }

    pub fn is_winner(&self) -> bool {
        self.net_pnl > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_trade() -> TradeRecord {
        TradeRecord {
            symbol: "SPY".into(),
            side: PositionSide::Long,
            entry_bar: 4,
            entry_date: NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
            entry_price: 100.0,
            exit_bar: 8,
            exit_date: NaiveDate::from_ymd_opt(2024, 1, 11).unwrap(),
            exit_price: 110.0,
            quantity: 50.0,
            gross_pnl: 500.0,
            commission: 10.0,
            slippage: 5.0,
            net_pnl: 485.0,
            bars_held: 4,
            mae: -50.0,
            mfe: 600.0,
            signal_id: Some(SignalEventId(1)),
            signal_type: Some("donchian_breakout".into()),
            pm_type: Some("atr_trailing".into()),
            execution_model: Some("next_bar_open".into()),
            filter_type: Some("no_filter".into()),
        }
    }

    #[test]
    fn return_pct_calculation() {
        let trade = sample_trade();
        let expected = 485.0 / (100.0 * 50.0);
        assert!((trade.return_pct() - expected).abs() < 1e-10);
    }

    #[test]
    fn is_winner() {
        assert!(sample_trade().is_winner());
    }

    #[test]
    fn trade_serialization_roundtrip() {
        let trade = sample_trade();
        let json = serde_json::to_string(&trade).unwrap();
        let deser: TradeRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(trade.symbol, deser.symbol);
        assert_eq!(trade.net_pnl, deser.net_pnl);
        assert_eq!(trade.signal_id, deser.signal_id);
    }
}
