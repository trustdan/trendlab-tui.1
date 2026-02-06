//! Keyboard navigation and event handling
//!
//! Maps keyboard events to app actions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::App;

/// Handle keyboard input and update app state
pub fn handle_key_event(app: &mut App, key: KeyEvent) {
    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            app.quit();
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.quit();
        }

        // Navigation
        KeyCode::Up | KeyCode::Char('k') => {
            app.select_previous();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.select_next();
        }
        KeyCode::Enter => {
            app.drill_down();
        }
        KeyCode::Esc | KeyCode::Backspace => {
            app.go_back();
        }

        // Drill-down shortcuts
        KeyCode::Char('d') | KeyCode::Char('D') => {
            app.show_diagnostics();
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            app.show_rejected_intents();
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            app.show_execution_lab();
        }
        KeyCode::Char('x') | KeyCode::Char('X') => {
            app.show_sensitivity();
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            app.show_manifest();
        }
        KeyCode::Char('b') | KeyCode::Char('B') => {
            app.show_robustness();
        }

        // Chart mode toggle
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.toggle_chart_mode();
        }

        // Leaderboard controls
        KeyCode::Char('f') | KeyCode::Char('F') => {
            app.cycle_fitness_metric();
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            app.toggle_session_filter();
        }

        _ => {}
    }
}

/// Key bindings help text
pub fn key_bindings_help() -> Vec<(&'static str, &'static str)> {
    vec![
        ("q / Ctrl+C", "Quit"),
        ("↑/k, ↓/j", "Navigate"),
        ("Enter", "Drill down"),
        ("Esc/Backspace", "Go back"),
        ("d", "Show diagnostics"),
        ("i", "Show rejected intents"),
        ("r", "Show execution lab"),
        ("x", "Show sensitivity"),
        ("m", "Show run manifest"),
        ("b", "Show robustness ladder"),
        ("c", "Toggle equity/candle chart"),
        ("f", "Cycle fitness metric"),
        ("s", "Toggle session/all-time"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drill_down::DrillDownState;

    #[test]
    fn test_quit_on_q() {
        let mut app = App::new();
        let key = KeyEvent::from(KeyCode::Char('q'));

        handle_key_event(&mut app, key);
        assert!(app.should_quit);
    }

    #[test]
    fn test_quit_on_ctrl_c() {
        let mut app = App::new();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

        handle_key_event(&mut app, key);
        assert!(app.should_quit);
    }

    #[test]
    fn test_navigation_up_down() {
        let mut app = App::new();
        // Add some mock results so navigation works
        let results = vec![
            crate::test_helpers::create_test_result("run_1", 2.5),
            crate::test_helpers::create_test_result("run_2", 2.0),
            crate::test_helpers::create_test_result("run_3", 1.8),
            crate::test_helpers::create_test_result("run_4", 1.5),
            crate::test_helpers::create_test_result("run_5", 1.2),
            crate::test_helpers::create_test_result("run_6", 1.0),
            crate::test_helpers::create_test_result("run_7", 0.8),
        ];
        app.load_results(results);
        app.selected_index = 5;

        // Move down
        let key = KeyEvent::from(KeyCode::Down);
        handle_key_event(&mut app, key);
        assert_eq!(app.selected_index, 6);

        // Move up
        let key = KeyEvent::from(KeyCode::Up);
        handle_key_event(&mut app, key);
        assert_eq!(app.selected_index, 5);
    }

    #[test]
    fn test_vim_style_navigation() {
        let mut app = App::new();
        // Add some mock results
        let results = vec![
            crate::test_helpers::create_test_result("run_1", 2.5),
            crate::test_helpers::create_test_result("run_2", 2.0),
            crate::test_helpers::create_test_result("run_3", 1.8),
            crate::test_helpers::create_test_result("run_4", 1.5),
            crate::test_helpers::create_test_result("run_5", 1.2),
        ];
        app.load_results(results);
        app.selected_index = 3;

        // j = down
        let key = KeyEvent::from(KeyCode::Char('j'));
        handle_key_event(&mut app, key);
        assert_eq!(app.selected_index, 4);

        // k = up
        let key = KeyEvent::from(KeyCode::Char('k'));
        handle_key_event(&mut app, key);
        assert_eq!(app.selected_index, 3);
    }

    #[test]
    fn test_drill_down_shortcut() {
        let mut app = App::new();
        app.drill_down = DrillDownState::ChartWithTrade("run_1".to_string(), "trade_1".to_string());

        let key = KeyEvent::from(KeyCode::Char('d'));
        handle_key_event(&mut app, key);

        assert!(matches!(app.drill_down, DrillDownState::Diagnostics(_, _)));
    }

    #[test]
    fn test_rejected_intents_shortcut() {
        let mut app = App::new();
        app.drill_down = DrillDownState::SummaryCard("run_1".to_string());

        let key = KeyEvent::from(KeyCode::Char('i'));
        handle_key_event(&mut app, key);

        assert!(matches!(app.drill_down, DrillDownState::RejectedIntents(_)));
    }

    #[test]
    fn test_key_bindings_help() {
        let bindings = key_bindings_help();
        assert!(!bindings.is_empty());
        assert_eq!(bindings[0].0, "q / Ctrl+C");
    }
}
