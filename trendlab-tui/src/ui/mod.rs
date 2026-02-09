//! Top-level UI layout — six-panel frame with status bar.

pub mod chart_panel;
pub mod data_panel;
pub mod help_panel;
pub mod overlays;
pub mod results_panel;
pub mod status_bar;
pub mod strategy_panel;
pub mod sweep_panel;
pub mod widgets;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders};

use crate::app::{AppState, Overlay, Panel};
use crate::theme;

/// Draw the entire UI.
pub fn draw(f: &mut Frame, app: &AppState) {
    // Split: main area + 1-line status bar.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(f.area());

    let main_area = chunks[0];
    let status_area = chunks[1];

    // Draw the active panel.
    draw_panel(f, main_area, app);

    // Draw status bar.
    status_bar::render(f, status_area, app);

    // Draw overlays on top.
    match &app.overlay {
        Overlay::Welcome => overlays::render_welcome(f, main_area),
        Overlay::ErrorHistory => overlays::render_error_history(f, main_area, app),
        Overlay::Search => overlays::render_search(f, main_area, &app.search_input),
        Overlay::Detail(idx) => overlays::render_detail(f, main_area, app, *idx),
        Overlay::None => {}
    }
}

/// Draw a single panel with its border.
fn draw_panel(f: &mut Frame, area: Rect, app: &AppState) {
    let panel = app.active_panel;
    let is_active = true; // always active since we show only one

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::panel_border(is_active))
        .title(format!(" {} [{}] ", panel.label(), panel.index() + 1))
        .title_style(theme::panel_title(is_active));

    let inner = block.inner(area);
    f.render_widget(block, area);

    match panel {
        Panel::Data => data_panel::render(f, inner, app),
        Panel::Strategy => strategy_panel::render(f, inner, app),
        Panel::Sweep => sweep_panel::render(f, inner, app),
        Panel::Results => results_panel::render(f, inner, app),
        Panel::Chart => chart_panel::render(f, inner, app),
        Panel::Help => help_panel::render(f, inner, app),
    }
}

/// Compute a centered rect for overlays.
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
