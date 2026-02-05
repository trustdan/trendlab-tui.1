/// Position management strategies
///
/// This module contains concrete PM strategy implementations.
pub mod atr_stop;
pub mod chandelier;
pub mod fixed_percent;
pub mod time_stop;

pub use atr_stop::AtrStop;
pub use chandelier::ChandelierExit;
pub use fixed_percent::FixedPercentStop;
pub use time_stop::TimeStop;
