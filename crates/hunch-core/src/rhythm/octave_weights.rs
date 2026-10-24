//! Octave-decision weights. Hand-tuned defaults; `hunch-eval fit-octave` overwrites this file.
//!
//! Format: three `[f32; N]` arrays and one `f32`, all literals. Entry `i` of each array belongs to
//! feature `OCTAVE_FEATURE_NAMES[i]`, one per line with the name in a trailing comment. A
//! candidate scores `BIAS + Σ WEIGHTS[i] · (feature[i] − FEATURE_MEANS[i]) / FEATURE_STDS[i]`, and
//! the softmax of the scores over a sound's candidates is the octave probability. Every std must be
//! positive.

pub const FEATURE_MEANS: [f32; 18] = [
    -0.11, // log2_tempo
    0.44,  // log2_tempo_squared
    0.2,   // combined_beat
    0.12,  // combined_half_beat
    0.3,   // combined_double_beat
    0.15,  // low_beat
    0.23,  // low_two_beats
    0.29,  // low_four_beats
    0.23,  // mid_beat
    0.31,  // mid_two_beats
    0.38,  // mid_four_beats
    0.21,  // high_beat
    0.16,  // high_half_beat
    0.12,  // high_quarter_beat
    1.0,   // onset_density_deviation
    0.92,  // hat_density_deviation
    2.8,   // grid_salience
    0.34,  // grid_offbeat_contrast
];

pub const FEATURE_STDS: [f32; 18] = [
    0.66, // log2_tempo
    0.42, // log2_tempo_squared
    0.28, // combined_beat
    0.26, // combined_half_beat
    0.29, // combined_double_beat
    0.24, // low_beat
    0.27, // low_two_beats
    0.32, // low_four_beats
    0.29, // mid_beat
    0.31, // mid_two_beats
    0.35, // mid_four_beats
    0.32, // high_beat
    0.31, // high_half_beat
    0.29, // high_quarter_beat
    1.19, // onset_density_deviation
    1.29, // hat_density_deviation
    1.54, // grid_salience
    0.25, // grid_offbeat_contrast
];

pub const WEIGHTS: [f32; 18] = [
    -0.49, // log2_tempo
    -1.25, // log2_tempo_squared
    1.0,   // combined_beat
    0.2,   // combined_half_beat
    0.3,   // combined_double_beat
    0.0,   // low_beat
    0.3,   // low_two_beats
    0.0,   // low_four_beats
    -0.4,  // mid_beat
    0.6,   // mid_two_beats
    0.0,   // mid_four_beats
    0.2,   // high_beat
    0.2,   // high_half_beat
    0.0,   // high_quarter_beat
    -0.5,  // onset_density_deviation
    -1.0,  // hat_density_deviation
    0.3,   // grid_salience
    0.0,   // grid_offbeat_contrast
];

pub const BIAS: f32 = 0.0;
