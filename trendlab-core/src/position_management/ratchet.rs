/// Ratchet invariant enforcement
///
/// **Core Rule:** Stops may tighten, never loosen (even if ATR expands).
///
/// This prevents the "volatility trap" where ATR expansion would widen stops,
/// exposing positions to larger losses after favorable moves.
use crate::position_management::manager::Side;

/// Ratchet state for stop-loss management
///
/// Enforces the invariant that stops can only move in the favorable direction:
/// - Long positions: stop can only rise (tighten)
/// - Short positions: stop can only fall (tighten)
#[derive(Debug, Clone, PartialEq)]
pub struct RatchetState {
    /// Current stop level (high-water mark for longs, low-water mark for shorts)
    current_level: Option<f64>,

    /// Position side
    side: Side,

    /// Whether ratchet is enabled (default: true)
    enabled: bool,
}

impl RatchetState {
    /// Create a new ratchet state
    pub fn new(side: Side) -> Self {
        Self {
            current_level: None,
            side,
            enabled: true,
        }
    }

    /// Create a ratchet with an initial level
    pub fn with_initial_level(side: Side, initial_level: f64) -> Self {
        Self {
            current_level: Some(initial_level),
            side,
            enabled: true,
        }
    }

    /// Create a disabled ratchet (allows loosening)
    pub fn disabled(side: Side) -> Self {
        Self {
            current_level: None,
            side,
            enabled: false,
        }
    }

    /// Apply ratchet to a proposed stop level
    ///
    /// Returns the ratcheted level (can only tighten, never loosen).
    ///
    /// # Rules
    /// - Long positions: stop can only rise (max of current and proposed)
    /// - Short positions: stop can only fall (min of current and proposed)
    /// - If ratchet is disabled, returns proposed level unchanged
    /// - If no current level exists, initializes to proposed level
    ///
    /// # Example
    /// ```
    /// use trendlab_core::position_management::{RatchetState, Side};
    ///
    /// let mut ratchet = RatchetState::with_initial_level(Side::Long, 95.0);
    ///
    /// // Tightening: $95 → $100 (allowed)
    /// let level = ratchet.apply(100.0);
    /// assert_eq!(level, 100.0);
    ///
    /// // Loosening: $100 → $90 (blocked, stays at $100)
    /// let level = ratchet.apply(90.0);
    /// assert_eq!(level, 100.0);
    /// ```
    pub fn apply(&mut self, proposed: f64) -> f64 {
        if !self.enabled {
            // Ratchet disabled: allow any level
            self.current_level = Some(proposed);
            return proposed;
        }

        match self.current_level {
            None => {
                // First time: initialize to proposed level
                self.current_level = Some(proposed);
                proposed
            }
            Some(current) => {
                // Apply ratchet rule based on side
                let ratcheted = match self.side {
                    Side::Long => {
                        // Long: stop can only rise (tighten)
                        // Use max(current, proposed)
                        current.max(proposed)
                    }
                    Side::Short => {
                        // Short: stop can only fall (tighten)
                        // Use min(current, proposed)
                        current.min(proposed)
                    }
                };

                self.current_level = Some(ratcheted);
                ratcheted
            }
        }
    }

    /// Get current ratchet level (if set)
    pub fn current_level(&self) -> Option<f64> {
        self.current_level
    }

    /// Check if ratchet is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Reset the ratchet to a new level
    pub fn reset(&mut self, new_level: f64) {
        self.current_level = Some(new_level);
    }

    /// Clear the ratchet state
    pub fn clear(&mut self) {
        self.current_level = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ratchet_long_tightening_allowed() {
        let mut ratchet = RatchetState::with_initial_level(Side::Long, 95.0);

        // Propose tightening: $95 → $100
        let result = ratchet.apply(100.0);
        assert_eq!(result, 100.0);
        assert_eq!(ratchet.current_level(), Some(100.0));
    }

    #[test]
    fn test_ratchet_long_loosening_blocked() {
        let mut ratchet = RatchetState::with_initial_level(Side::Long, 100.0);

        // Propose loosening: $100 → $90 (should be blocked)
        let result = ratchet.apply(90.0);
        assert_eq!(result, 100.0); // Stays at $100
        assert_eq!(ratchet.current_level(), Some(100.0));
    }

    #[test]
    fn test_ratchet_short_tightening_allowed() {
        let mut ratchet = RatchetState::with_initial_level(Side::Short, 105.0);

        // Propose tightening: $105 → $100
        let result = ratchet.apply(100.0);
        assert_eq!(result, 100.0);
        assert_eq!(ratchet.current_level(), Some(100.0));
    }

    #[test]
    fn test_ratchet_short_loosening_blocked() {
        let mut ratchet = RatchetState::with_initial_level(Side::Short, 100.0);

        // Propose loosening: $100 → $110 (should be blocked)
        let result = ratchet.apply(110.0);
        assert_eq!(result, 100.0); // Stays at $100
        assert_eq!(ratchet.current_level(), Some(100.0));
    }

    #[test]
    fn test_ratchet_initialization() {
        let mut ratchet = RatchetState::new(Side::Long);
        assert_eq!(ratchet.current_level(), None);

        // First apply initializes the level
        let result = ratchet.apply(95.0);
        assert_eq!(result, 95.0);
        assert_eq!(ratchet.current_level(), Some(95.0));
    }

    #[test]
    fn test_ratchet_disabled() {
        let mut ratchet = RatchetState::disabled(Side::Long);
        assert!(!ratchet.is_enabled());

        // First level
        let result = ratchet.apply(100.0);
        assert_eq!(result, 100.0);

        // Loosening is allowed when disabled
        let result = ratchet.apply(90.0);
        assert_eq!(result, 90.0);
        assert_eq!(ratchet.current_level(), Some(90.0));
    }

    #[test]
    fn test_ratchet_reset() {
        let mut ratchet = RatchetState::with_initial_level(Side::Long, 100.0);

        // Reset to new level
        ratchet.reset(110.0);
        assert_eq!(ratchet.current_level(), Some(110.0));

        // Can still only tighten from new level
        let result = ratchet.apply(105.0);
        assert_eq!(result, 110.0); // Blocked
    }

    #[test]
    fn test_ratchet_clear() {
        let mut ratchet = RatchetState::with_initial_level(Side::Long, 100.0);

        ratchet.clear();
        assert_eq!(ratchet.current_level(), None);

        // Next apply reinitializes
        let result = ratchet.apply(95.0);
        assert_eq!(result, 95.0);
    }

    #[test]
    fn test_ratchet_volatility_trap_scenario() {
        // Scenario: ATR expansion should not loosen stop
        let mut ratchet = RatchetState::with_initial_level(Side::Long, 95.0);

        // Price rises to $110
        // ATR expands from $5 to $10
        // Proposed stop: $110 - 2*$10 = $90 (looser)
        let proposed = 90.0;
        let result = ratchet.apply(proposed);

        // Ratchet blocks loosening: stop stays at $95
        assert_eq!(result, 95.0);
        assert_eq!(ratchet.current_level(), Some(95.0));
    }

    #[test]
    fn test_ratchet_favorable_move_scenario() {
        // Scenario: Favorable price move allows tightening
        let mut ratchet = RatchetState::with_initial_level(Side::Long, 95.0);

        // Price rises to $110, ATR stable at $5
        // Proposed stop: $110 - 2*$5 = $100 (tighter)
        let proposed = 100.0;
        let result = ratchet.apply(proposed);

        // Ratchet allows tightening
        assert_eq!(result, 100.0);
        assert_eq!(ratchet.current_level(), Some(100.0));
    }
}
