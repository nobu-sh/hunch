//! Streaming key and BPM analysis. Pure DSP: no IO, no browser dependencies.

mod analyzer;
mod change;
#[cfg(feature = "serde")]
pub mod contract;
pub mod dsp;
mod key;
pub mod rhythm;
pub mod segment;
mod session;
pub mod tonal;

pub use analyzer::{Analyzer, Estimate};
pub use change::{ChangeDetector, Observation, TempoChange};
pub use key::{Key, Mode, PitchClass};
pub use session::{Session, SessionEvent};
