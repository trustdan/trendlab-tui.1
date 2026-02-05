/// ATR-based stop with ratchet invariant
///
/// Dynamically adjusts stop based on volatility (ATR), but enforces
/// the ratchet rule: stops can only tighten, never loosen.
///
/// **Key Feature:** Prevents the "volatility trap" where ATR expansion
/// would widen stops after favorable moves.
use crate::domain::{Bar, OrderId, Position};
use crate::position_management::{OrderIntent, PositionManager, RatchetState, Side};

/// ATR-based stop loss with ratchet
#[derive(Debug, Clone)]
pub struct AtrStop {
    /// ATR multiplier (e.g., 2.0 = 2x ATR)
    pub atr_mult: f64,

    /// Current ATR value (updated per bar)
    current_atr: f64,

    /// Ratchet state (prevents loosening)
    ratchet: Option<RatchetState>,

    /// Currently active stop order ID (if any)
    active_stop_id: Option<OrderId>,
}

impl AtrStop {
    /// Create a new ATR stop strategy
    ///
    /// # Arguments
    /// * `atr_mult` - ATR multiplier (e.g., 2.0 for 2x ATR stop distance)
    /// * `initial_atr` - Initial ATR value (will be updated from bars)
    ///
    /// # Example
    /// ```
    /// use trendlab_core::position_management::AtrStop;
    ///
    /// // 2x ATR stop, starting with ATR = 5.0
    /// let pm = AtrStop::new(2.0, 5.0);
    /// ```
    pub fn new(atr_mult: f64, initial_atr: f64) -> Self {
        assert!(atr_mult > 0.0, "atr_mult must be positive");
        assert!(initial_atr > 0.0, "initial_atr must be positive");

        Self {
            atr_mult,
            current_atr: initial_atr,
            ratchet: None,
            active_stop_id: None,
        }
    }

    /// Update ATR value (would typically come from indicator calculation)
    pub fn update_atr(&mut self, new_atr: f64) {
        assert!(new_atr > 0.0, "ATR must be positive");
        self.current_atr = new_atr;
    }

    /// Calculate proposed stop price based on current price and ATR
    fn calculate_stop(&self, current_price: f64, side: Side) -> f64 {
        let stop_distance = self.atr_mult * self.current_atr;
        match side {
            Side::Long => current_price - stop_distance,
            Side::Short => current_price + stop_distance,
        }
    }
}

