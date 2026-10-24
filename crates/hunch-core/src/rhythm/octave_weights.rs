//! Octave-decision weights, fitted by `hunch-eval fit-octave --rust-out`.
//!
//! Format: three `[f32; N]` arrays and one `f32`, all literals. Entry `i` of each array belongs to
//! feature `OCTAVE_FEATURE_NAMES[i]`, one per line with the name in a trailing comment. A
//! candidate scores `BIAS + Σ WEIGHTS[i] · (feature[i] − FEATURE_MEANS[i]) / FEATURE_STDS[i]`, and
//! the softmax of the scores over a sound's candidates is the octave probability. Every std must be
//! positive. `CONFIDENCE_WEIGHTS` (one per `CONFIDENCE_INPUT_NAMES`) and `CONFIDENCE_BIAS` map the
//! winner's evidence to the reported confidence through a logistic.

pub const FEATURE_MEANS: [f32; 20] = [
    -0.070212655, // log2_tempo
    0.4651726,    // log2_tempo_squared
    0.21717237,   // combined_beat
    0.14664164,   // combined_half_beat
    0.29948166,   // combined_double_beat
    0.16141635,   // low_beat
    0.2415061,    // low_two_beats
    0.31249833,   // low_four_beats
    0.24124591,   // mid_beat
    0.31783274,   // mid_two_beats
    0.3812136,    // mid_four_beats
    0.20613486,   // high_beat
    0.15048091,   // high_half_beat
    0.114584744,  // high_quarter_beat
    1.0359603,    // onset_density_deviation
    1.505273,     // hat_density_deviation
    3.112281,     // grid_salience
    0.33022428,   // grid_offbeat_contrast
    0.10358178,   // loop_beats_off_integer
    0.17004316,   // loop_beats_off_power_of_two
];

pub const FEATURE_STDS: [f32; 20] = [
    0.67841196, // log2_tempo
    0.41865858, // log2_tempo_squared
    0.28547147, // combined_beat
    0.25600472, // combined_half_beat
    0.3115115,  // combined_double_beat
    0.24151038, // low_beat
    0.2821075,  // low_two_beats
    0.32618466, // low_four_beats
    0.30920112, // mid_beat
    0.33092943, // mid_two_beats
    0.35900575, // mid_four_beats
    0.3035567,  // high_beat
    0.2755667,  // high_half_beat
    0.24729784, // high_quarter_beat
    1.3058195,  // onset_density_deviation
    3.2533867,  // hat_density_deviation
    2.3908403,  // grid_salience
    0.26123926, // grid_offbeat_contrast
    0.14501593, // loop_beats_off_integer
    0.1742697,  // loop_beats_off_power_of_two
];

pub const WEIGHTS: [f32; 20] = [
    0.062059566,  // log2_tempo
    -1.2600276,   // log2_tempo_squared
    0.14850521,   // combined_beat
    0.15123692,   // combined_half_beat
    -0.003446478, // combined_double_beat
    0.1233462,    // low_beat
    -0.016100848, // low_two_beats
    0.35447237,   // low_four_beats
    0.08973507,   // mid_beat
    0.18225895,   // mid_two_beats
    0.2902282,    // mid_four_beats
    0.08062056,   // high_beat
    0.28929004,   // high_half_beat
    0.26634538,   // high_quarter_beat
    -0.123945534, // onset_density_deviation
    -0.2043207,   // hat_density_deviation
    0.051201824,  // grid_salience
    -0.03749273,  // grid_offbeat_contrast
    -0.26009732,  // loop_beats_off_integer
    -0.31283483,  // loop_beats_off_power_of_two
];

pub const BIAS: f32 = 0.0;

pub const CONFIDENCE_WEIGHTS: [f32; 1] = [
    0.8645725, // probability_logit
];

pub const CONFIDENCE_BIAS: f32 = 0.35512567;
