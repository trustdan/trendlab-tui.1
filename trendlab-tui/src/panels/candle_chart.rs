//! Candle chart panel - OHLC candle rendering with order overlays
//!
//! Renders candlestick chart using direct buffer writes:
//! - Each candle = 1 terminal column
//! - Body: block char, green if close > open, red if close < open
//! - Wicks: vertical line chars to high/low
//! - Order overlays: horizontal dashed lines at stop/limit prices

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Widget},
};
use crate::theme::Theme;

/// OHLC bar for candle chart rendering
#[derive(Debug, Clone)]
pub struct OhlcBar {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

/// Order overlay line on the candle chart
#[derive(Debug, Clone)]
pub struct OrderOverlay {
    pub price: f64,
    pub label: String,
    pub is_stop: bool, // true = stop (negative color), false = limit (positive color)
}

/// Candle chart panel widget
pub struct CandleChartPanel<'a> {
    bars: &'a [OhlcBar],
    overlays: &'a [OrderOverlay],
    symbol: &'a str,
    theme: &'a Theme,
}

impl<'a> CandleChartPanel<'a> {
    pub fn new(
        bars: &'a [OhlcBar],
        overlays: &'a [OrderOverlay],
        symbol: &'a str,
        theme: &'a Theme,
    ) -> Self {
        Self {
            bars,
            overlays,
            symbol,
            theme,
        }
    }

    /// Map a price to a Y position in the plot area (0 = top)
    fn price_to_y(&self, price: f64, y_min: f64, y_max: f64, plot_height: u16) -> u16 {
        if (y_max - y_min).abs() < 1e-9 || plot_height == 0 {
            return 0;
        }
        let frac = (price - y_min) / (y_max - y_min);
        let y = plot_height.saturating_sub(1) as f64 * (1.0 - frac);
        y.round().max(0.0).min(plot_height.saturating_sub(1) as f64) as u16
    }
}

