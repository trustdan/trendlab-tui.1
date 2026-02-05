/// Position management: anti-stickiness + ratchet invariant
///
/// **Key Design Principles:**
/// 1. PM strategies emit **intents**, never direct fills
/// 2. **Ratchet invariant**: stops may tighten, never loosen (even if ATR expands)
/// 3. **Anti-stickiness**: reference levels prevent chasing highs/lows
/// 4. Clean separation from execution engine
///
/// **Module Structure:**
/// - `intent`: Order intents (cancel/replace/new)
/// - `ratchet`: Ratchet state enforcement
/// - `manager`: PositionManager trait + registry
/// - `strategies`: Concrete PM implementations (fixed %, ATR, chandelier, time)
pub mod intent;
pub mod manager;
pub mod ratchet;
pub mod strategies;

pub use intent::{CancelReplaceIntent, OrderIntent};
pub use manager::{PmRegistry, PositionManager, Side};
pub use ratchet::RatchetState;
pub use strategies::{AtrStop, ChandelierExit, FixedPercentStop, TimeStop};
