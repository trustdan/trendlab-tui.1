//! Universe configuration — sector-organized ticker lists.
//!
//! The universe is stored as a TOML config file with GICS sectors
//! and their member tickers. Supports selection/deselection of
//! individual tickers or entire sectors.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// A sector in the universe (e.g., Technology, Healthcare).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sector {
    pub name: String,
    pub tickers: Vec<String>,
}

/// The complete universe configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Universe {
    pub sectors: BTreeMap<String, Vec<String>>,
}

impl Universe {
    /// Load a universe from a TOML file.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("read universe file: {e}"))?;
        Self::from_toml(&content)
    }

    /// Parse a universe from a TOML string.
    pub fn from_toml(content: &str) -> Result<Self, String> {
        toml::from_str(content).map_err(|e| format!("parse universe TOML: {e}"))
    }

    /// Get all tickers across all sectors.
    pub fn all_tickers(&self) -> Vec<&str> {
        self.sectors
            .values()
            .flat_map(|tickers| tickers.iter().map(|t| t.as_str()))
            .collect()
    }

    /// Get tickers for a specific sector.
    pub fn sector_tickers(&self, sector: &str) -> Option<&[String]> {
        self.sectors.get(sector).map(|v| v.as_slice())
    }

    /// Get the list of sector names.
    pub fn sector_names(&self) -> Vec<&str> {
        self.sectors.keys().map(|s| s.as_str()).collect()
    }

    /// Total number of tickers.
    pub fn ticker_count(&self) -> usize {
        self.sectors.values().map(|v| v.len()).sum()
    }

    /// Create a default US equity universe with major sectors.
    pub fn default_us() -> Self {
        let mut sectors = BTreeMap::new();

        sectors.insert(
            "Technology".into(),
            vec![
                "AAPL", "MSFT", "GOOGL", "AMZN", "NVDA", "META", "AVGO", "CRM", "ADBE", "ORCL",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );

        sectors.insert(
            "Healthcare".into(),
            vec!["JNJ", "UNH", "PFE", "ABBV", "MRK", "LLY", "TMO", "ABT"]
                .into_iter()
                .map(String::from)
                .collect(),
        );

        sectors.insert(
            "Finance".into(),
            vec![
                "JPM", "BAC", "WFC", "GS", "MS", "BLK", "SCHW", "C", "AXP", "V",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );

        sectors.insert(
            "Energy".into(),
            vec!["XOM", "CVX", "COP", "SLB", "EOG", "MPC", "PSX", "VLO"]
                .into_iter()
                .map(String::from)
                .collect(),
        );

        sectors.insert(
            "Consumer".into(),
            vec![
                "WMT", "PG", "KO", "PEP", "COST", "HD", "MCD", "NKE", "SBUX", "TGT",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        );

        sectors.insert(
            "ETFs".into(),
            vec!["SPY", "QQQ", "IWM", "DIA", "XLF", "XLE", "XLK", "XLV"]
                .into_iter()
                .map(String::from)
                .collect(),
        );

        Self { sectors }
    }

    /// Serialize the universe to TOML.
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| format!("serialize universe: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_universe_has_sectors() {
        let u = Universe::default_us();
        assert!(u.sector_names().contains(&"Technology"));
        assert!(u.sector_names().contains(&"ETFs"));
        assert!(u.ticker_count() > 30);
    }

    #[test]
    fn toml_roundtrip() {
        let u = Universe::default_us();
        let toml_str = u.to_toml().unwrap();
        let parsed = Universe::from_toml(&toml_str).unwrap();
        assert_eq!(u.ticker_count(), parsed.ticker_count());
    }

    #[test]
    fn all_tickers_flattens() {
        let u = Universe::default_us();
        let all = u.all_tickers();
        assert!(all.contains(&"SPY"));
        assert!(all.contains(&"AAPL"));
    }

    #[test]
    fn sector_lookup() {
        let u = Universe::default_us();
        let etfs = u.sector_tickers("ETFs").unwrap();
        assert!(etfs.contains(&"SPY".to_string()));
    }
}
