use chrono::TimeZone;
use trendlab_tui::ghost_curve::{GhostCurve, IdealEquity, RealEquity};

#[test]
fn hard_fail_death_crossing() {
    let mut ideal = IdealEquity::new();
    let mut real = RealEquity::new();
    let ts = chrono::Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();

    ideal.push(ts, 10000.0);
    ideal.push(ts, 12000.0); // +20%

    real.push(ts, 10000.0);
    real.push(ts, 10000.0); // 0% real return

    let ghost = GhostCurve::new(ideal, real);
    assert!(ghost.drag_metric.is_death_crossing());
}
