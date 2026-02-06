//! Parrot/neon theme tokens for TrendLab v3 TUI
//!
//! Provides a consistent color palette inspired by:
//! - Parrot color scheme (neon accents on dark background)
//! - Terminal aesthetic with high contrast
//!
//! # Color Palette
//! - **Background**: Near-black / deep charcoal (base layer)
//! - **Accent**: Electric cyan (primary highlights, focus)
//! - **Positive**: Neon green (gains, success, long positions)
//! - **Negative**: Hot pink (losses, failures, short positions)
//! - **Warning**: Neon orange (alerts, thresholds, important info)
//! - **Neutral**: Cool purple (secondary info, neutral states)
//! - **Muted**: Steel blue (disabled, secondary text)

use ratatui::style::Color;

/// Parrot/neon theme for TrendLab TUI
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Near-black background (primary surface)
    pub background: Color,
    /// Electric cyan accent (focus, highlights)
    pub accent: Color,
    /// Neon green (positive values, gains, long)
    pub positive: Color,
    /// Hot pink (negative values, losses, short)
    pub negative: Color,
    /// Neon orange (warnings, alerts)
    pub warning: Color,
    /// Cool purple (neutral info, secondary)
    pub neutral: Color,
    /// Steel blue (muted text, disabled)
    pub muted: Color,
    /// White (primary text)
    pub text_primary: Color,
    /// Light gray (secondary text)
    pub text_secondary: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::parrot_neon()
    }
}

impl Theme {
    /// Create the default Parrot/neon theme
    pub fn parrot_neon() -> Self {
        Self {
            // Background: deep charcoal (almost black)
            background: Color::Rgb(18, 18, 20),

            // Accent: electric cyan
            accent: Color::Rgb(0, 255, 255),

            // Positive: neon green
            positive: Color::Rgb(0, 255, 128),

            // Negative: hot pink
            negative: Color::Rgb(255, 20, 147),

            // Warning: neon orange
            warning: Color::Rgb(255, 140, 0),

            // Neutral: cool purple
            neutral: Color::Rgb(147, 112, 219),

            // Muted: steel blue
            muted: Color::Rgb(100, 149, 237),

            // Text colors
            text_primary: Color::White,
            text_secondary: Color::Rgb(170, 170, 170),
        }
    }

    /// Get color for PnL value (positive = green, negative = pink)
    pub fn pnl_color(&self, value: f64) -> Color {
        if value >= 0.0 {
            self.positive
        } else {
            self.negative
        }
    }

    /// Get color for Sharpe ratio (gradient from muted to positive)
    pub fn sharpe_color(&self, sharpe: f64) -> Color {
        match sharpe {
            s if s >= 2.0 => self.positive,
            s if s >= 1.0 => self.accent,
            s if s >= 0.5 => self.neutral,
            s if s >= 0.0 => self.muted,
            _ => self.negative,
        }
    }

    /// Get color for win rate percentage
    pub fn win_rate_color(&self, win_rate: f64) -> Color {
        match win_rate {
            w if w >= 0.7 => self.positive,
            w if w >= 0.5 => self.accent,
            w if w >= 0.4 => self.neutral,
            _ => self.warning,
        }
    }

    /// Get color for rejection reason (diagnostic colors)
    pub fn rejection_color(&self, reason: &str) -> Color {
        match reason {
            "InsufficientCash" | "MarginGuard" => self.negative,
            "VolatilityTooHigh" | "VolatilityGuard" => self.warning,
            "PositionSizeTooSmall" | "LiquidityGuard" => self.muted,
            "OrderPolicyBlocked" | "RiskGuard" => self.neutral,
            _ => self.text_secondary,
        }
    }

    /// Get color for signal direction
    pub fn signal_color(&self, signal: &str) -> Color {
        match signal {
            "Long" | "Buy" => self.positive,
            "Short" | "Sell" => self.negative,
            "Flat" | "Exit" => self.neutral,
            _ => self.text_secondary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_creation() {
        let theme = Theme::default();
        assert_eq!(theme.background, Color::Rgb(18, 18, 20));
        assert_eq!(theme.accent, Color::Rgb(0, 255, 255));
    }

    #[test]
    fn test_pnl_color() {
        let theme = Theme::default();
        assert_eq!(theme.pnl_color(100.0), theme.positive);
        assert_eq!(theme.pnl_color(-50.0), theme.negative);
        assert_eq!(theme.pnl_color(0.0), theme.positive);
    }

    #[test]
    fn test_sharpe_color() {
        let theme = Theme::default();
        assert_eq!(theme.sharpe_color(2.5), theme.positive);
        assert_eq!(theme.sharpe_color(1.5), theme.accent);
        assert_eq!(theme.sharpe_color(0.7), theme.neutral);
        assert_eq!(theme.sharpe_color(0.3), theme.muted);
        assert_eq!(theme.sharpe_color(-0.5), theme.negative);
    }

    #[test]
    fn test_win_rate_color() {
        let theme = Theme::default();
        assert_eq!(theme.win_rate_color(0.75), theme.positive);
        assert_eq!(theme.win_rate_color(0.55), theme.accent);
        assert_eq!(theme.win_rate_color(0.45), theme.neutral);
        assert_eq!(theme.win_rate_color(0.30), theme.warning);
    }

    #[test]
    fn test_rejection_color() {
        let theme = Theme::default();
        assert_eq!(theme.rejection_color("InsufficientCash"), theme.negative);
        assert_eq!(theme.rejection_color("VolatilityTooHigh"), theme.warning);
        assert_eq!(theme.rejection_color("PositionSizeTooSmall"), theme.muted);
        assert_eq!(theme.rejection_color("OrderPolicyBlocked"), theme.neutral);
    }

    #[test]
    fn test_signal_color() {
        let theme = Theme::default();
        assert_eq!(theme.signal_color("Long"), theme.positive);
        assert_eq!(theme.signal_color("Short"), theme.negative);
        assert_eq!(theme.signal_color("Flat"), theme.neutral);
        assert_eq!(theme.signal_color("Unknown"), theme.text_secondary);
    }
}
