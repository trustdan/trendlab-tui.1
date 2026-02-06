//! Execution drag metric calculation
//!
//! Computes the drag between ideal and real equity curves.

use super::{IdealEquity, RealEquity};

/// Combined ghost curve (ideal + real + drag)
#[derive(Debug, Clone)]
pub struct GhostCurve {
    pub ideal: IdealEquity,
    pub real: RealEquity,
    pub drag_metric: DragMetric,
}

impl GhostCurve {
    /// Create a new ghost curve from ideal and real equity
    pub fn new(ideal: IdealEquity, real: RealEquity) -> Self {
        let drag_metric = DragMetric::compute(&ideal, &real);
        Self {
            ideal,
            real,
            drag_metric,
        }
    }

    /// Check if both curves are aligned (same length)
    pub fn is_aligned(&self) -> bool {
        self.ideal.len() == self.real.len()
    }

    /// Get drag percentage at final point
    pub fn final_drag_percentage(&self) -> f64 {
        self.drag_metric.percentage
    }

    /// Get drag in absolute dollars at final point
    pub fn final_drag_dollars(&self) -> f64 {
        self.drag_metric.dollars
    }
}

/// Execution drag metric
#[derive(Debug, Clone, Copy)]
pub struct DragMetric {
    /// Drag as percentage: (ideal - real) / ideal * 100
    pub percentage: f64,
    /// Drag in absolute dollars: ideal - real
    pub dollars: f64,
}

impl DragMetric {
    /// Compute execution drag between ideal and real equity
    pub fn compute(ideal: &IdealEquity, real: &RealEquity) -> Self {
        let ideal_final = ideal.final_equity().unwrap_or(0.0);
        let real_final = real.final_equity().unwrap_or(0.0);

        let dollars = ideal_final - real_final;
        let percentage = if ideal_final != 0.0 {
            (dollars / ideal_final) * 100.0
        } else {
            0.0
        };

        Self {
            percentage,
            dollars,
        }
    }

    /// Format drag as percentage string
    pub fn format_percentage(&self) -> String {
        format!("{:.2}%", self.percentage)
    }

    /// Format drag as dollar string
    pub fn format_dollars(&self) -> String {
        format!("${:.2}", self.dollars)
    }

    /// Check if drag is significant (> 1%)
    pub fn is_significant(&self) -> bool {
        self.percentage > 1.0
    }

    /// Check if drag is severe (> 5%)
    pub fn is_severe(&self) -> bool {
        self.percentage > 5.0
    }

    /// Check if drag exceeds death crossing threshold (> 15%)
    pub fn is_death_crossing(&self) -> bool {
        self.percentage > 15.0
    }
}

impl Default for DragMetric {
    fn default() -> Self {
        Self {
            percentage: 0.0,
            dollars: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono::Utc;

    fn create_test_curves() -> (IdealEquity, RealEquity) {
        let mut ideal = IdealEquity::new();
        let mut real = RealEquity::new();
        let ts = Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();

        ideal.push(ts, 10000.0);
        ideal.push(ts, 11000.0); // 10% ideal return

        real.push(ts, 10000.0);
        real.push(ts, 10800.0); // 8% real return (2% drag)

        (ideal, real)
    }

    #[test]
    fn test_drag_metric_compute() {
        let (ideal, real) = create_test_curves();
        let drag = DragMetric::compute(&ideal, &real);

        assert!((drag.dollars - 200.0).abs() < 0.01);
        assert!((drag.percentage - 1.818).abs() < 0.01); // (200/11000)*100
    }

    #[test]
    fn test_drag_metric_format() {
        let drag = DragMetric {
            percentage: 1.82,
            dollars: 200.0,
        };

        assert_eq!(drag.format_percentage(), "1.82%");
        assert_eq!(drag.format_dollars(), "$200.00");
    }

    #[test]
    fn test_drag_significance() {
        let low_drag = DragMetric {
            percentage: 0.5,
            dollars: 50.0,
        };
        assert!(!low_drag.is_significant());

        let high_drag = DragMetric {
            percentage: 2.0,
            dollars: 200.0,
        };
        assert!(high_drag.is_significant());

        let severe_drag = DragMetric {
            percentage: 6.0,
            dollars: 600.0,
        };
        assert!(severe_drag.is_severe());
    }

    #[test]
    fn test_death_crossing_threshold() {
        let drag = DragMetric {
            percentage: 15.1,
            dollars: 1500.0,
        };
        assert!(drag.is_death_crossing());
    }

    #[test]
    fn test_ghost_curve_creation() {
        let (ideal, real) = create_test_curves();
        let ghost = GhostCurve::new(ideal, real);

        assert!(ghost.is_aligned());
        assert!((ghost.final_drag_percentage() - 1.818).abs() < 0.01);
    }
}
