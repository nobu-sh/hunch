//! Octave-decision weights. Hand-tuned defaults; `hunch-eval fit-octave` overwrites this file.
//!
//! Format: three `[f32; N]` arrays and one `f32`, all literals. Entry `i` of each array belongs to
//! feature `OCTAVE_FEATURE_NAMES[i]`, one per line with the name in a trailing comment. A
//! candidate scores `BIAS + Σ WEIGHTS[i] · (feature[i] − FEATURE_MEANS[i]) / FEATURE_STDS[i]`, and
//! the softmax of the scores over a sound's candidates is the octave probability. Every std must be
//! positive.

pub const FEATURE_MEANS: [f32; 18] = [
    -0.1, // log2_tempo
    0.35, // log2_tempo_squared
    0.3,  // combined_beat
    0.3,  // combined_half_beat
    0.3,  // combined_double_beat
    0.2,  // low_beat
    0.2,  // low_two_beats
    0.2,  // low_four_beats
    0.2,  // mid_beat
    0.2,  // mid_two_beats
    0.2,  // mid_four_beats
    0.3,  // high_beat
    0.3,  // high_half_beat
    0.3,  // high_quarter_beat
    1.0,  // onset_density_deviation
    1.0,  // hat_density_deviation
    2.0,  // grid_salience
    0.3,  // grid_offbeat_contrast
];

pub const FEATURE_STDS: [f32; 18] = [
    0.6,  // log2_tempo
    0.35, // log2_tempo_squared
    0.25, // combined_beat
    0.25, // combined_half_beat
    0.25, // combined_double_beat
    0.25, // low_beat
    0.25, // low_two_beats
    0.25, // low_four_beats
    0.25, // mid_beat
    0.25, // mid_two_beats
    0.25, // mid_four_beats
    0.25, // high_beat
    0.25, // high_half_beat
    0.25, // high_quarter_beat
    1.0,  // onset_density_deviation
    1.0,  // hat_density_deviation
    1.0,  // grid_salience
    0.3,  // grid_offbeat_contrast
];

pub const WEIGHTS: [f32; 18] = [
    -0.45, // log2_tempo
    -1.05, // log2_tempo_squared
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
