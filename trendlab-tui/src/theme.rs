//! Semantic color tokens for the parrot/neon TUI theme.
//!
//! All widgets reference these tokens — no hardcoded colors in panel code.

use ratatui::style::{Color, Modifier, Style};

/// Near-black / deep charcoal background.
#[allow(dead_code)]
pub const BG: Color = Color::Rgb(26, 26, 46);

/// Electric cyan — accent, active panel borders, highlights.
pub const ACCENT: Color = Color::Rgb(0, 255, 255);

/// Neon green — positive values (profit, good Sharpe).
pub const POSITIVE: Color = Color::Rgb(57, 255, 20);

/// Hot pink — negative values (loss, drawdown).
pub const NEGATIVE: Color = Color::Rgb(255, 16, 100);

/// Neon orange — warnings, incompatible configs.
pub const WARNING: Color = Color::Rgb(255, 165, 0);

/// Cool purple — neutral/decorative.
pub const NEUTRAL: Color = Color::Rgb(170, 130, 255);

/// Steel blue — muted text, inactive borders.
pub const MUTED: Color = Color::Rgb(100, 130, 180);

/// Accent style (electric cyan foreground).
pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

/// Positive style (neon green foreground).
pub fn positive() -> Style {
    Style::default().fg(POSITIVE)
}

/// Negative style (hot pink foreground).
pub fn negative() -> Style {
    Style::default().fg(NEGATIVE)
}

/// Warning style (neon orange foreground).
pub fn warning() -> Style {
    Style::default().fg(WARNING)
}

/// Neutral style (cool purple foreground).
pub fn neutral() -> Style {
    Style::default().fg(NEUTRAL)
}

/// Muted style (steel blue foreground).
pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

/// Bold accent (for titles).
pub fn accent_bold() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

/// Bold style on any color.
#[allow(dead_code)]
pub fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

/// Panel border style — accent if active, muted otherwise.
pub fn panel_border(is_active: bool) -> Style {
    if is_active {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(MUTED)
    }
}

/// Panel title style — bold accent if active, muted otherwise.
pub fn panel_title(is_active: bool) -> Style {
    if is_active {
        accent_bold()
    } else {
        muted()
    }
}

/// Style a metric value: green if positive, red if negative.
pub fn metric_color(value: f64) -> Style {
    if value > 0.0 {
        positive()
    } else if value < 0.0 {
        negative()
    } else {
        muted()
    }
}

/// Style for Sharpe ratio: green if >1, muted if 0-1, red if <0.
pub fn sharpe_style(sharpe: f64) -> Style {
    if sharpe >= 1.0 {
        positive()
    } else if sharpe >= 0.0 {
        muted()
    } else {
        negative()
    }
}
