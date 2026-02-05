/// Chandelier exit: anti-stickiness via snapshot reference levels
///
/// **Key Anti-Stickiness Feature:**
/// Uses a *snapshot* of the highest high (or lowest low for shorts) from a
/// lookback period. The reference level does NOT chase new highs/lows,
/// allowing profitable exits in rise-then-fall scenarios.
///
/// **How it works:**
/// 1. Capture reference high/low from lookback period
/// 2. Place stop at: reference ± (atr_mult * ATR)
/// 3. Reference only updates on NEW highs/lows
/// 4. Stop does NOT chase price—it stays anchored to the reference
///
/// This prevents the classic "chasing highs" trap where exits never trigger.
use crate::domain::{Bar, OrderId, Position};
use crate::position_management::{OrderIntent, PositionManager, Side};
use std::collections::VecDeque;

/// Chandelier exit strategy
#[derive(Debug, Clone)]
pub struct ChandelierExit {
    /// Lookback period for highest high / lowest low
    pub lookback: usize,

    /// ATR multiplier for stop distance
    pub atr_mult: f64,

    /// Current ATR value
    current_atr: f64,

    /// Reference level (snapshot, doesn't chase)
    reference_level: Option<f64>,

    /// Price history (for lookback calculation)
    price_history: VecDeque<f64>,

    /// Currently active stop order ID (if any)
    active_stop_id: Option<OrderId>,
}

impl ChandelierExit {
    /// Create a new Chandelier exit strategy
    ///
    /// # Arguments
    /// * `lookback` - Number of bars for highest high / lowest low
    /// * `atr_mult` - ATR multiplier for stop distance
    /// * `initial_atr` - Initial ATR value
    ///
    /// # Example
    /// ```
    /// use trendlab_core::position_management::ChandelierExit;
    ///
    /// // 20-bar chandelier with 2x ATR
    /// let pm = ChandelierExit::new(20, 2.0, 5.0);
    /// ```
    pub fn new(lookback: usize, atr_mult: f64, initial_atr: f64) -> Self {
        assert!(lookback > 0, "lookback must be positive");
        assert!(atr_mult > 0.0, "atr_mult must be positive");
        assert!(initial_atr > 0.0, "initial_atr must be positive");

        Self {
            lookback,
            atr_mult,
            current_atr: initial_atr,
            reference_level: None,
            price_history: VecDeque::with_capacity(lookback),
            active_stop_id: None,
        }
    }

    /// Update ATR value
    pub fn update_atr(&mut self, new_atr: f64) {
        assert!(new_atr > 0.0, "ATR must be positive");
        self.current_atr = new_atr;
    }

    /// Update price history and reference level
    fn update_reference(&mut self, bar: &Bar, side: Side) {
        // Add current price to history
        let price = match side {
            Side::Long => bar.high,
            Side::Short => bar.low,
        };

        self.price_history.push_back(price);
        if self.price_history.len() > self.lookback {
            self.price_history.pop_front();
        }

        // Calculate reference level from history
        let new_reference = match side {
            Side::Long => {
                // Highest high over lookback
                self.price_history
                    .iter()
                    .copied()
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(price)
            }
            Side::Short => {
                // Lowest low over lookback
                self.price_history
                    .iter()
                    .copied()
                    .min_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(price)
            }
        };

        // **Anti-stickiness: Only update if new extreme is made**
        // This prevents chasing—reference stays at snapshot until new high/low
        match self.reference_level {
            None => {
                // First time: initialize
                self.reference_level = Some(new_reference);
            }
            Some(current_ref) => {
                // Update only if new extreme
                let is_new_extreme = match side {
                    Side::Long => new_reference > current_ref,
                    Side::Short => new_reference < current_ref,
                };

                if is_new_extreme {
                    self.reference_level = Some(new_reference);
                }
                // else: reference stays at snapshot (anti-stickiness)
            }
        }
    }

    /// Calculate stop price from reference level
    fn calculate_stop(&self, side: Side) -> Option<f64> {
        let reference = self.reference_level?;
        let stop_distance = self.atr_mult * self.current_atr;

        let stop = match side {
            Side::Long => reference - stop_distance,
            Side::Short => reference + stop_distance,
        };

        Some(stop)
    }
}

