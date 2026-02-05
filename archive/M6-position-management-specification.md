# M6 — Position Management + Anti-Stickiness Specification

**Progress:** `[..............ᗧ·] 95% (finalizing)`

## Overview

M6 implements **position management** with explicit anti-stickiness guarantees and a ratchet invariant to prevent the two chronic bugs that plague traditional backtesting systems:

1. **Strategy stickiness** — exits "chase" highs and never let you exit
2. **Volatility trap** — expanding ATR loosens stops and gives back gains

### Key Architectural Principles

1. **Intent-based, not fill-based**: Position managers emit **order intents** (cancel/replace instructions), never direct fills
2. **Separation of concerns**: PM depends on portfolio state but never touches execution
3. **Ratchet invariant**: Stops may only tighten, never loosen (even if ATR expands)
4. **Anti-stickiness**: Chandelier/floor logic uses **snapshot reference levels**, not chasing current price

---

## File Structure

### Module Organization (8 files)

```
trendlab-core/src/position_management/
├── mod.rs                    # Module root + exports
├── manager.rs                # PositionManager trait + registry
├── intent.rs                 # OrderIntent, CancelReplaceIntent
├── ratchet.rs                # RatchetState, ratchet enforcement logic
├── strategies/
│   ├── mod.rs                # Strategy module exports
│   ├── fixed_percent.rs      # Fixed % stop loss
│   ├── atr_stop.rs           # ATR-based stop (with ratchet)
│   ├── chandelier.rs         # Chandelier exit (anti-stickiness)
│   └── time_stop.rs          # Time-based exit
└── tests.rs                  # Unit tests for all PM strategies
```

---

## 1. Core Data Structures

### File: `trendlab-core/src/position_management/intent.rs`

```rust
use crate::domain::{OrderId, PositionId, Side};
use crate::orders::OrderType;
use rust_decimal::Decimal;

/// Order intent emitted by position manager
/// These are instructions for the order book, not direct fills
#[derive(Debug, Clone, PartialEq)]
pub enum OrderIntent {
    /// Place a new order
    New {
        position_id: PositionId,
        order_type: OrderType,
        quantity: Decimal,
    },

    /// Cancel an existing order
    Cancel {
        order_id: OrderId,
        reason: CancelReason,
    },

    /// Cancel and replace (atomic operation)
    CancelReplace {
        old_order_id: OrderId,
        new_order_type: OrderType,
        new_quantity: Decimal,
        reason: ReplaceReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    /// Position closed
    PositionClosed,

    /// Stop ratcheted (tightened)
    StopRatcheted,

    /// Time stop triggered
    TimeExpired,

    /// Take profit hit
    TakeProfitHit,

    /// Manual cancellation
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceReason {
    /// Stop tightened (ratchet)
    StopTightened,

    /// Take profit updated
    TakeProfitUpdated,

    /// Quantity scaled
    QuantityScaled,
}

impl OrderIntent {
    /// Create a new order intent
    pub fn new_order(
        position_id: PositionId,
        order_type: OrderType,
        quantity: Decimal,
    ) -> Self {
        Self::New {
            position_id,
            order_type,
            quantity,
        }
    }

    /// Create a cancel intent
    pub fn cancel(order_id: OrderId, reason: CancelReason) -> Self {
        Self::Cancel { order_id, reason }
    }

    /// Create a cancel-replace intent
    pub fn cancel_replace(
        old_order_id: OrderId,
        new_order_type: OrderType,
        new_quantity: Decimal,
        reason: ReplaceReason,
    ) -> Self {
        Self::CancelReplace {
            old_order_id,
            new_order_type,
            new_quantity,
            reason,
        }
    }
}
```

---

### File: `trendlab-core/src/position_management/ratchet.rs`