impl<'a> Widget for CandleChartPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.bars.is_empty() {
            let block = Block::default()
                .title(format!(" Candle Chart: {} [No Data] ", self.symbol))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.theme.muted))
                .style(Style::default().bg(self.theme.background));
            block.render(area, buf);
            return;
        }

        // Compute price bounds
        let y_min = self
            .bars
            .iter()
            .map(|b| b.low)
            .fold(f64::INFINITY, f64::min);
        let y_max = self
            .bars
            .iter()
            .map(|b| b.high)
            .fold(f64::NEG_INFINITY, f64::max);

        // Add padding
        let range = y_max - y_min;
        let pad = if range > 0.0 { range * 0.05 } else { 1.0 };
        let y_lower = y_min - pad;
        let y_upper = y_max + pad;

        let up_count = self.bars.iter().filter(|b| b.close >= b.open).count();
        let down_count = self.bars.len() - up_count;

        let title = format!(
            " {} | {} bars | {} up {} down ",
            self.symbol,
            self.bars.len(),
            up_count,
            down_count,
        );

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent))
            .style(Style::default().bg(self.theme.background));

        let inner = block.inner(area);
        block.render(area, buf);

        // Reserve left margin for Y-axis labels (8 chars) and bottom for X axis (1 row)
        let label_width: u16 = 8;
        let plot_left = inner.x + label_width;
        let plot_top = inner.y;
        let plot_width = inner.width.saturating_sub(label_width);
        let plot_height = inner.height.saturating_sub(1);

        if plot_width == 0 || plot_height == 0 {
            return;
        }

        // Draw Y-axis labels
        let y_labels = [y_upper, (y_upper + y_lower) / 2.0, y_lower];
        let y_positions = [0u16, plot_height / 2, plot_height.saturating_sub(1)];
        for (label_val, y_pos) in y_labels.iter().zip(y_positions.iter()) {
            let label = format!("{:>7.1}", label_val);
            let y = plot_top + y_pos;
            if y < inner.y + inner.height {
                buf.set_string(
                    inner.x,
                    y,
                    &label,
                    Style::default().fg(self.theme.muted),
                );
            }
        }

        // Draw candles
        let bars_to_draw = self.bars.len().min(plot_width as usize);
        let start_bar = if self.bars.len() > plot_width as usize {
            self.bars.len() - plot_width as usize
        } else {
            0
        };

        for (i, bar) in self.bars[start_bar..start_bar + bars_to_draw]
            .iter()
            .enumerate()
        {
            let x = plot_left + i as u16;
            if x >= area.right() {
                break;
            }

            let is_up = bar.close >= bar.open;
            let color = if is_up {
                self.theme.positive
            } else {
                self.theme.negative
            };
            let style = Style::default().fg(color);

            let high_y = self.price_to_y(bar.high, y_lower, y_upper, plot_height);
            let low_y = self.price_to_y(bar.low, y_lower, y_upper, plot_height);
            let body_top_y = self.price_to_y(
                bar.open.max(bar.close),
                y_lower,
                y_upper,
                plot_height,
            );
            let body_bot_y = self.price_to_y(
                bar.open.min(bar.close),
                y_lower,
                y_upper,
                plot_height,
            );

            // Draw upper wick
            for y in high_y..body_top_y {
                let py = plot_top + y;
                if py < area.bottom() {
                    buf.set_string(x, py, "|", style);
                }
            }

            // Draw body
            let body_char = if is_up { "\u{2588}" } else { "\u{2593}" }; // full block vs medium shade
            for y in body_top_y..=body_bot_y {
                let py = plot_top + y;
                if py < area.bottom() {
                    buf.set_string(x, py, body_char, style);
                }
            }

            // Draw lower wick
            for y in (body_bot_y + 1)..=low_y {
                let py = plot_top + y;
                if py < area.bottom() {
                    buf.set_string(x, py, "|", style);
                }
            }
        }

        // Draw order overlays as horizontal dashed lines
        for overlay in self.overlays {
            let y = self.price_to_y(overlay.price, y_lower, y_upper, plot_height);
            let py = plot_top + y;
            if py >= area.bottom() || py < plot_top {
                continue;
            }

            let color = if overlay.is_stop {
                self.theme.negative
            } else {
                self.theme.positive
            };
            let style = Style::default()
                .fg(color)
                .add_modifier(Modifier::DIM);

            // Draw dashed line across plot width
            for x in plot_left..plot_left + plot_width {
                if x < area.right() {
                    let ch = if (x - plot_left).is_multiple_of(3) { "-" } else { " " };
                    buf.set_string(x, py, ch, style);
                }
            }

            // Draw label at left edge
            let label_style = Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD);
            buf.set_string(plot_left, py, &overlay.label, label_style);
        }

        // Draw bottom bar info
        let info_y = plot_top + plot_height;
        if info_y < area.bottom() {
            let info = format!(
                "c/C: toggle to equity curve | {} overlays",
                self.overlays.len()
            );
            buf.set_string(
                plot_left,
                info_y,
                &info,
                Style::default().fg(self.theme.muted),
            );
        }
    }
}

/// Build OhlcBars from an equity curve (synthetic OHLC from equity values)
pub fn ohlc_from_equity(equity_values: &[f64]) -> Vec<OhlcBar> {
    equity_values
        .windows(2)
        .map(|w| {
            let prev = w[0];
            let curr = w[1];
            let open = prev;
            let close = curr;
            let high = prev.max(curr) * 1.005;
            let low = prev.min(curr) * 0.995;
            OhlcBar {
                open,
                high,
                low,
                close,
            }
        })
        .collect()
}

