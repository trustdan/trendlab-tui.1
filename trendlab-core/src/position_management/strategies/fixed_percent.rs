/// Fixed percentage stop loss
///
/// Places a stop at a fixed % below entry (long) or above entry (short).
/// Simple, static stop that doesn't adjust with market conditions.
use crate::domain::{Bar, OrderId, Position};
use crate::position_management::{OrderIntent, PositionManager, Side};

/// Fixed percentage stop loss strategy
#[derive(Debug, Clone)]
pub struct FixedPercentStop {
    /// Stop distance as a percentage (e.g., 0.05 = 5%)
    pub stop_pct: f64,

    /// Currently active stop order ID (if any)
    active_stop_id: Option<OrderId>,
}

impl FixedPercentStop {
    /// Create a new fixed percentage stop
    ///
    /// # Arguments
    /// * `stop_pct` - Stop distance as decimal (e.g., 0.05 for 5%)
    ///
    /// # Example
    /// ```
    /// use trendlab_core::position_management::FixedPercentStop;
    ///
    /// // 5% stop loss
    /// let pm = FixedPercentStop::new(0.05);
    /// ```
    pub fn new(stop_pct: f64) -> Self {
        assert!(stop_pct > 0.0 && stop_pct < 1.0, "stop_pct must be in (0, 1)");
        Self {
            stop_pct,
            active_stop_id: None,
        }
    }

    /// Calculate stop price based on entry
    fn calculate_stop(&self, entry_price: f64, side: Side) -> f64 {
        match side {
            Side::Long => entry_price * (1.0 - self.stop_pct),
            Side::Short => entry_price * (1.0 + self.stop_pct),
        }
    }
}

impl PositionManager for FixedPercentStop {
    fn update(&mut self, position: &Position, _bar: &Bar) -> Vec<OrderIntent> {
        // Determine position side
        let side = match Side::from_quantity(position.quantity) {
            Some(s) => s,
            None => {
                // Flat position: no action
                self.active_stop_id = None;
                return vec![OrderIntent::None];
            }
        };

        // If we already have a stop, no update needed (fixed stop)
        if self.active_stop_id.is_some() {
            return vec![OrderIntent::None];
        }

        // Place initial stop
        let stop_price = self.calculate_stop(position.avg_entry_price, side);
        let qty = position.quantity.abs() as u32;

        // Create stop order intent
        let intent = OrderIntent::UpdateStop {
            old_order_id: OrderId::from(1), // Dummy ID (will be ignored if no existing stop)
            new_stop_price: stop_price,
            qty,
            side,
        };

        // Track that we've placed a stop
        // (In real implementation, this would be set after order confirmation)
        // For now, we'll use a placeholder ID
        self.active_stop_id = Some(OrderId::from(1));

        vec![intent]
    }

    fn name(&self) -> &str {
        "FixedPercentStop"
    }

    fn clone_box(&self) -> Box<dyn PositionManager> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_position(qty: f64, entry: f64) -> Position {
        Position {
            symbol: "AAPL".to_string(),
            quantity: qty,
            avg_entry_price: entry,
        }
    }

    fn create_test_bar(close: f64) -> Bar {
        Bar {
            timestamp: Utc::now(),
            symbol: "AAPL".to_string(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_fixed_percent_stop_creation() {
        let pm = FixedPercentStop::new(0.05);
        assert_eq!(pm.stop_pct, 0.05);
        assert!(pm.active_stop_id.is_none());
    }

    #[test]
    #[should_panic(expected = "stop_pct must be in (0, 1)")]
    fn test_invalid_stop_pct_zero() {
        FixedPercentStop::new(0.0);
    }

    #[test]
    #[should_panic(expected = "stop_pct must be in (0, 1)")]
    fn test_invalid_stop_pct_negative() {
        FixedPercentStop::new(-0.05);
    }

    #[test]
    #[should_panic(expected = "stop_pct must be in (0, 1)")]
    fn test_invalid_stop_pct_too_large() {
        FixedPercentStop::new(1.5);
    }

    #[test]
    fn test_long_stop_calculation() {
        let pm = FixedPercentStop::new(0.05);
        let stop = pm.calculate_stop(100.0, Side::Long);
        assert_eq!(stop, 95.0); // 100 * (1 - 0.05)
    }

    #[test]
    fn test_short_stop_calculation() {
        let pm = FixedPercentStop::new(0.05);
        let stop = pm.calculate_stop(100.0, Side::Short);
        assert_eq!(stop, 105.0); // 100 * (1 + 0.05)
    }

    #[test]
    fn test_flat_position_no_action() {
        let mut pm = FixedPercentStop::new(0.05);
        let position = create_test_position(0.0, 100.0);
        let bar = create_test_bar(100.0);

        let intents = pm.update(&position, &bar);
        assert_eq!(intents.len(), 1);
        assert!(intents[0].is_none());
    }

    #[test]
    fn test_long_position_places_stop() {
        let mut pm = FixedPercentStop::new(0.05);
        let position = create_test_position(100.0, 100.0); // Long 100 @ 100
        let bar = create_test_bar(105.0);

        let intents = pm.update(&position, &bar);
        assert_eq!(intents.len(), 1);

        match &intents[0] {
            OrderIntent::UpdateStop {
                new_stop_price,
                qty,
                side,
                ..
            } => {
                assert_eq!(*new_stop_price, 95.0); // 5% below entry
                assert_eq!(*qty, 100);
                assert_eq!(*side, Side::Long);
            }
            _ => panic!("Expected UpdateStop intent"),
        }
    }

    #[test]
    fn test_short_position_places_stop() {
        let mut pm = FixedPercentStop::new(0.05);
        let position = create_test_position(-100.0, 100.0); // Short 100 @ 100
        let bar = create_test_bar(95.0);

        let intents = pm.update(&position, &bar);
        assert_eq!(intents.len(), 1);

        match &intents[0] {
            OrderIntent::UpdateStop {
                new_stop_price,
                qty,
                side,
                ..
            } => {
                assert_eq!(*new_stop_price, 105.0); // 5% above entry
                assert_eq!(*qty, 100);
                assert_eq!(*side, Side::Short);
            }
            _ => panic!("Expected UpdateStop intent"),
        }
    }

    #[test]
    fn test_no_update_after_initial_stop() {
        let mut pm = FixedPercentStop::new(0.05);
        let position = create_test_position(100.0, 100.0);
        let bar = create_test_bar(105.0);

        // First update: places stop
        let intents1 = pm.update(&position, &bar);
        assert_eq!(intents1.len(), 1);
        assert!(!intents1[0].is_none());

        // Second update: no action (stop already placed)
        let intents2 = pm.update(&position, &bar);
        assert_eq!(intents2.len(), 1);
        assert!(intents2[0].is_none());
    }

    #[test]
    fn test_name() {
        let pm = FixedPercentStop::new(0.05);
        assert_eq!(pm.name(), "FixedPercentStop");
    }

    #[test]
    fn test_clone_box() {
        let pm = FixedPercentStop::new(0.05);
        let cloned = pm.clone_box();
        assert_eq!(cloned.name(), "FixedPercentStop");
    }
}