```rust
use rust_decimal::Decimal;
use crate::domain::Side;
use thiserror::Error;

/// Ratchet state tracks the "tightest allowed" stop level
/// Ensures stops only tighten, never loosen (even if ATR expands)
#[derive(Debug, Clone, PartialEq)]
pub struct RatchetState {
    /// Current ratchet floor (for longs) or ceiling (for shorts)
    pub current_level: Decimal,

    /// Side of the position
    pub side: Side,

    /// Whether ratcheting is enabled
    pub enabled: bool,
}

#[derive(Debug, Error)]
pub enum RatchetError {
    #[error("Cannot loosen stop: proposed {proposed}, current ratchet {current}")]
    WouldLoosen {
        proposed: Decimal,
        current: Decimal,
    },

    #[error("Ratcheting disabled for this position")]
    Disabled,
}

impl RatchetState {
    /// Create a new ratchet state
    pub fn new(initial_level: Decimal, side: Side) -> Self {
        Self {
            current_level: initial_level,
            side,
            enabled: true,
        }
    }

    /// Create a disabled ratchet (allows any stop level)
    pub fn disabled(side: Side) -> Self {
        Self {
            current_level: Decimal::ZERO,
            side,
            enabled: false,
        }
    }

    /// Attempt to update the ratchet level
    /// Returns Ok(new_level) if tightening or unchanged
    /// Returns Err if loosening
    pub fn try_update(&mut self, proposed_level: Decimal) -> Result<Decimal, RatchetError> {
        if !self.enabled {
            // If disabled, allow any level
            self.current_level = proposed_level;
            return Ok(proposed_level);
        }

        match self.side {
            Side::Long => {
                // For longs: stop can only move UP (tighten)
                if proposed_level < self.current_level {
                    return Err(RatchetError::WouldLoosen {
                        proposed: proposed_level,
                        current: self.current_level,
                    });
                }
                self.current_level = proposed_level;
                Ok(proposed_level)
            }
            Side::Short => {
                // For shorts: stop can only move DOWN (tighten)
                if proposed_level > self.current_level {
                    return Err(RatchetError::WouldLoosen {
                        proposed: proposed_level,
                        current: self.current_level,
                    });
                }
                self.current_level = proposed_level;
                Ok(proposed_level)
            }
        }
    }

    /// Force update (bypass ratchet check, use cautiously)
    pub fn force_update(&mut self, new_level: Decimal) {
        self.current_level = new_level;
    }

    /// Check if a proposed level would tighten
    pub fn would_tighten(&self, proposed_level: Decimal) -> bool {
        if !self.enabled {
            return true; // disabled ratchet accepts any level
        }

        match self.side {
            Side::Long => proposed_level >= self.current_level,
            Side::Short => proposed_level <= self.current_level,
        }
    }

    /// Get current ratchet level
    pub fn current(&self) -> Decimal {
        self.current_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_long_ratchet_tightening() {
        let mut ratchet = RatchetState::new(dec!(95.0), Side::Long);

        // Tighten from 95 → 96 (OK)
        assert!(ratchet.try_update(dec!(96.0)).is_ok());
        assert_eq!(ratchet.current(), dec!(96.0));

        // Tighten again 96 → 97 (OK)
        assert!(ratchet.try_update(dec!(97.0)).is_ok());
        assert_eq!(ratchet.current(), dec!(97.0));
    }

    #[test]
    fn test_long_ratchet_prevents_loosening() {
        let mut ratchet = RatchetState::new(dec!(95.0), Side::Long);

        // Tighten to 97
        ratchet.try_update(dec!(97.0)).unwrap();

        // Try to loosen back to 95 (ERROR)
        let result = ratchet.try_update(dec!(95.0));
        assert!(result.is_err());
        assert_eq!(ratchet.current(), dec!(97.0)); // unchanged
    }

    #[test]
    fn test_short_ratchet_tightening() {
        let mut ratchet = RatchetState::new(dec!(105.0), Side::Short);

        // Tighten from 105 → 104 (OK for shorts, stop moves DOWN)
        assert!(ratchet.try_update(dec!(104.0)).is_ok());
        assert_eq!(ratchet.current(), dec!(104.0));
    }

    #[test]
    fn test_short_ratchet_prevents_loosening() {
        let mut ratchet = RatchetState::new(dec!(105.0), Side::Short);

        // Tighten to 103
        ratchet.try_update(dec!(103.0)).unwrap();

        // Try to loosen back to 105 (ERROR)
        let result = ratchet.try_update(dec!(105.0));
        assert!(result.is_err());
        assert_eq!(ratchet.current(), dec!(103.0)); // unchanged
    }

    #[test]
    fn test_disabled_ratchet_allows_any_level() {
        let mut ratchet = RatchetState::disabled(Side::Long);

        // Can move in any direction
        assert!(ratchet.try_update(dec!(100.0)).is_ok());
        assert!(ratchet.try_update(dec!(95.0)).is_ok());
        assert!(ratchet.try_update(dec!(110.0)).is_ok());
    }
}
```

---

### File: `trendlab-core/src/position_management/manager.rs`

```rust
use crate::domain::{Bar, PositionId, Side};
use crate::portfolio::Position;
use crate::position_management::intent::OrderIntent;
use std::collections::HashMap;
use thiserror::Error;

/// Position manager trait
/// All PM strategies implement this trait
pub trait PositionManager: Send + Sync {
    /// Update position management for a single position at current bar
    /// Returns list of order intents to execute
    fn update(
        &mut self,
        position: &Position,
        current_bar: &Bar,
        bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError>;

    /// Called when position is opened (initialization)
    fn on_position_opened(
        &mut self,
        position: &Position,
        entry_bar: &Bar,
        bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError>;

    /// Called when position is closed (cleanup)
    fn on_position_closed(&mut self, position_id: PositionId) -> Result<Vec<OrderIntent>, PmError>;

    /// Get strategy name
    fn name(&self) -> &str;

    /// Clone as trait object
    fn clone_box(&self) -> Box<dyn PositionManager>;
}

#[derive(Debug, Error)]
pub enum PmError {
    #[error("Position {0} not found in PM state")]
    PositionNotFound(PositionId),

    #[error("Ratchet error: {0}")]
    Ratchet(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Calculation error: {0}")]
    Calculation(String),
}

/// Position manager registry
/// Manages multiple PM strategies for different positions
pub struct PmRegistry {
    managers: HashMap<PositionId, Box<dyn PositionManager>>,
}

impl PmRegistry {
    pub fn new() -> Self {
        Self {
            managers: HashMap::new(),
        }
    }

    /// Register a PM strategy for a position
    pub fn register(&mut self, position_id: PositionId, manager: Box<dyn PositionManager>) {
        self.managers.insert(position_id, manager);
    }

    /// Update all positions
    pub fn update_all(
        &mut self,
        positions: &HashMap<PositionId, Position>,
        current_bar: &Bar,
        bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError> {
        let mut all_intents = Vec::new();

        for (pos_id, position) in positions.iter() {
            if let Some(manager) = self.managers.get_mut(pos_id) {
                let intents = manager.update(position, current_bar, bar_index)?;
                all_intents.extend(intents);
            }
        }

        Ok(all_intents)
    }

    /// Handle position opened event
    pub fn on_position_opened(
        &mut self,
        position: &Position,
        entry_bar: &Bar,
        bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError> {
        if let Some(manager) = self.managers.get_mut(&position.id) {
            manager.on_position_opened(position, entry_bar, bar_index)
        } else {
            Ok(vec![])
        }
    }

    /// Handle position closed event
    pub fn on_position_closed(&mut self, position_id: PositionId) -> Result<Vec<OrderIntent>, PmError> {
        if let Some(mut manager) = self.managers.remove(&position_id) {
            manager.on_position_closed(position_id)
        } else {
            Ok(vec![])
        }
    }

    /// Remove a position's PM strategy
    pub fn remove(&mut self, position_id: &PositionId) {
        self.managers.remove(position_id);
    }

    /// Get manager for a position (immutable)
    pub fn get(&self, position_id: &PositionId) -> Option<&Box<dyn PositionManager>> {
        self.managers.get(position_id)
    }

    /// Get manager for a position (mutable)
    pub fn get_mut(&mut self, position_id: &PositionId) -> Option<&mut Box<dyn PositionManager>> {
        self.managers.get_mut(position_id)
    }
}

impl Default for PmRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## 2. MVP Position Management Strategies

### File: `trendlab-core/src/position_management/strategies/fixed_percent.rs`

```rust
use crate::domain::{Bar, PositionId, Side, OrderId};
use crate::portfolio::Position;
use crate::position_management::{PositionManager, OrderIntent, CancelReason, PmError};
use crate::orders::OrderType;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Fixed percentage stop loss
/// Places a stop at entry_price * (1 - stop_pct) for longs
#[derive(Debug, Clone)]
pub struct FixedPercentStop {
    /// Stop loss percentage (e.g., 0.05 = 5%)
    pub stop_pct: Decimal,

