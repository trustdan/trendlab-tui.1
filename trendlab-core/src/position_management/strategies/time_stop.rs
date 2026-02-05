/// Time-based exit
///
/// Closes positions after a fixed number of bars, regardless of P&L.
/// Useful for:
/// - Mean-reversion strategies (exit after reversion window)
/// - Preventing stale positions
/// - Testing time-limited strategies
use crate::domain::{Bar, OrderId, Position};
use crate::orders::order_type::MarketTiming;
use crate::orders::{Order, OrderType};
use crate::position_management::{OrderIntent, PositionManager, Side};

/// Time-based exit strategy
#[derive(Debug, Clone)]
pub struct TimeStop {
    /// Maximum bars to hold position
    pub max_bars: usize,

    /// Bar count since position entry
    bars_held: usize,

    /// Whether exit order has been placed
    exit_order_placed: bool,
}

impl TimeStop {
    /// Create a new time-based exit strategy
    ///
    /// # Arguments
    /// * `max_bars` - Maximum number of bars to hold position
    ///
    /// # Example
    /// ```
    /// use trendlab_core::position_management::TimeStop;
    ///
    /// // Exit after 10 bars
    /// let pm = TimeStop::new(10);
    /// ```
    pub fn new(max_bars: usize) -> Self {
        assert!(max_bars > 0, "max_bars must be positive");
        Self {
            max_bars,
            bars_held: 0,
            exit_order_placed: false,
        }
    }

    /// Reset state (called when position goes flat)
    fn reset(&mut self) {
        self.bars_held = 0;
        self.exit_order_placed = false;
    }
}

impl PositionManager for TimeStop {
    fn update(&mut self, position: &Position, _bar: &Bar) -> Vec<OrderIntent> {
        // Determine position side
        let _side = match Side::from_quantity(position.quantity) {
            Some(s) => s,
            None => {
                // Flat position: reset state
                self.reset();
                return vec![OrderIntent::None];
            }
        };

        // Increment bar counter
        self.bars_held += 1;

        // Check if time limit reached
        if self.bars_held >= self.max_bars && !self.exit_order_placed {
            // Time to exit: place market-on-close order to flatten position
            let qty = position.quantity.abs() as u32;

            // Create market-on-close order to exit
            let order = Order::new(
                OrderId::from(1),
                position.symbol.clone(),
                OrderType::Market(MarketTiming::MOC),
                qty,
                0, // Bar number would be set by order book
            );

            self.exit_order_placed = true;

            vec![OrderIntent::Place(order)]
        } else {
            // Not yet time to exit
            vec![OrderIntent::None]
        }
    }

    fn name(&self) -> &str {
        "TimeStop"
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
    fn test_time_stop_creation() {
        let pm = TimeStop::new(10);
        assert_eq!(pm.max_bars, 10);
        assert_eq!(pm.bars_held, 0);
        assert!(!pm.exit_order_placed);
    }

    #[test]
    #[should_panic(expected = "max_bars must be positive")]
    fn test_invalid_max_bars() {
        TimeStop::new(0);
    }

    #[test]
    fn test_flat_position_no_action() {
        let mut pm = TimeStop::new(10);
        let position = create_test_position(0.0, 100.0);
        let bar = create_test_bar(100.0);

        let intents = pm.update(&position, &bar);
        assert_eq!(intents.len(), 1);
        assert!(intents[0].is_none());
        assert_eq!(pm.bars_held, 0);
    }

    #[test]
    fn test_bar_counter_increments() {
        let mut pm = TimeStop::new(10);
        let position = create_test_position(100.0, 100.0);
        let bar = create_test_bar(105.0);

        // Bar 1
        pm.update(&position, &bar);
        assert_eq!(pm.bars_held, 1);

        // Bar 2
        pm.update(&position, &bar);
        assert_eq!(pm.bars_held, 2);

        // Bar 3
        pm.update(&position, &bar);
        assert_eq!(pm.bars_held, 3);
    }

    #[test]
    fn test_exit_before_max_bars() {
        let mut pm = TimeStop::new(10);
        let position = create_test_position(100.0, 100.0);
        let bar = create_test_bar(105.0);

        // Bars 1-9: no action
        for _ in 0..9 {
            let intents = pm.update(&position, &bar);
            assert_eq!(intents.len(), 1);
            assert!(intents[0].is_none());
        }

        assert_eq!(pm.bars_held, 9);
        assert!(!pm.exit_order_placed);
    }

    #[test]
    fn test_exit_at_max_bars() {
        let mut pm = TimeStop::new(10);
        let position = create_test_position(100.0, 100.0);
        let bar = create_test_bar(105.0);

        // Bars 1-9: no action
        for _ in 0..9 {
            pm.update(&position, &bar);
        }

        // Bar 10: exit
        let intents = pm.update(&position, &bar);
        assert_eq!(intents.len(), 1);

        match &intents[0] {
            OrderIntent::Place(order) => {
                assert_eq!(order.qty, 100);
                assert_eq!(order.symbol, "AAPL");
                match order.order_type {
                    OrderType::Market(MarketTiming::MOC) => {
                        // Expected: market-on-close order
                    }
                    _ => panic!("Expected Market(MOC) order"),
                }
            }
            _ => panic!("Expected Place intent"),
        }

        assert!(pm.exit_order_placed);
    }

    #[test]
    fn test_no_duplicate_exit_order() {
        let mut pm = TimeStop::new(10);
        let position = create_test_position(100.0, 100.0);
        let bar = create_test_bar(105.0);

        // Bars 1-9
        for _ in 0..9 {
            pm.update(&position, &bar);
        }

        // Bar 10: exit order placed
        let intents1 = pm.update(&position, &bar);
        assert!(!intents1[0].is_none());
        assert!(pm.exit_order_placed);

        // Bar 11: no duplicate order
        let intents2 = pm.update(&position, &bar);
        assert_eq!(intents2.len(), 1);
        assert!(intents2[0].is_none());
    }

    #[test]
    fn test_reset_on_flat_position() {
        let mut pm = TimeStop::new(10);
        let position = create_test_position(100.0, 100.0);
        let bar = create_test_bar(105.0);

        // Hold for 5 bars
        for _ in 0..5 {
            pm.update(&position, &bar);
        }
        assert_eq!(pm.bars_held, 5);

        // Go flat
        let flat_position = create_test_position(0.0, 100.0);
        pm.update(&flat_position, &bar);

        // State should reset
        assert_eq!(pm.bars_held, 0);
        assert!(!pm.exit_order_placed);
    }

    #[test]
    fn test_short_position_exit() {
        let mut pm = TimeStop::new(5);
        let position = create_test_position(-100.0, 100.0);
        let bar = create_test_bar(95.0);

        // Bars 1-4: no action
        for _ in 0..4 {
            let intents = pm.update(&position, &bar);
            assert!(intents[0].is_none());
        }

        // Bar 5: exit
        let intents = pm.update(&position, &bar);
        match &intents[0] {
            OrderIntent::Place(order) => {
                assert_eq!(order.qty, 100);
            }
            _ => panic!("Expected Place intent"),
        }
    }

    #[test]
    fn test_name() {
        let pm = TimeStop::new(10);
        assert_eq!(pm.name(), "TimeStop");
    }

    #[test]
    fn test_clone_box() {
        let pm = TimeStop::new(10);
        let cloned = pm.clone_box();
        assert_eq!(cloned.name(), "TimeStop");
    }
}
