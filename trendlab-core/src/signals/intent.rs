//! Signal intent — portfolio-agnostic market exposure desire

use serde::{Deserialize, Serialize};

/// Portfolio-agnostic expression of desired market exposure
///
/// Intent is what the signal "wants" based purely on market data,
/// NOT what the portfolio currently has.
///
/// # Examples
/// - Price breaks above 20-day high → Long (regardless of current position)
/// - RSI < 30 and bouncing → Long (regardless of current position)
/// - Bearish divergence → Short or Flat (regardless of current position)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalIntent {
    /// Want long exposure
    Long,

    /// Want short exposure
    Short,

    /// Want no exposure (close existing position)
    Flat,
}

impl SignalIntent {
    /// Check if intent represents directional exposure
    pub fn is_directional(&self) -> bool {
        matches!(self, SignalIntent::Long | SignalIntent::Short)
    }

    /// Check if intent is flat/neutral
    pub fn is_flat(&self) -> bool {
        matches!(self, SignalIntent::Flat)
    }

    /// Get the opposite intent (for position flips)
    pub fn opposite(&self) -> Self {
        match self {
            SignalIntent::Long => SignalIntent::Short,
            SignalIntent::Short => SignalIntent::Long,
            SignalIntent::Flat => SignalIntent::Flat,
        }
    }

    /// Check if intent requires a position change from current state
    ///
    /// # Arguments
    /// - `current_exposure`: Current signed position size (positive = long, negative = short, zero = flat)
    pub fn requires_change(&self, current_exposure: i32) -> bool {
        match self {
            SignalIntent::Long => current_exposure <= 0,
            SignalIntent::Short => current_exposure >= 0,
            SignalIntent::Flat => current_exposure != 0,
        }
    }
}

/// Optional signal strength/confidence (for advanced use)
///
/// Not used in M7 MVP, but reserved for future enhancements
/// (e.g., position scaling based on signal strength).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SignalStrength {
    /// Intent direction
    pub intent: SignalIntent,

    /// Strength/confidence [0.0, 1.0]
    /// 0.0 = weak signal, 1.0 = strong conviction
    pub strength: f64,
}

impl SignalStrength {
    pub fn new(intent: SignalIntent, strength: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&strength),
            "strength must be in [0.0, 1.0]"
        );
        Self { intent, strength }
    }

    /// Create maximum strength signal
    pub fn strong(intent: SignalIntent) -> Self {
        Self {
            intent,
            strength: 1.0,
        }
    }

    /// Create weak signal
    pub fn weak(intent: SignalIntent) -> Self {
        Self {
            intent,
            strength: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_intent_is_directional() {
        assert!(SignalIntent::Long.is_directional());
        assert!(SignalIntent::Short.is_directional());
        assert!(!SignalIntent::Flat.is_directional());
    }

    #[test]
    fn test_signal_intent_opposite() {
        assert_eq!(SignalIntent::Long.opposite(), SignalIntent::Short);
        assert_eq!(SignalIntent::Short.opposite(), SignalIntent::Long);
        assert_eq!(SignalIntent::Flat.opposite(), SignalIntent::Flat);
    }

    #[test]
    fn test_requires_change() {
        // Long intent
        assert!(SignalIntent::Long.requires_change(0)); // flat → long
        assert!(SignalIntent::Long.requires_change(-100)); // short → long
        assert!(!SignalIntent::Long.requires_change(100)); // already long

        // Short intent
        assert!(SignalIntent::Short.requires_change(0)); // flat → short
        assert!(SignalIntent::Short.requires_change(100)); // long → short
        assert!(!SignalIntent::Short.requires_change(-100)); // already short

        // Flat intent
        assert!(SignalIntent::Flat.requires_change(100)); // long → flat
        assert!(SignalIntent::Flat.requires_change(-100)); // short → flat
        assert!(!SignalIntent::Flat.requires_change(0)); // already flat
    }

    #[test]
    fn test_signal_strength_bounds() {
        let strong = SignalStrength::strong(SignalIntent::Long);
        assert_eq!(strong.strength, 1.0);

        let weak = SignalStrength::weak(SignalIntent::Short);
        assert_eq!(weak.strength, 0.0);
    }

    #[test]
    #[should_panic(expected = "strength must be in [0.0, 1.0]")]
    fn test_signal_strength_invalid() {
        SignalStrength::new(SignalIntent::Long, 1.5);
    }
}