impl PositionManager for AtrStop {
    fn update(&mut self, position: &Position, bar: &Bar) -> Vec<OrderIntent> {
        // Determine position side
        let side = match Side::from_quantity(position.quantity) {
            Some(s) => s,
            None => {
                // Flat position: reset ratchet
                self.ratchet = None;
                self.active_stop_id = None;
                return vec![OrderIntent::None];
            }
        };

        // Check if this is the first update (no stop placed yet)
        let is_first_update = self.active_stop_id.is_none();

        // Initialize ratchet if needed
        if self.ratchet.is_none() {
            let initial_stop = self.calculate_stop(bar.close, side);
            self.ratchet = Some(RatchetState::with_initial_level(side, initial_stop));
        }

        // Calculate proposed stop based on current price and ATR
        let proposed_stop = self.calculate_stop(bar.close, side);

        // Get current ratchet level before applying (for change detection)
        let previous_level = self.ratchet.as_ref().unwrap().current_level();

        // Apply ratchet: can only tighten, never loosen
        let ratcheted_stop = self
            .ratchet
            .as_mut()
            .unwrap()
            .apply(proposed_stop);

        // Check if stop has changed from previous level
        let stop_changed = is_first_update
            || match previous_level {
                None => true, // Safety: always emit if no previous level
                Some(prev) => (ratcheted_stop - prev).abs() > 1e-6,
            };

        if !stop_changed {
            // Stop hasn't moved: no action
            return vec![OrderIntent::None];
        }

        // Emit update stop intent
        let qty = position.quantity.abs() as u32;
        let intent = OrderIntent::UpdateStop {
            old_order_id: self.active_stop_id.clone().unwrap_or_else(|| OrderId::from(1)),
            new_stop_price: ratcheted_stop,
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
        "AtrStop"
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
    fn test_atr_stop_creation() {
        let pm = AtrStop::new(2.0, 5.0);
        assert_eq!(pm.atr_mult, 2.0);
        assert_eq!(pm.current_atr, 5.0);
        assert!(pm.ratchet.is_none());
        assert!(pm.active_stop_id.is_none());
    }

    #[test]
    #[should_panic(expected = "atr_mult must be positive")]
    fn test_invalid_atr_mult() {
        AtrStop::new(-1.0, 5.0);
    }

    #[test]
    #[should_panic(expected = "initial_atr must be positive")]
    fn test_invalid_initial_atr() {
        AtrStop::new(2.0, -5.0);
    }

    #[test]
    fn test_update_atr() {
        let mut pm = AtrStop::new(2.0, 5.0);
        pm.update_atr(10.0);
        assert_eq!(pm.current_atr, 10.0);
    }

    #[test]
    fn test_long_stop_calculation() {
        let pm = AtrStop::new(2.0, 5.0);
        let stop = pm.calculate_stop(110.0, Side::Long);
        assert_eq!(stop, 100.0); // 110 - 2*5
    }

    #[test]
    fn test_short_stop_calculation() {
        let pm = AtrStop::new(2.0, 5.0);
        let stop = pm.calculate_stop(100.0, Side::Short);
        assert_eq!(stop, 110.0); // 100 + 2*5
    }

    #[test]
    fn test_flat_position_no_action() {
        let mut pm = AtrStop::new(2.0, 5.0);
        let position = create_test_position(0.0, 100.0);
        let bar = create_test_bar(100.0);

        let intents = pm.update(&position, &bar);
        assert_eq!(intents.len(), 1);
        assert!(intents[0].is_none());
    }

    #[test]
    fn test_long_position_initializes_ratchet() {
        let mut pm = AtrStop::new(2.0, 5.0);
        let position = create_test_position(100.0, 100.0);
        let bar = create_test_bar(110.0);

        let intents = pm.update(&position, &bar);
        assert_eq!(intents.len(), 1);

        // Ratchet should be initialized
        assert!(pm.ratchet.is_some());
        assert_eq!(pm.ratchet.as_ref().unwrap().current_level(), Some(100.0)); // 110 - 2*5
    }

    #[test]
    fn test_ratchet_prevents_loosening() {
        let mut pm = AtrStop::new(2.0, 5.0);
        let position = create_test_position(100.0, 100.0);

        // Bar 1: Price at 110, ATR = 5
        // Proposed stop: 110 - 2*5 = 100
        let bar1 = create_test_bar(110.0);
        let intents1 = pm.update(&position, &bar1);
        assert_eq!(intents1.len(), 1);
        match &intents1[0] {
            OrderIntent::UpdateStop { new_stop_price, .. } => {
                assert_eq!(*new_stop_price, 100.0);
            }
            _ => panic!("Expected UpdateStop intent"),
        }

        // Bar 2: Price still 110, but ATR expands to 10
        // Proposed stop: 110 - 2*10 = 90 (looser)
        // Ratchet should block this
        pm.update_atr(10.0);
        let bar2 = create_test_bar(110.0);
        let intents2 = pm.update(&position, &bar2);

        // Should return None (stop unchanged by ratchet)
        assert_eq!(intents2.len(), 1);
        assert!(intents2[0].is_none());

        // Ratchet level should remain at 100
        assert_eq!(pm.ratchet.as_ref().unwrap().current_level(), Some(100.0));
    }

    #[test]
    fn test_ratchet_allows_tightening() {
        let mut pm = AtrStop::new(2.0, 5.0);
        let position = create_test_position(100.0, 100.0);

        // Bar 1: Price at 110, ATR = 5
        // Proposed stop: 110 - 2*5 = 100
        let bar1 = create_test_bar(110.0);
        pm.update(&position, &bar1);

        // Bar 2: Price rises to 120, ATR stable at 5
        // Proposed stop: 120 - 2*5 = 110 (tighter)
        // Ratchet should allow this
        let bar2 = create_test_bar(120.0);
        let intents2 = pm.update(&position, &bar2);

        assert_eq!(intents2.len(), 1);
        match &intents2[0] {
            OrderIntent::UpdateStop { new_stop_price, .. } => {
                assert_eq!(*new_stop_price, 110.0);
            }
            _ => panic!("Expected UpdateStop intent"),
        }

        // Ratchet level should update to 110
        assert_eq!(pm.ratchet.as_ref().unwrap().current_level(), Some(110.0));
    }

    #[test]
    fn test_short_position_ratchet() {
        let mut pm = AtrStop::new(2.0, 5.0);
        let position = create_test_position(-100.0, 100.0);

        // Bar 1: Price at 90, ATR = 5
        // Proposed stop: 90 + 2*5 = 100
        let bar1 = create_test_bar(90.0);
        let intents1 = pm.update(&position, &bar1);
        assert_eq!(intents1.len(), 1);
        match &intents1[0] {
            OrderIntent::UpdateStop { new_stop_price, side, .. } => {
                assert_eq!(*new_stop_price, 100.0);
                assert_eq!(*side, Side::Short);
            }
            _ => panic!("Expected UpdateStop intent"),
        }

        // Bar 2: Price still 90, but ATR expands to 10
        // Proposed stop: 90 + 2*10 = 110 (looser)
        // Ratchet should block this
        pm.update_atr(10.0);
        let bar2 = create_test_bar(90.0);
        let intents2 = pm.update(&position, &bar2);

        // Should return None (stop unchanged by ratchet)
        assert_eq!(intents2.len(), 1);
        assert!(intents2[0].is_none());
    }

    #[test]
    fn test_name() {
        let pm = AtrStop::new(2.0, 5.0);
        assert_eq!(pm.name(), "AtrStop");
    }

    #[test]
    fn test_clone_box() {
        let pm = AtrStop::new(2.0, 5.0);
        let cloned = pm.clone_box();
        assert_eq!(cloned.name(), "AtrStop");
    }
}
