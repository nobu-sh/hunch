//! Streaming key and BPM analysis. Pure DSP: no IO, no browser dependencies.

mod analyzer;
pub mod dsp;
mod key;
pub mod rhythm;
pub mod tonal;

pub use analyzer::{Analyzer, Estimate};
pub use key::{Key, Mode, PitchClass};
