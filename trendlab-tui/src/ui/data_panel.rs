//! Panel 1 — Data: sector/ticker tree, fetch progress, cache status indicators.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::AppState;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &AppState) {
    let data = &app.data;
    let mut lines: Vec<Line> = Vec::new();

    // Header
    let selected_count = data.selected.len();
    let total_count = data.universe.ticker_count();
    lines.push(Line::from(vec![
        Span::styled("Selected: ", theme::muted()),
        Span::styled(
            format!("{selected_count}/{total_count}"),
            theme::accent(),
        ),
        Span::styled(
            "  [Space]toggle [a]ll [d]eselect [f]etch [s]earch",
            theme::muted(),
        ),
    ]));
    lines.push(Line::from(""));

    // Fetch progress
    if data.fetch_in_progress {
        let sym = data
            .fetch_current_symbol
            .as_deref()
            .unwrap_or("...");
        lines.push(Line::from(vec![
            Span::styled("Fetching ", theme::warning()),
            Span::styled(sym, theme::accent()),
            Span::styled(
                format!("... [{}/{}]", data.fetch_done, data.fetch_total),
                theme::muted(),
            ),
        ]));
        lines.push(Line::from(""));
    }

    // Tree view
    let mut row = 0usize;
    for sector_name in data.universe.sector_names() {
        let is_expanded = data.expanded_sectors.contains(sector_name);
        let is_cursor = row == data.cursor.row;

        // Count selected in this sector
        let tickers = data.universe.sector_tickers(sector_name).unwrap_or(&[]);
        let sel_in_sector = tickers
            .iter()
            .filter(|t| data.selected.contains(*t))
            .count();

        let arrow = if is_expanded { "▾" } else { "▸" };
        let label = format!(
            "{arrow} {sector_name} ({sel_in_sector}/{})",
            tickers.len()
        );

        let style = if is_cursor {
            theme::accent().add_modifier(Modifier::REVERSED)
        } else {
            theme::neutral()
        };
        lines.push(Line::from(Span::styled(label, style)));
        row += 1;

        if is_expanded {
            for ticker in tickers {
                let is_cursor = row == data.cursor.row;
                let is_selected = data.selected.contains(ticker);
                let is_cached = data
                    .cache_status
                    .get(ticker)
                    .copied()
                    .unwrap_or(false);

                let check = if is_selected { "[x]" } else { "[ ]" };
                let dot = if is_cached { " ●" } else { " ○" };

                let mut spans = vec![
                    Span::raw("  "),
                    Span::raw(check),
                    Span::raw(" "),
                ];

                let ticker_style = if is_cursor {
                    theme::accent().add_modifier(Modifier::REVERSED)
                } else if is_selected {
                    theme::accent()
                } else {
                    theme::muted()
                };
                spans.push(Span::styled(ticker.as_str(), ticker_style));

                let dot_style = if is_cached {
                    theme::positive()
                } else {
                    theme::muted()
                };
                spans.push(Span::styled(dot, dot_style));

                lines.push(Line::from(spans));
                row += 1;
            }
        }
    }

    let para = Paragraph::new(lines);
    f.render_widget(para, area);
}