/// Extract order overlays from trade records' order_type field
pub fn overlays_from_trades(
    trades: &[trendlab_runner::result::TradeRecord],
) -> Vec<OrderOverlay> {
    let mut overlays = Vec::new();
    for trade in trades {
        if let Some(ref order_type) = trade.order_type {
            // Parse stop prices from order_type strings like "StopMarket(99.5)"
            if let Some(start) = order_type.find('(') {
                if let Some(end) = order_type.find(')') {
                    if let Ok(price) = order_type[start + 1..end].parse::<f64>() {
                        let is_stop = order_type.contains("Stop");
                        overlays.push(OrderOverlay {
                            price,
                            label: (if is_stop { "STP" } else { "LMT" }).to_string(),
                            is_stop,
                        });
                    }
                }
            }
        }
    }
    overlays
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_bars() -> Vec<OhlcBar> {
        vec![
            OhlcBar { open: 100.0, high: 102.0, low: 99.0, close: 101.0 },  // up
            OhlcBar { open: 101.0, high: 103.0, low: 100.0, close: 100.5 }, // down
            OhlcBar { open: 100.5, high: 104.0, low: 99.5, close: 103.0 },  // up
            OhlcBar { open: 103.0, high: 105.0, low: 101.0, close: 102.0 }, // down
            OhlcBar { open: 102.0, high: 106.0, low: 101.5, close: 105.5 }, // up
        ]
    }

    #[test]
    fn test_candle_chart_renders_without_panic() {
        let theme = Theme::default();
        let bars = make_test_bars();
        let overlays = vec![];
        let panel = CandleChartPanel::new(&bars, &overlays, "SPY", &theme);

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);
    }

    #[test]
    fn test_candle_chart_with_overlays() {
        let theme = Theme::default();
        let bars = make_test_bars();
        let overlays = vec![
            OrderOverlay { price: 99.5, label: "STP".to_string(), is_stop: true },
            OrderOverlay { price: 105.0, label: "LMT".to_string(), is_stop: false },
        ];
        let panel = CandleChartPanel::new(&bars, &overlays, "SPY", &theme);

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("STP"));
        assert!(content.contains("LMT"));
    }

    #[test]
    fn test_candle_chart_empty_bars() {
        let theme = Theme::default();
        let bars: Vec<OhlcBar> = vec![];
        let overlays = vec![];
        let panel = CandleChartPanel::new(&bars, &overlays, "SPY", &theme);

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("No Data"));
    }

    #[test]
    fn test_up_candle_uses_positive_color() {
        let theme = Theme::default();
        let bars = vec![OhlcBar {
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.0,
        }];
        let panel = CandleChartPanel::new(&bars, &[], "SPY", &theme);

        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        // Check title shows "1 up 0 down"
        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("1 up 0 down"));
    }

    #[test]
    fn test_ohlc_from_equity() {
        let equity = vec![100.0, 105.0, 103.0, 108.0];
        let bars = ohlc_from_equity(&equity);
        assert_eq!(bars.len(), 3);
        assert!(bars[0].close > bars[0].open); // up
        assert!(bars[1].close < bars[1].open); // down
        assert!(bars[2].close > bars[2].open); // up
    }

    #[test]
    fn test_overlays_from_trades() {
        use trendlab_runner::result::{TradeRecord, TradeDirection};
        use chrono::NaiveDate;

        let trades = vec![TradeRecord {
            symbol: "SPY".to_string(),
            entry_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            exit_date: NaiveDate::from_ymd_opt(2020, 2, 1).unwrap(),
            direction: TradeDirection::Long,
            entry_price: 300.0,
            exit_price: 310.0,
            quantity: 100,
            pnl: 1000.0,
            return_pct: 3.3,
            signal_intent: None,
            order_type: Some("StopMarket(295.0)".to_string()),
            fill_context: None,
            entry_slippage: None,
            exit_slippage: None,
            entry_was_gapped: None,
            exit_was_gapped: None,
        }];

        let overlays = overlays_from_trades(&trades);
        assert_eq!(overlays.len(), 1);
        assert!((overlays[0].price - 295.0).abs() < 0.01);
        assert!(overlays[0].is_stop);
    }
}