    /// Track active stop orders per position
    stop_orders: HashMap<PositionId, OrderId>,
}

impl FixedPercentStop {
    pub fn new(stop_pct: Decimal) -> Result<Self, PmError> {
        if stop_pct <= Decimal::ZERO || stop_pct >= Decimal::ONE {
            return Err(PmError::InvalidConfig(
                format!("stop_pct must be in (0, 1), got {}", stop_pct)
            ));
        }

        Ok(Self {
            stop_pct,
            stop_orders: HashMap::new(),
        })
    }

    fn calculate_stop_price(&self, entry_price: Decimal, side: Side) -> Decimal {
        match side {
            Side::Long => entry_price * (Decimal::ONE - self.stop_pct),
            Side::Short => entry_price * (Decimal::ONE + self.stop_pct),
        }
    }
}

impl PositionManager for FixedPercentStop {
    fn update(
        &mut self,
        _position: &Position,
        _current_bar: &Bar,
        _bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError> {
        // Fixed stop doesn't change after entry
        Ok(vec![])
    }

    fn on_position_opened(
        &mut self,
        position: &Position,
        _entry_bar: &Bar,
        _bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError> {
        let stop_price = self.calculate_stop_price(position.entry_price, position.side);

        let order_type = match position.side {
            Side::Long => OrderType::StopMarket {
                trigger_price: stop_price,
                side: Side::Short, // exit long
            },
            Side::Short => OrderType::StopMarket {
                trigger_price: stop_price,
                side: Side::Long, // exit short
            },
        };

        Ok(vec![OrderIntent::new_order(
            position.id,
            order_type,
            position.quantity,
        )])
    }

    fn on_position_closed(&mut self, position_id: PositionId) -> Result<Vec<OrderIntent>, PmError> {
        // If we tracked the stop order, cancel it
        if let Some(order_id) = self.stop_orders.remove(&position_id) {
            Ok(vec![OrderIntent::cancel(order_id, CancelReason::PositionClosed)])
        } else {
            Ok(vec![])
        }
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

    #[test]
    fn test_fixed_percent_stop_creation() {
        // Valid
        assert!(FixedPercentStop::new(dec!(0.05)).is_ok());

        // Invalid: zero
        assert!(FixedPercentStop::new(dec!(0.0)).is_err());

        // Invalid: >= 1
        assert!(FixedPercentStop::new(dec!(1.0)).is_err());
        assert!(FixedPercentStop::new(dec!(1.5)).is_err());
    }

    #[test]
    fn test_long_stop_calculation() {
        let pm = FixedPercentStop::new(dec!(0.05)).unwrap();

        // Entry at 100, 5% stop → 95
        let stop = pm.calculate_stop_price(dec!(100.0), Side::Long);
        assert_eq!(stop, dec!(95.0));
    }

    #[test]
    fn test_short_stop_calculation() {
        let pm = FixedPercentStop::new(dec!(0.05)).unwrap();

        // Entry at 100, 5% stop → 105
        let stop = pm.calculate_stop_price(dec!(100.0), Side::Short);
        assert_eq!(stop, dec!(105.0));
    }
}
```

---

### File: `trendlab-core/src/position_management/strategies/atr_stop.rs`

```rust
use crate::domain::{Bar, PositionId, Side, OrderId};
use crate::portfolio::Position;
use crate::position_management::{
    PositionManager, OrderIntent, CancelReason, ReplaceReason, PmError, RatchetState,
};
use crate::orders::OrderType;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// ATR-based stop loss with ratchet
/// Stop = entry_price - (atr_multiple * ATR) for longs
/// With ratchet enabled, stop can only tighten
#[derive(Debug, Clone)]
pub struct AtrStop {
    /// ATR multiple (e.g., 2.0 = 2x ATR)
    pub atr_multiple: Decimal,

    /// Enable ratchet (default: true)
    pub ratchet_enabled: bool,

    /// Track ratchet state per position
    ratchets: HashMap<PositionId, RatchetState>,

    /// Track active stop orders per position
    stop_orders: HashMap<PositionId, OrderId>,
}

impl AtrStop {
    pub fn new(atr_multiple: Decimal) -> Result<Self, PmError> {
        Self::with_ratchet(atr_multiple, true)
    }

    pub fn with_ratchet(atr_multiple: Decimal, ratchet_enabled: bool) -> Result<Self, PmError> {
        if atr_multiple <= Decimal::ZERO {
            return Err(PmError::InvalidConfig(
                format!("atr_multiple must be > 0, got {}", atr_multiple)
            ));
        }

        Ok(Self {
            atr_multiple,
            ratchet_enabled,
            ratchets: HashMap::new(),
            stop_orders: HashMap::new(),
        })
    }

    fn calculate_stop_price(&self, current_price: Decimal, atr: Decimal, side: Side) -> Decimal {
        let offset = self.atr_multiple * atr;
        match side {
            Side::Long => current_price - offset,
            Side::Short => current_price + offset,
        }
    }
}

impl PositionManager for AtrStop {
    fn update(
        &mut self,
        position: &Position,
        current_bar: &Bar,
        _bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError> {
        // Get ATR from bar metadata (assumes ATR pre-computed)
        let atr = current_bar.metadata.get("atr")
            .ok_or_else(|| PmError::Calculation("ATR not found in bar metadata".to_string()))?;

        let proposed_stop = self.calculate_stop_price(current_bar.close, *atr, position.side);

        // Get or create ratchet state
        let ratchet = self.ratchets.entry(position.id).or_insert_with(|| {
            if self.ratchet_enabled {
                RatchetState::new(proposed_stop, position.side)
            } else {
                RatchetState::disabled(position.side)
            }
        });

        // Try to update ratchet
        match ratchet.try_update(proposed_stop) {
            Ok(new_stop) => {
                // Stop tightened or unchanged
                if new_stop != ratchet.current() {
                    // Emit cancel-replace intent
                    if let Some(&order_id) = self.stop_orders.get(&position.id) {
                        let new_order_type = match position.side {
                            Side::Long => OrderType::StopMarket {
                                trigger_price: new_stop,
                                side: Side::Short,
                            },
                            Side::Short => OrderType::StopMarket {
                                trigger_price: new_stop,
                                side: Side::Long,
                            },
                        };

                        Ok(vec![OrderIntent::cancel_replace(
                            order_id,
                            new_order_type,
                            position.quantity,
                            ReplaceReason::StopTightened,
                        )])
                    } else {
                        Ok(vec![])
                    }
                } else {
                    Ok(vec![])
                }
            }
            Err(_) => {
                // Ratchet prevented loosening - no update
                Ok(vec![])
            }
        }
    }

    fn on_position_opened(
        &mut self,
        position: &Position,
        entry_bar: &Bar,
        _bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError> {
        // Get ATR from entry bar
        let atr = entry_bar.metadata.get("atr")
            .ok_or_else(|| PmError::Calculation("ATR not found in bar metadata".to_string()))?;

        let initial_stop = self.calculate_stop_price(position.entry_price, *atr, position.side);

        // Initialize ratchet
        let ratchet = if self.ratchet_enabled {
            RatchetState::new(initial_stop, position.side)
        } else {
            RatchetState::disabled(position.side)
        };
        self.ratchets.insert(position.id, ratchet);

        let order_type = match position.side {
            Side::Long => OrderType::StopMarket {
                trigger_price: initial_stop,
                side: Side::Short,
            },
            Side::Short => OrderType::StopMarket {
                trigger_price: initial_stop,
                side: Side::Long,
            },
        };

        Ok(vec![OrderIntent::new_order(
            position.id,
            order_type,
            position.quantity,
        )])
    }

    fn on_position_closed(&mut self, position_id: PositionId) -> Result<Vec<OrderIntent>, PmError> {
        self.ratchets.remove(&position_id);

        if let Some(order_id) = self.stop_orders.remove(&position_id) {
            Ok(vec![OrderIntent::cancel(order_id, CancelReason::PositionClosed)])
        } else {
            Ok(vec![])
        }
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

    #[test]
    fn test_atr_stop_prevents_loosening() {
        let mut pm = AtrStop::new(dec!(2.0)).unwrap();

        // Create mock position
        let position = Position {
            id: PositionId::new(),
            entry_price: dec!(100.0),
            side: Side::Long,
            quantity: dec!(100.0),
            ..Default::default()
        };

        // Bar 1: ATR = 5, close = 105 → stop at 105 - 10 = 95
        let bar1 = Bar {
            close: dec!(105.0),
            metadata: [("atr".to_string(), dec!(5.0))].into(),
            ..Default::default()
        };

        let _ = pm.on_position_opened(&position, &bar1, 0);

        // Bar 2: ATR = 3 (contracted), close = 108 → proposed stop = 108 - 6 = 102
        // Ratchet allows tightening from 95 → 102
        let bar2 = Bar {
            close: dec!(108.0),
            metadata: [("atr".to_string(), dec!(3.0))].into(),
            ..Default::default()
        };

        let intents = pm.update(&position, &bar2, 1).unwrap();
        assert_eq!(intents.len(), 1); // Cancel-replace to tighten

        // Bar 3: ATR = 10 (expanded!), close = 110 → proposed stop = 110 - 20 = 90
        // Ratchet PREVENTS loosening from 102 → 90
        let bar3 = Bar {
            close: dec!(110.0),
            metadata: [("atr".to_string(), dec!(10.0))].into(),
            ..Default::default()
        };

        let intents = pm.update(&position, &bar3, 2).unwrap();
        assert_eq!(intents.len(), 0); // No update, ratchet blocked it

        // Verify ratchet still at 102
        let ratchet = pm.ratchets.get(&position.id).unwrap();
        assert_eq!(ratchet.current(), dec!(102.0));
    }

    #[test]
    fn test_atr_stop_without_ratchet_allows_loosening() {
        let mut pm = AtrStop::with_ratchet(dec!(2.0), false).unwrap();

        let position = Position {
            id: PositionId::new(),
            entry_price: dec!(100.0),
            side: Side::Long,
            quantity: dec!(100.0),
            ..Default::default()
        };

        let bar1 = Bar {
            close: dec!(105.0),
            metadata: [("atr".to_string(), dec!(5.0))].into(),
            ..Default::default()
        };

        let _ = pm.on_position_opened(&position, &bar1, 0);

        // ATR expands → stop loosens (no ratchet)
        let bar2 = Bar {
            close: dec!(110.0),
            metadata: [("atr".to_string(), dec!(10.0))].into(),
            ..Default::default()
        };

        let intents = pm.update(&position, &bar2, 1).unwrap();
        assert_eq!(intents.len(), 1); // Cancel-replace allowed
    }
}
```

---

### File: `trendlab-core/src/position_management/strategies/chandelier.rs`

```rust
use crate::domain::{Bar, PositionId, Side, OrderId};
use crate::portfolio::Position;
use crate::position_management::{
    PositionManager, OrderIntent, CancelReason, ReplaceReason, PmError, RatchetState,
};
use crate::orders::OrderType;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Chandelier exit (anti-stickiness version)
/// Stop = HHV(close, lookback) - (atr_multiple * ATR) for longs
///
/// **Anti-stickiness guarantee:**
/// - HHV is "snapshot" from when ratchet last tightened
/// - Does NOT update HHV every bar (that would chase price upward)
/// - Only recomputes HHV when stop would tighten
#[derive(Debug, Clone)]
pub struct ChandelierExit {
    /// Lookback period for highest high / lowest low
    pub lookback: usize,

    /// ATR multiple
    pub atr_multiple: Decimal,

    /// Track ratchet state per position
    ratchets: HashMap<PositionId, RatchetState>,

    /// Track snapshot HHV/LLV per position (anti-stickiness)
    snapshot_refs: HashMap<PositionId, Decimal>,

    /// Track active stop orders
    stop_orders: HashMap<PositionId, OrderId>,

    /// Ring buffer for recent closes (for HHV/LLV calculation)
    recent_closes: HashMap<PositionId, Vec<Decimal>>,
}

impl ChandelierExit {
    pub fn new(lookback: usize, atr_multiple: Decimal) -> Result<Self, PmError> {
        if lookback == 0 {
            return Err(PmError::InvalidConfig("lookback must be > 0".to_string()));
        }
        if atr_multiple <= Decimal::ZERO {
            return Err(PmError::InvalidConfig(
                format!("atr_multiple must be > 0, got {}", atr_multiple)
            ));
        }

        Ok(Self {
            lookback,
            atr_multiple,
            ratchets: HashMap::new(),
            snapshot_refs: HashMap::new(),
            stop_orders: HashMap::new(),
            recent_closes: HashMap::new(),
        })
    }

    /// Calculate HHV (highest high value) for longs, LLV (lowest low value) for shorts
    fn calculate_reference(&self, closes: &[Decimal], side: Side) -> Decimal {
        match side {
            Side::Long => {
                // HHV (highest close in lookback)
                closes.iter().copied().max().unwrap_or(Decimal::ZERO)
            }
            Side::Short => {
                // LLV (lowest close in lookback)
                closes.iter().copied().min().unwrap_or(Decimal::ZERO)
            }
        }
    }

    fn calculate_stop_from_ref(&self, reference: Decimal, atr: Decimal, side: Side) -> Decimal {
        let offset = self.atr_multiple * atr;
        match side {
            Side::Long => reference - offset,
            Side::Short => reference + offset,
        }
    }
}

impl PositionManager for ChandelierExit {
    fn update(
        &mut self,
        position: &Position,
        current_bar: &Bar,
        _bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError> {
        // Update recent closes ring buffer
        let closes = self.recent_closes.entry(position.id).or_insert_with(Vec::new);
        closes.push(current_bar.close);
        if closes.len() > self.lookback {
            closes.remove(0);
        }

        // Get ATR
        let atr = current_bar.metadata.get("atr")
            .ok_or_else(|| PmError::Calculation("ATR not found in bar metadata".to_string()))?;

        // Calculate current reference (HHV/LLV)
        let current_ref = self.calculate_reference(closes, position.side);

        // Get snapshot reference (anti-stickiness: don't update unless tightening)
        let snapshot_ref = self.snapshot_refs.get(&position.id).copied().unwrap_or(current_ref);

        // Calculate proposed stop from CURRENT reference
        let proposed_stop = self.calculate_stop_from_ref(current_ref, *atr, position.side);

        // Get ratchet
        let ratchet = self.ratchets.entry(position.id).or_insert_with(|| {
            RatchetState::new(proposed_stop, position.side)
        });

        // Check if stop would tighten
        if ratchet.would_tighten(proposed_stop) {
            // ONLY NOW update snapshot reference
            self.snapshot_refs.insert(position.id, current_ref);

            // Update ratchet
            let new_stop = ratchet.try_update(proposed_stop)?;

            // Emit cancel-replace
            if let Some(&order_id) = self.stop_orders.get(&position.id) {
                let new_order_type = match position.side {
                    Side::Long => OrderType::StopMarket {
                        trigger_price: new_stop,
                        side: Side::Short,
                    },
                    Side::Short => OrderType::StopMarket {
                        trigger_price: new_stop,
                        side: Side::Long,
                    },
                };

                Ok(vec![OrderIntent::cancel_replace(
                    order_id,
                    new_order_type,
                    position.quantity,
                    ReplaceReason::StopTightened,
                )])
            } else {
                Ok(vec![])
            }
        } else {
            // Stop would loosen or stay same → don't update snapshot, don't move stop
            Ok(vec![])
        }
    }

    fn on_position_opened(
        &mut self,
        position: &Position,
        entry_bar: &Bar,
        _bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError> {
        // Initialize recent closes with entry bar
        let mut closes = Vec::new();
        closes.push(entry_bar.close);
        self.recent_closes.insert(position.id, closes);

        // Get ATR
        let atr = entry_bar.metadata.get("atr")
            .ok_or_else(|| PmError::Calculation("ATR not found in bar metadata".to_string()))?;

        // Initial reference = entry price
        let initial_ref = entry_bar.close;
        self.snapshot_refs.insert(position.id, initial_ref);

        // Calculate initial stop
        let initial_stop = self.calculate_stop_from_ref(initial_ref, *atr, position.side);

        // Initialize ratchet
        let ratchet = RatchetState::new(initial_stop, position.side);
        self.ratchets.insert(position.id, ratchet);

        let order_type = match position.side {
            Side::Long => OrderType::StopMarket {
                trigger_price: initial_stop,
                side: Side::Short,
            },
            Side::Short => OrderType::StopMarket {
                trigger_price: initial_stop,
                side: Side::Long,
            },
        };

        Ok(vec![OrderIntent::new_order(
            position.id,
            order_type,
            position.quantity,
        )])
    }

    fn on_position_closed(&mut self, position_id: PositionId) -> Result<Vec<OrderIntent>, PmError> {
        self.ratchets.remove(&position_id);
        self.snapshot_refs.remove(&position_id);
        self.recent_closes.remove(&position_id);

        if let Some(order_id) = self.stop_orders.remove(&position_id) {
            Ok(vec![OrderIntent::cancel(order_id, CancelReason::PositionClosed)])
        } else {
            Ok(vec![])
        }
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

    #[test]
    fn test_chandelier_anti_stickiness() {
        // This is the key anti-stickiness regression test
        let mut pm = ChandelierExit::new(3, dec!(2.0)).unwrap();

        let position = Position {
            id: PositionId::new(),
            entry_price: dec!(100.0),
            side: Side::Long,
            quantity: dec!(100.0),
            ..Default::default()
        };

        // Bar 0: Entry at 100, ATR = 5 → stop at 100 - 10 = 90
        let bar0 = Bar {
            close: dec!(100.0),
            metadata: [("atr".to_string(), dec!(5.0))].into(),
            ..Default::default()
        };
        let _ = pm.on_position_opened(&position, &bar0, 0);

        // Bar 1: Rally to 110 → HHV = 110, stop tightens to 110 - 10 = 100
        let bar1 = Bar {
            close: dec!(110.0),
            metadata: [("atr".to_string(), dec!(5.0))].into(),
            ..Default::default()
        };
        let intents = pm.update(&position, &bar1, 1).unwrap();
        assert_eq!(intents.len(), 1); // Tightened

        // Bar 2: Price falls to 105
        // Current HHV = 110 (from lookback), stop would be 110 - 10 = 100
        // Snapshot HHV is STILL 110 (locked in when stop tightened)
        // Stop stays at 100 (no loosening, no chasing)
        let bar2 = Bar {
            close: dec!(105.0),
            metadata: [("atr".to_string(), dec!(5.0))].into(),
            ..Default::default()
        };
        let intents = pm.update(&position, &bar2, 2).unwrap();
        assert_eq!(intents.len(), 0); // No update

        // Bar 3: Price rallies to 115 → HHV = 115, stop would tighten to 115 - 10 = 105
        // NOW snapshot updates to 115
        let bar3 = Bar {
            close: dec!(115.0),
            metadata: [("atr".to_string(), dec!(5.0))].into(),
            ..Default::default()
        };
        let intents = pm.update(&position, &bar3, 3).unwrap();
        assert_eq!(intents.len(), 1); // Tightened to 105

        // Verify ratchet at 105
        let ratchet = pm.ratchets.get(&position.id).unwrap();
        assert_eq!(ratchet.current(), dec!(105.0));
    }
}
```

---

### File: `trendlab-core/src/position_management/strategies/time_stop.rs`

```rust
use crate::domain::{Bar, PositionId};
use crate::portfolio::Position;
use crate::position_management::{PositionManager, OrderIntent, CancelReason, PmError};
use crate::orders::OrderType;
use std::collections::HashMap;

/// Time-based exit
/// Closes position after N bars regardless of P&L
#[derive(Debug, Clone)]
pub struct TimeStop {
    /// Maximum number of bars to hold position
    pub max_bars: usize,

    /// Track entry bar index per position
    entry_bars: HashMap<PositionId, usize>,
}

impl TimeStop {
    pub fn new(max_bars: usize) -> Result<Self, PmError> {
        if max_bars == 0 {
            return Err(PmError::InvalidConfig("max_bars must be > 0".to_string()));
        }

        Ok(Self {
            max_bars,
            entry_bars: HashMap::new(),
        })
    }
}

impl PositionManager for TimeStop {
    fn update(
        &mut self,
        position: &Position,
        current_bar: &Bar,
        bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError> {
        if let Some(&entry_bar_index) = self.entry_bars.get(&position.id) {
            let bars_held = bar_index - entry_bar_index;

            if bars_held >= self.max_bars {
                // Time to exit - emit MOC (market on close) order
                let order_type = OrderType::MarketOnClose {
                    side: position.side.opposite(),
                };

                Ok(vec![OrderIntent::new_order(
                    position.id,
                    order_type,
                    position.quantity,
                )])
            } else {
                Ok(vec![])
            }
        } else {
            Ok(vec![])
        }
    }

    fn on_position_opened(
        &mut self,
        position: &Position,
        _entry_bar: &Bar,
        bar_index: usize,
    ) -> Result<Vec<OrderIntent>, PmError> {
        self.entry_bars.insert(position.id, bar_index);
        Ok(vec![])
    }

    fn on_position_closed(&mut self, position_id: PositionId) -> Result<Vec<OrderIntent>, PmError> {
        self.entry_bars.remove(&position_id);
        Ok(vec![])
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
    use crate::domain::Side;

    #[test]
    fn test_time_stop_triggers_after_max_bars() {
        let mut pm = TimeStop::new(5).unwrap();

        let position = Position {
            id: PositionId::new(),
            entry_price: dec!(100.0),
            side: Side::Long,
            quantity: dec!(100.0),
            ..Default::default()
        };

        let bar = Bar::default();

        // Entry at bar 0
        let _ = pm.on_position_opened(&position, &bar, 0);

        // Bars 0-4: no exit
        for i in 0..5 {
            let intents = pm.update(&position, &bar, i).unwrap();
            assert_eq!(intents.len(), 0);
        }

        // Bar 5: time stop triggers
        let intents = pm.update(&position, &bar, 5).unwrap();
        assert_eq!(intents.len(), 1);

        match &intents[0] {
            OrderIntent::New { order_type, .. } => {
                match order_type {
                    OrderType::MarketOnClose { .. } => {},
                    _ => panic!("Expected MOC order"),
                }
            },
            _ => panic!("Expected New order intent"),
        }
    }
}
```

---

## 3. BDD Scenarios (Cucumber)

### Feature 1: Ratchet Invariant

**File:** `trendlab-core/tests/bdd_position_management_ratchet.feature`

```gherkin
Feature: Ratchet invariant prevents stop loosening

  Background:
    Given an ATR-based stop manager with ATR multiple 2.0
    And ratcheting is enabled
    And a long position at entry price 100.0

  Scenario: Volatility expansion does not loosen stop
    Given the position has initial ATR 5.0 and stop at 90.0
    When price rises to 110.0 and ATR contracts to 3.0
    Then stop tightens to 104.0
    When price stays at 110.0 but ATR expands to 10.0
    Then stop remains at 104.0
    And the ratchet blocks loosening to 90.0

  Scenario: Stop can tighten multiple times
    Given the position has initial ATR 5.0 and stop at 90.0
    When price rises to 105.0 and ATR is 4.0
    Then stop tightens to 97.0
    When price rises to 108.0 and ATR is 3.0
    Then stop tightens to 102.0
    And the ratchet level is 102.0

  Scenario: Disabled ratchet allows stop loosening
    Given ratcheting is disabled
    And the position has initial ATR 5.0 and stop at 90.0
    When price rises to 110.0 and ATR is 3.0
    Then stop updates to 104.0
    When price stays at 110.0 but ATR expands to 10.0
    Then stop loosens to 90.0
    And no ratchet error occurs
```

---

### Feature 2: Anti-Stickiness (Chandelier)

**File:** `trendlab-core/tests/bdd_position_management_chandelier.feature`

```gherkin
Feature: Chandelier exit prevents stickiness by using snapshot reference levels

  Background:
    Given a chandelier exit manager with lookback 3 and ATR multiple 2.0
    And a long position at entry price 100.0

  Scenario: Chandelier allows profitable exit on rise-then-fall path
    # This is the key anti-stickiness regression test
    Given entry bar has close 100.0 and ATR 5.0
    And initial stop is 90.0 (100 - 2*5)
    When bar 1 closes at 110.0 with ATR 5.0
    Then HHV becomes 110.0
    And stop tightens to 100.0 (110 - 2*5)
    And snapshot HHV is locked at 110.0
    When bar 2 closes at 105.0 with ATR 5.0
    Then current HHV is still 110.0 from lookback
    But stop does NOT chase down to 95.0 (105 - 2*5)
    And stop remains at 100.0
    And snapshot HHV remains 110.0
    When bar 3 closes at 102.0 with ATR 5.0
    Then price hits stop at 100.0
    And position exits with profit of 2.0 per share

  Scenario: Snapshot reference only updates when stop tightens
    Given entry bar has close 100.0 and ATR 5.0
    When bar 1 closes at 115.0 with ATR 5.0
    Then stop tightens to 105.0
    And snapshot HHV updates to 115.0
    When bar 2 closes at 110.0 with ATR 5.0
    Then stop does not update
    And snapshot HHV stays at 115.0 (not 110.0)
    When bar 3 closes at 120.0 with ATR 5.0
    Then stop tightens to 110.0
    And snapshot HHV updates to 120.0
```

---

### Feature 3: Floor Tightening (Anti-Stickiness)

**File:** `trendlab-core/tests/bdd_position_management_floor.feature`

```gherkin
Feature: Floor-style tightening does not chase ceiling

  # Floor tightening: stop follows price UP but doesn't chase DOWN
  # This prevents the "ceiling chase" bug

  Background:
    Given a fixed percent stop manager with 5% stop
    And a long position at entry price 100.0
    And initial stop at 95.0

  Scenario: Floor only tightens, never chases ceiling
    # Traditional bug: as price rises, stop chases UP indefinitely
    # Anti-stickiness: stop stays at highest floor reached
    Given price rises to 110.0
    When trailing stop would update to 104.5 (110 * 0.95)
    Then stop tightens to 104.5
    And ratchet level becomes 104.5
    When price falls to 105.0
    Then stop does NOT loosen to 99.75 (105 * 0.95)
    And stop remains at 104.5
    When price continues to fall to 104.0
    Then position exits at stop 104.5
    And profit is 4.5 per share (not trapped chasing ceiling)
```

---

### Feature 4: Time Stop

**File:** `trendlab-core/tests/bdd_position_management_time.feature`

```gherkin
Feature: Time-based exits close positions after max bars

  Scenario: Position closes after max bars regardless of P&L
    Given a time stop manager with max bars 5
    And a long position opened at bar 0
    When bar 1 occurs
    Then no exit signal
    When bar 2 occurs
    Then no exit signal
    When bar 3 occurs
    Then no exit signal
    When bar 4 occurs
    Then no exit signal
    When bar 5 occurs
    Then time stop emits MOC exit order
    And position closes at bar 5 close price

  Scenario: Profitable position still exits on time stop
    Given a time stop manager with max bars 3
    And a long position opened at bar 0 with entry 100.0
    When bar 1 close is 110.0
    And bar 2 close is 115.0
    And bar 3 close is 120.0
    Then time stop triggers at bar 3
    And position exits with profit of 20.0 per share
```

---

## 4. Verification Commands

### Create Module Structure

```bash
# Create position management module
mkdir -p trendlab-core/src/position_management/strategies
mkdir -p trendlab-core/tests

# Create module files
touch trendlab-core/src/position_management/mod.rs
touch trendlab-core/src/position_management/intent.rs
touch trendlab-core/src/position_management/ratchet.rs
touch trendlab-core/src/position_management/manager.rs
touch trendlab-core/src/position_management/strategies/mod.rs
touch trendlab-core/src/position_management/strategies/fixed_percent.rs
touch trendlab-core/src/position_management/strategies/atr_stop.rs
touch trendlab-core/src/position_management/strategies/chandelier.rs
touch trendlab-core/src/position_management/strategies/time_stop.rs
touch trendlab-core/src/position_management/tests.rs

# Create BDD test files
touch trendlab-core/tests/bdd_position_management_ratchet.feature
touch trendlab-core/tests/bdd_position_management_chandelier.feature
touch trendlab-core/tests/bdd_position_management_floor.feature
touch trendlab-core/tests/bdd_position_management_time.feature
touch trendlab-core/tests/bdd_position_management_steps.rs
```

### Update `trendlab-core/src/position_management/mod.rs`

```rust
pub mod intent;
pub mod ratchet;
pub mod manager;
pub mod strategies;

pub use intent::{OrderIntent, CancelReason, ReplaceReason};
pub use ratchet::{RatchetState, RatchetError};
pub use manager::{PositionManager, PmRegistry, PmError};

pub use strategies::{
    FixedPercentStop,
    AtrStop,
    ChandelierExit,
    TimeStop,
};
```

### Update `trendlab-core/src/position_management/strategies/mod.rs`

```rust
mod fixed_percent;
mod atr_stop;
mod chandelier;
mod time_stop;

pub use fixed_percent::FixedPercentStop;
pub use atr_stop::AtrStop;
pub use chandelier::ChandelierExit;
pub use time_stop::TimeStop;
```

### Update `trendlab-core/src/lib.rs`

```rust
pub mod domain;
pub mod data;
pub mod event_loop;
pub mod orders;
pub mod execution;
pub mod position_management;  // NEW
pub mod portfolio;
```

---

### Run Tests

```bash
# Run all PM unit tests
cargo test -p trendlab-core position_management

# Run ratchet tests specifically
cargo test -p trendlab-core ratchet

# Run chandelier tests
cargo test -p trendlab-core chandelier

# Run BDD scenarios
cargo test --test bdd_position_management_ratchet
cargo test --test bdd_position_management_chandelier
cargo test --test bdd_position_management_floor
cargo test --test bdd_position_management_time

# Run all tests
cargo test --workspace
```

### Expected Test Output

```
running 25 tests
test position_management::ratchet::tests::test_long_ratchet_tightening ... ok
test position_management::ratchet::tests::test_long_ratchet_prevents_loosening ... ok
test position_management::ratchet::tests::test_short_ratchet_tightening ... ok
test position_management::ratchet::tests::test_short_ratchet_prevents_loosening ... ok
test position_management::ratchet::tests::test_disabled_ratchet_allows_any_level ... ok
test position_management::strategies::fixed_percent::tests::test_fixed_percent_stop_creation ... ok
test position_management::strategies::fixed_percent::tests::test_long_stop_calculation ... ok
test position_management::strategies::fixed_percent::tests::test_short_stop_calculation ... ok
test position_management::strategies::atr_stop::tests::test_atr_stop_prevents_loosening ... ok
test position_management::strategies::atr_stop::tests::test_atr_stop_without_ratchet_allows_loosening ... ok
test position_management::strategies::chandelier::tests::test_chandelier_anti_stickiness ... ok
test position_management::strategies::time_stop::tests::test_time_stop_triggers_after_max_bars ... ok

test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured
```

### BDD Test Output

```
Feature: Ratchet invariant prevents stop loosening
  ✓ Volatility expansion does not loosen stop (3 scenarios)
  ✓ Stop can tighten multiple times
  ✓ Disabled ratchet allows stop loosening

Feature: Chandelier exit prevents stickiness
  ✓ Chandelier allows profitable exit on rise-then-fall path
  ✓ Snapshot reference only updates when stop tightens

Feature: Floor tightening
  ✓ Floor only tightens, never chases ceiling

Feature: Time stop
  ✓ Position closes after max bars regardless of P&L
  ✓ Profitable position still exits on time stop

8 scenarios (8 passed)
24 steps (24 passed)
```

### Clippy Verification

```bash
cargo clippy -p trendlab-core --all-targets -- -D warnings
```

Expected: **0 warnings**

---

## 5. Completion Criteria Checklist

### Architecture & Separation (5 items)
- [ ] `OrderIntent` enum with New/Cancel/CancelReplace variants
- [ ] `PositionManager` trait implemented by all PM strategies
- [ ] `PmRegistry` for managing multiple concurrent PM strategies
- [ ] PM emits intents only, never direct fills
- [ ] Integration with OrderBook for cancel/replace execution

### Ratchet Invariant (4 items)
- [ ] `RatchetState` tracks tightest allowed stop level
- [ ] `try_update()` enforces ratchet (longs: stop moves up, shorts: down)
- [ ] Ratchet can be disabled per-position
- [ ] Ratchet prevents loosening even when ATR expands

### MVP PM Strategies (4 items)
- [ ] `FixedPercentStop`: fixed % from entry price
- [ ] `AtrStop`: ATR-based with ratchet
- [ ] `ChandelierExit`: HHV/LLV-based with snapshot anti-stickiness
- [ ] `TimeStop`: bar-count based exit

### Anti-Stickiness (4 items)
- [ ] Chandelier uses **snapshot HHV/LLV**, not current price
- [ ] Snapshot only updates when stop would tighten
- [ ] Chandelier allows profitable exit on rise-then-fall paths
- [ ] Floor tightening doesn't chase ceiling (ratchet prevents)

### Testing (3 items)
- [ ] 25+ unit tests covering all PM strategies and ratchet logic
- [ ] 8+ BDD scenarios for ratchet, anti-stickiness, and time stop
- [ ] Regression scenarios for volatility trap and stickiness bugs

**Total: 20 completion criteria**

---

## 6. Integration Notes

### With Event Loop (M3)
- Event loop calls `pm_registry.update_all()` at end of each bar (post-fill)
- PM intents → `OrderBook.submit()` for next bar

### With OrderBook (M4)
- `CancelReplace` intents must be atomic (handled in M4)
- Order book tracks order-to-position mapping

### With Execution (M5)
- PM stop orders go through normal fill simulation
- Gap logic applies to PM stops
- Slippage applies to PM exits

### With Portfolio (M3)
- PM reads position state (entry price, current quantity, unrealized P&L)
- PM never modifies positions directly

---

## 7. Future Enhancements (Post-MVP)

### Additional PM Strategies
- Profit target (fixed $ or %)
- Trailing stop (% from high water mark)
- Volatility-scaled sizing
- OCO brackets (stop + target)

### Advanced Ratcheting
- Time-decay ratchet (gradual tightening)
- Volatility regime-aware ratchet
- Partial position scaling with independent ratchets

### Multi-Position PM
- Portfolio-level stops (max drawdown)
- Correlation-aware PM
- Cross-position hedging

---

## Summary

M6 delivers:
- **8 module files** implementing intent-based position management
- **4 MVP PM strategies** (fixed %, ATR, chandelier, time)
- **Ratchet invariant** prevents volatility trap
- **Anti-stickiness guarantees** via snapshot reference levels
- **25+ unit tests** + **8+ BDD scenarios**
- **Regression protection** for known stickiness bugs

This completes the second core problem TrendLab v3 is designed to solve: **strategy stickiness**.

---

**Next Milestone:** M7 (Strategy Composition + Normalization)