impl PositionManager for ChandelierExit {
    fn update(&mut self, position: &Position, bar: &Bar) -> Vec<OrderIntent> {
        // Determine position side
        let side = match Side::from_quantity(position.quantity) {
            Some(s) => s,
            None => {
                // Flat position: reset state
                self.reference_level = None;
                self.price_history.clear();
                self.active_stop_id = None;
                return vec![OrderIntent::None];
            }
        };

        // Update reference level
        self.update_reference(bar, side);

        // Calculate stop from reference
        let stop_price = match self.calculate_stop(side) {
            Some(stop) => stop,
            None => return vec![OrderIntent::None],
        };

        // Emit update stop intent
        let qty = position.quantity.abs() as u32;
        let intent = OrderIntent::UpdateStop {
            old_order_id: self.active_stop_id.clone().unwrap_or_else(|| OrderId::from(1)),
            new_stop_price: stop_price,
            qty,
            side,
        };

        // Track active stop
        if self.active_stop_id.is_none() {
            self.active_stop_id = Some(OrderId::from(1));
        }

        vec![intent]
    }

    fn name(&self) -> &str {
        "ChandelierExit"
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

    fn create_test_bar(high: f64, low: f64, close: f64) -> Bar {
        Bar {
            timestamp: Utc::now(),
            symbol: "AAPL".to_string(),
            open: close,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_chandelier_creation() {
        let pm = ChandelierExit::new(20, 2.0, 5.0);
        assert_eq!(pm.lookback, 20);
        assert_eq!(pm.atr_mult, 2.0);
        assert_eq!(pm.current_atr, 5.0);
        assert!(pm.reference_level.is_none());
    }

    #[test]
    #[should_panic(expected = "lookback must be positive")]
    fn test_invalid_lookback() {
        ChandelierExit::new(0, 2.0, 5.0);
    }

    #[test]
    fn test_update_atr() {
        let mut pm = ChandelierExit::new(20, 2.0, 5.0);
        pm.update_atr(10.0);
        assert_eq!(pm.current_atr, 10.0);
    }

    #[test]
    fn test_flat_position_no_action() {
        let mut pm = ChandelierExit::new(20, 2.0, 5.0);
        let position = create_test_position(0.0, 100.0);
        let bar = create_test_bar(100.0, 100.0, 100.0);

        let intents = pm.update(&position, &bar);
        assert_eq!(intents.len(), 1);
        assert!(intents[0].is_none());
    }

    #[test]
    fn test_long_position_initializes_reference() {
        let mut pm = ChandelierExit::new(20, 2.0, 5.0);
        let position = create_test_position(100.0, 100.0);
        let bar = create_test_bar(120.0, 110.0, 115.0);

        let intents = pm.update(&position, &bar);
        assert_eq!(intents.len(), 1);

        // Reference should be initialized to highest high (120)
        assert_eq!(pm.reference_level, Some(120.0));

        // Stop should be at reference - atr_mult * ATR
        // = 120 - 2*5 = 110
        match &intents[0] {
            OrderIntent::UpdateStop { new_stop_price, .. } => {
                assert_eq!(*new_stop_price, 110.0);
            }
            _ => panic!("Expected UpdateStop intent"),
        }
    }

    #[test]
    fn test_anti_stickiness_reference_does_not_chase() {
        let mut pm = ChandelierExit::new(5, 2.0, 5.0);
        let position = create_test_position(100.0, 100.0);

        // Bar 1: High = 120
        // Reference = 120, Stop = 120 - 10 = 110
        let bar1 = create_test_bar(120.0, 110.0, 115.0);
        let intents1 = pm.update(&position, &bar1);
        assert_eq!(pm.reference_level, Some(120.0));
        match &intents1[0] {
            OrderIntent::UpdateStop { new_stop_price, .. } => {
                assert_eq!(*new_stop_price, 110.0);
            }
            _ => panic!("Expected UpdateStop intent"),
        }

        // Bar 2: Price falls to 116 (high still 120 in history)
        // Reference should STAY at 120 (anti-stickiness)
        // Stop should STAY at 110
        let bar2 = create_test_bar(116.0, 114.0, 115.0);
        let intents2 = pm.update(&position, &bar2);

        // Reference should NOT chase down
        assert_eq!(pm.reference_level, Some(120.0));

        // Stop should stay at 110
        match &intents2[0] {
            OrderIntent::UpdateStop { new_stop_price, .. } => {
                assert_eq!(*new_stop_price, 110.0);
            }
            _ => panic!("Expected UpdateStop intent"),
        }
    }

    #[test]
    fn test_reference_updates_on_new_high() {
        let mut pm = ChandelierExit::new(5, 2.0, 5.0);
        let position = create_test_position(100.0, 100.0);

        // Bar 1: High = 120
        let bar1 = create_test_bar(120.0, 110.0, 115.0);
        pm.update(&position, &bar1);
        assert_eq!(pm.reference_level, Some(120.0));

        // Bar 2: NEW high at 130
        // Reference should update to 130
        let bar2 = create_test_bar(130.0, 120.0, 125.0);
        let intents2 = pm.update(&position, &bar2);

        assert_eq!(pm.reference_level, Some(130.0));

        // Stop should be 130 - 10 = 120
        match &intents2[0] {
            OrderIntent::UpdateStop { new_stop_price, .. } => {
                assert_eq!(*new_stop_price, 120.0);
            }
            _ => panic!("Expected UpdateStop intent"),
        }
    }

    #[test]
    fn test_short_position_reference() {
        let mut pm = ChandelierExit::new(5, 2.0, 5.0);
        let position = create_test_position(-100.0, 100.0);

        // Bar 1: Low = 80
        // Reference = 80, Stop = 80 + 10 = 90
        let bar1 = create_test_bar(90.0, 80.0, 85.0);
        let intents1 = pm.update(&position, &bar1);

        assert_eq!(pm.reference_level, Some(80.0));
        match &intents1[0] {
            OrderIntent::UpdateStop { new_stop_price, side, .. } => {
                assert_eq!(*new_stop_price, 90.0);
                assert_eq!(*side, Side::Short);
            }
            _ => panic!("Expected UpdateStop intent"),
        }
    }

    #[test]
    fn test_short_anti_stickiness() {
        let mut pm = ChandelierExit::new(5, 2.0, 5.0);
        let position = create_test_position(-100.0, 100.0);

        // Bar 1: Low = 80
        let bar1 = create_test_bar(90.0, 80.0, 85.0);
        pm.update(&position, &bar1);
        assert_eq!(pm.reference_level, Some(80.0));

        // Bar 2: Price rises to 84 (low = 82)
        // Reference should STAY at 80 (lowest low, anti-stickiness)
        let bar2 = create_test_bar(86.0, 82.0, 84.0);
        pm.update(&position, &bar2);

        // Reference should NOT chase up
        assert_eq!(pm.reference_level, Some(80.0));
    }

    #[test]
    fn test_lookback_limit() {
        let mut pm = ChandelierExit::new(3, 2.0, 5.0);
        let position = create_test_position(100.0, 100.0);

        // Add 3 bars
        pm.update(&position, &create_test_bar(120.0, 110.0, 115.0));
        pm.update(&position, &create_test_bar(125.0, 115.0, 120.0));
        pm.update(&position, &create_test_bar(130.0, 120.0, 125.0));

        assert_eq!(pm.price_history.len(), 3);

        // Add one more bar (should evict oldest)
        pm.update(&position, &create_test_bar(135.0, 125.0, 130.0));

        assert_eq!(pm.price_history.len(), 3); // Still 3 (not 4)
    }

    #[test]
    fn test_name() {
        let pm = ChandelierExit::new(20, 2.0, 5.0);
        assert_eq!(pm.name(), "ChandelierExit");
    }

    #[test]
    fn test_clone_box() {
        let pm = ChandelierExit::new(20, 2.0, 5.0);
        let cloned = pm.clone_box();
        assert_eq!(cloned.name(), "ChandelierExit");
    }

    #[test]
    fn test_rise_then_fall_profitable_exit_scenario() {
        // **Key Anti-Stickiness Test:**
        // Position enters at $100, rises to $120, then falls to $116.
        // Chandelier should allow exit at ~$115, NOT be stuck chasing $120.
        let mut pm = ChandelierExit::new(5, 2.0, 2.5);
        let position = create_test_position(100.0, 100.0);

        // Bar 1: Price rises to $120
        // Reference = 120, Stop = 120 - 2*2.5 = 115
        let bar1 = create_test_bar(120.0, 115.0, 118.0);
        let intents1 = pm.update(&position, &bar1);

        assert_eq!(pm.reference_level, Some(120.0));
        match &intents1[0] {
            OrderIntent::UpdateStop { new_stop_price, .. } => {
                assert_eq!(*new_stop_price, 115.0);
            }
            _ => panic!("Expected UpdateStop intent"),
        }

        // Bar 2: Price falls to $116
        // Reference STAYS at $120 (anti-stickiness)
        // Stop STAYS at $115
        // Position can exit at $115 for a profitable exit (+$15)
        let bar2 = create_test_bar(116.0, 114.0, 115.0);
        let intents2 = pm.update(&position, &bar2);

        assert_eq!(pm.reference_level, Some(120.0));
        match &intents2[0] {
            OrderIntent::UpdateStop { new_stop_price, .. } => {
                assert_eq!(*new_stop_price, 115.0);
            }
            _ => panic!("Expected UpdateStop intent"),
        }

        // ✓ Stop is at $115, allowing profitable exit
        // ✗ Without anti-stickiness: stop would chase to $116-5=$111, missing exit
    }
}
