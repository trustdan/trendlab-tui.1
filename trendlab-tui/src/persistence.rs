//! App state persistence — JSON save/load across restarts.

use std::path::Path;

use serde::{Deserialize, Serialize};

use trendlab_core::fingerprint::TradingMode;
use trendlab_runner::{RiskProfile, YoloConfig};

use crate::app::{Panel, SessionFilter};

/// Serializable subset of app state that persists across restarts.
#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedState {
    pub selected_tickers: Vec<String>,
    pub yolo_config: YoloConfig,
    pub risk_profile: RiskProfile,
    pub active_panel: Panel,
    pub session_filter: SessionFilter,
    pub welcome_dismissed: bool,
    pub trading_mode: TradingMode,
    pub initial_capital: f64,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            selected_tickers: Vec::new(),
            yolo_config: YoloConfig::default(),
            risk_profile: RiskProfile::default(),
            active_panel: Panel::Data,
            session_filter: SessionFilter::Session,
            welcome_dismissed: false,
            trading_mode: TradingMode::LongOnly,
            initial_capital: 100_000.0,
        }
    }
}

/// Load persisted state from disk. Returns defaults if file is missing or corrupt.
pub fn load(path: &Path) -> PersistedState {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => PersistedState::default(),
    }
}

/// Save persisted state to disk. Creates parent directories if needed.
pub fn save(path: &Path, state: &PersistedState) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Extract persisted state from AppState.
pub fn extract(app: &crate::app::AppState) -> PersistedState {
    PersistedState {
        selected_tickers: app.data.selected.iter().cloned().collect(),
        yolo_config: app.sweep.config.clone(),
        risk_profile: app.results.risk_profile,
        active_panel: app.active_panel,
        session_filter: app.results.session_filter,
        welcome_dismissed: app.overlay != crate::app::Overlay::Welcome,
        trading_mode: app.strategy.trading_mode,
        initial_capital: app.strategy.initial_capital,
    }
}

/// Apply persisted state to AppState.
pub fn apply(app: &mut crate::app::AppState, state: PersistedState) {
    for ticker in &state.selected_tickers {
        app.data.selected.insert(ticker.clone());
    }
    app.sweep.config = state.yolo_config;
    app.results.risk_profile = state.risk_profile;
    app.active_panel = state.active_panel;
    app.results.session_filter = state.session_filter;
    if !state.welcome_dismissed {
        app.overlay = crate::app::Overlay::Welcome;
    }
    app.strategy.trading_mode = state.trading_mode;
    app.strategy.initial_capital = state.initial_capital;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let dir = std::env::temp_dir().join("trendlab_persist_test");
        let path = dir.join("state.json");

        let mut state = PersistedState::default();
        state.selected_tickers = vec!["SPY".into(), "AAPL".into()];
        state.welcome_dismissed = true;
        state.initial_capital = 50_000.0;

        save(&path, &state).unwrap();
        let loaded = load(&path);

        assert_eq!(loaded.selected_tickers.len(), 2);
        assert!(loaded.welcome_dismissed);
        assert_eq!(loaded.initial_capital, 50_000.0);

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_returns_defaults() {
        let loaded = load(Path::new("/nonexistent/path/state.json"));
        assert!(loaded.selected_tickers.is_empty());
        assert!(!loaded.welcome_dismissed);
    }

    #[test]
    fn corrupt_file_returns_defaults() {
        let dir = std::env::temp_dir().join("trendlab_persist_corrupt");
        let path = dir.join("state.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "not valid json {{{").unwrap();

        let loaded = load(&path);
        assert!(loaded.selected_tickers.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
