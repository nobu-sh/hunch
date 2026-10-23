//! Streaming key and BPM analysis. Pure DSP: no IO, no browser dependencies.

mod analyzer;
mod key;

pub use analyzer::{Analyzer, Estimate};
pub use key::{Key, Mode, PitchClass};
