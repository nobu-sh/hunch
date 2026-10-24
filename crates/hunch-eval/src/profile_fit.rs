//! Fits a 24-key profile (12 major + 12 minor weights, shared across tonics) to labeled chroma.
//!
//! The model is the detector's own scoring rule: each key scores
//! `scale · (corr(chroma, template rotated to the tonic) + tonic_weight · bass cue at the tonic)`, and a
//! softmax over the 24 keys gives the probability of each. Fitting maximizes the weighted mean
//! log-probability of the labeled keys (any listed alternative counts) with an L2 pull of each
//! template toward a hand profile. Templates stay zero-mean and unit-length, the form correlation
//! sees, so the result drops into `KeyProfile` unchanged, with `tonic_weight` as the detector's.

use hunch_core::{Key, Mode, PitchClass};

const PITCH_CLASSES: usize = 12;
const MODES: [Mode; 2] = [Mode::Major, Mode::Minor];
const KEY_COUNT: usize = PITCH_CLASSES * MODES.len();
/// Semitones from a major tonic up to its relative minor's tonic.
const RELATIVE_MINOR_OFFSET: usize = 9;
const INITIAL_SCALE: f64 = 10.0;
const INITIAL_STEP: f64 = 0.1;
const MAX_STEP: f64 = 10.0;
const MAX_HALVINGS: usize = 40;
const MAX_ITERATIONS: usize = 3_000;
/// Stop once a step improves the loss by less than this.
const LOSS_TOLERANCE: f64 = 1e-10;
const MIN_NORM: f64 = 1e-12;

type Template = [f64; PITCH_CLASSES];

/// One labeled sound's scoring inputs.
pub struct ProfileSample {
    /// Zero-mean, unit-length chroma.
    pub chroma: Template,
    /// 0..=1 bass evidence per pitch class.
    pub cue: Template,
    /// Indices (`mode · 12 + tonic`) of the keys that count as correct.
    pub labels: Vec<usize>,
    /// `labels` plus their relative keys: every key with the right notes.
    pub same_scale: Vec<usize>,
    pub weight: f64,
}

impl ProfileSample {
    /// `None` when the chroma is flat or there are no labels.
    pub fn new(
        chroma: &[f32; PITCH_CLASSES],
        cue: &[f32; PITCH_CLASSES],
        labels: &[Key],
    ) -> Option<Self> {
        let chroma = centered_unit(&chroma.map(f64::from))?;
        let mut same_scale: Vec<usize> = labels
            .iter()
            .flat_map(|&key| [key_index(key), key_index(relative_key(key))])
            .collect();
        same_scale.sort_unstable();
        same_scale.dedup();
        let labels: Vec<usize> = labels.iter().map(|&key| key_index(key)).collect();
        (!labels.is_empty()).then_some(Self {
            chroma,
            cue: cue.map(f64::from),
            labels,
            same_scale,
            weight: 1.0,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProfileModel {
    /// Major then minor, zero-mean and unit-length, index 0 = tonic.
    pub templates: [Template; 2],
    /// Softmax sharpness of the correlation.
    pub scale: f64,
    pub tonic_weight: f64,
    /// Added to every minor key's correlation.
    pub minor_bias: f64,
}

impl ProfileModel {
    /// The hand profile `templates` with the detector's `tonic_weight`, at the initial scale.
    pub fn from_hand(templates: [&[f32; PITCH_CLASSES]; 2], tonic_weight: f64) -> Option<Self> {
        let major = centered_unit(&templates[0].map(f64::from))?;
        let minor = centered_unit(&templates[1].map(f64::from))?;
        Some(Self {
            templates: [major, minor],
            scale: INITIAL_SCALE,
            tonic_weight,
            minor_bias: 0.0,
        })
    }

    fn scores(&self, sample: &ProfileSample) -> [f64; KEY_COUNT] {
        std::array::from_fn(|index| {
            let (tonic, mode) = (index % PITCH_CLASSES, index / PITCH_CLASSES);
            self.scale
                * (rotated_dot(&sample.chroma, &self.templates[mode], tonic)
                    + self.tonic_weight * sample.cue[tonic]
                    + self.minor_bias * mode as f64)
        })
    }

    /// The best-scoring key, as the detector would pick it.
    pub fn predict(&self, sample: &ProfileSample) -> Key {
        let scores = self.scores(sample);
        let best = (0..KEY_COUNT)
            .max_by(|&a, &b| scores[a].total_cmp(&scores[b]))
            .unwrap_or(0);
        key_at(best)
    }

    /// Each template shifted and scaled into 0..=1, which correlation ignores, for printing.
    pub fn display_templates(&self) -> [Template; 2] {
        self.templates.map(|template| {
            let low = template.iter().copied().fold(f64::INFINITY, f64::min);
            let high = template.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let range = (high - low).max(MIN_NORM);
            template.map(|value| (value - low) / range)
        })
    }
}

pub struct FitReport {
    pub model: ProfileModel,
    pub loss: f64,
    pub iterations: usize,
}

pub struct FitSettings {
    /// Pull of each template toward the prior.
    pub l2: f64,
    /// Share of the likelihood on the exact key; the rest is on the scale (exact or relative).
    pub exact_share: f64,
    /// Fit the tonic weight too, rather than keep the prior's.
    pub learns_tonic_weight: bool,
    /// Fit a bias toward minor keys too.
    pub learns_minor_bias: bool,
}

/// Fits starting from `prior`.
pub fn fit(samples: &[ProfileSample], prior: &ProfileModel, settings: &FitSettings) -> FitReport {
    let objective = Objective {
        samples,
        prior: &prior.templates,
        l2: settings.l2,
        exact_share: settings.exact_share,
        learns_tonic_weight: settings.learns_tonic_weight,
        learns_minor_bias: settings.learns_minor_bias,
    };
    let mut model = *prior;
    let (mut loss, mut gradient) = objective.evaluate(&model);
    let mut step = INITIAL_STEP;
    let mut iterations = 0;
    while iterations < MAX_ITERATIONS {
        iterations += 1;
        let Some((next, next_loss, taken)) = line_search(&objective, &model, &gradient, loss, step)
        else {
            break;
        };
        let improvement = loss - next_loss;
        model = next;
        loss = next_loss;
        gradient = objective.evaluate(&model).1;
        step = (taken * 2.0).min(MAX_STEP);
        if improvement < LOSS_TOLERANCE {
            break;
        }
    }
    FitReport {
        model,
        loss,
        iterations,
    }
}

/// Gradient of the loss: per template, then log-scale, then tonic weight.
struct Gradient {
    templates: [Template; 2],
    log_scale: f64,
    tonic_weight: f64,
    minor_bias: f64,
}

struct Objective<'a> {
    samples: &'a [ProfileSample],
    prior: &'a [Template; 2],
    l2: f64,
    exact_share: f64,
    learns_tonic_weight: bool,
    learns_minor_bias: bool,
}

impl Objective<'_> {
    /// Weighted mean negative log-likelihood (exact and same-scale mixed by `exact_share`) plus `l2 · Σ |template − prior|²`, and its gradient
    /// projected onto the zero-mean unit sphere each template lives on.
    fn evaluate(&self, model: &ProfileModel) -> (f64, Gradient) {
        let mut loss = 0.0;
        let mut gradient = Gradient {
            templates: [[0.0; PITCH_CLASSES]; 2],
            log_scale: 0.0,
            tonic_weight: 0.0,
            minor_bias: 0.0,
        };
        let total_weight: f64 = self
            .samples
            .iter()
            .map(|s| s.weight)
            .sum::<f64>()
            .max(MIN_NORM);
        for sample in self.samples {
            let scores = model.scores(sample);
            let all = softmax(&scores, |_| true);
            let exact = softmax(&scores, |index| sample.labels.contains(&index));
            let scale = softmax(&scores, |index| sample.same_scale.contains(&index));
            let weight = sample.weight / total_weight;
            let (exact_share, scale_share) = (self.exact_share, 1.0 - self.exact_share);
            loss += weight
                * (all.log_normalizer
                    - exact_share * exact.log_normalizer
                    - scale_share * scale.log_normalizer);
            for index in 0..KEY_COUNT {
                let target = exact_share * exact.probabilities[index]
                    + scale_share * scale.probabilities[index];
                let pull = weight * (all.probabilities[index] - target);
                if pull == 0.0 {
                    continue;
                }
                let (tonic, mode) = (index % PITCH_CLASSES, index / PITCH_CLASSES);
                let correlation = rotated_dot(&sample.chroma, &model.templates[mode], tonic);
                for (offset, g) in gradient.templates[mode].iter_mut().enumerate() {
                    *g += pull * model.scale * sample.chroma[(tonic + offset) % PITCH_CLASSES];
                }
                let cue = sample.cue[tonic];
                let bias = model.minor_bias * mode as f64;
                gradient.log_scale +=
                    pull * model.scale * (correlation + model.tonic_weight * cue + bias);
                if self.learns_tonic_weight {
                    gradient.tonic_weight += pull * model.scale * cue;
                }
                if self.learns_minor_bias {
                    gradient.minor_bias += pull * model.scale * mode as f64;
                }
            }
        }
        for mode in 0..MODES.len() {
            for offset in 0..PITCH_CLASSES {
                let gap = model.templates[mode][offset] - self.prior[mode][offset];
                loss += self.l2 * gap * gap;
                gradient.templates[mode][offset] += 2.0 * self.l2 * gap;
            }
            gradient.templates[mode] = tangent(&gradient.templates[mode], &model.templates[mode]);
        }
        (loss, gradient)
    }
}

/// Backtracking step along the negative gradient, retracted back onto the sphere.
fn line_search(
    objective: &Objective,
    model: &ProfileModel,
    gradient: &Gradient,
    loss: f64,
    initial_step: f64,
) -> Option<(ProfileModel, f64, f64)> {
    let mut step = initial_step;
    for _ in 0..MAX_HALVINGS {
        let candidate = take_step(model, gradient, step)?;
        let candidate_loss = objective.evaluate(&candidate).0;
        if candidate_loss < loss {
            return Some((candidate, candidate_loss, step));
        }
        step /= 2.0;
    }
    None
}

fn take_step(model: &ProfileModel, gradient: &Gradient, step: f64) -> Option<ProfileModel> {
    let moved = |mode: usize| -> Option<Template> {
        let template: Template = std::array::from_fn(|offset| {
            model.templates[mode][offset] - step * gradient.templates[mode][offset]
        });
        centered_unit(&template)
    };
    Some(ProfileModel {
        templates: [moved(0)?, moved(1)?],
        scale: model.scale * (-step * gradient.log_scale).exp(),
        tonic_weight: model.tonic_weight - step * gradient.tonic_weight,
        minor_bias: model.minor_bias - step * gradient.minor_bias,
    })
}

/// `gradient` with its components off the zero-mean subspace and along `point` removed.
fn tangent(gradient: &Template, point: &Template) -> Template {
    let mean = gradient.iter().sum::<f64>() / PITCH_CLASSES as f64;
    let centered = gradient.map(|value| value - mean);
    let along: f64 = centered.iter().zip(point).map(|(g, p)| g * p).sum();
    std::array::from_fn(|offset| centered[offset] - along * point[offset])
}

struct Softmax {
    probabilities: [f64; KEY_COUNT],
    log_normalizer: f64,
}

/// Softmax over the keys `include` selects; the others get probability 0.
fn softmax(scores: &[f64; KEY_COUNT], include: impl Fn(usize) -> bool) -> Softmax {
    let max = (0..KEY_COUNT)
        .filter(|&index| include(index))
        .map(|index| scores[index])
        .fold(f64::NEG_INFINITY, f64::max);
    let exps: [f64; KEY_COUNT] = std::array::from_fn(|index| {
        if include(index) {
            (scores[index] - max).exp()
        } else {
            0.0
        }
    });
    let total: f64 = exps.iter().sum();
    Softmax {
        probabilities: exps.map(|value| value / total),
        log_normalizer: max + total.ln(),
    }
}

fn rotated_dot(chroma: &Template, template: &Template, tonic: usize) -> f64 {
    (0..PITCH_CLASSES)
        .map(|offset| chroma[(tonic + offset) % PITCH_CLASSES] * template[offset])
        .sum()
}

fn centered_unit(values: &Template) -> Option<Template> {
    let mean = values.iter().sum::<f64>() / PITCH_CLASSES as f64;
    let centered = values.map(|value| value - mean);
    let norm = centered
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    (norm.is_finite() && norm > MIN_NORM).then(|| centered.map(|value| value / norm))
}

fn relative_key(key: Key) -> Key {
    let (offset, mode) = match key.mode {
        Mode::Major => (RELATIVE_MINOR_OFFSET, Mode::Minor),
        Mode::Minor => (PITCH_CLASSES - RELATIVE_MINOR_OFFSET, Mode::Major),
    };
    let tonic = (usize::from(key.tonic.index()) + offset) % PITCH_CLASSES;
    key_at(usize::from(mode == Mode::Minor) * PITCH_CLASSES + tonic)
}

fn key_index(key: Key) -> usize {
    let mode = usize::from(key.mode == Mode::Minor);
    mode * PITCH_CLASSES + usize::from(key.tonic.index())
}

fn key_at(index: usize) -> Key {
    let tonic = PitchClass::new((index % PITCH_CLASSES) as u8)
        .expect("index reduced modulo 12 is a valid pitch class");
    Key {
        tonic,
        mode: MODES[index / PITCH_CLASSES % MODES.len()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAJOR_SCALE: Template = [1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0];
    const FLAT: [f32; PITCH_CLASSES] = [0.5; PITCH_CLASSES];
    const MINOR_TRIAD_AND_FOURTH: Template =
        [1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];

    fn key(tonic: u8, mode: Mode) -> Key {
        Key {
            tonic: PitchClass::new(tonic).expect("valid pitch class"),
            mode,
        }
    }

    fn rotated(template: &Template, tonic: usize) -> [f32; PITCH_CLASSES] {
        std::array::from_fn(|pitch| {
            template[(pitch + PITCH_CLASSES - tonic) % PITCH_CLASSES] as f32
        })
    }

    /// Chroma of keys whose true templates differ from the prior: minor keys here stress the
    /// fourth (offset 5), which the prior doesn't.
    fn samples() -> Vec<ProfileSample> {
        let minor = MINOR_TRIAD_AND_FOURTH;
        let mut samples = Vec::new();
        for tonic in 0..PITCH_CLASSES {
            let major_chroma = rotated(&MAJOR_SCALE, tonic);
            let minor_chroma = rotated(&minor, tonic);
            let tonic = tonic as u8;
            samples.extend(ProfileSample::new(
                &major_chroma,
                &FLAT,
                &[key(tonic, Mode::Major)],
            ));
            samples.extend(ProfileSample::new(
                &minor_chroma,
                &FLAT,
                &[key(tonic, Mode::Minor)],
            ));
        }
        samples
    }

    fn settings(l2: f64) -> FitSettings {
        FitSettings {
            l2,
            exact_share: 1.0,
            learns_tonic_weight: true,
            learns_minor_bias: true,
        }
    }

    #[test]
    fn same_scale_includes_relative_keys() {
        let sample = ProfileSample::new(&rotated(&MAJOR_SCALE, 0), &FLAT, &[key(0, Mode::Major)])
            .expect("not flat");
        assert_eq!(sample.same_scale, vec![0, PITCH_CLASSES + 9]);
    }

    fn prior() -> ProfileModel {
        let minor_prior: [f32; PITCH_CLASSES] =
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0];
        let major_prior = MAJOR_SCALE.map(|value| value as f32);
        ProfileModel::from_hand([&major_prior, &minor_prior], 0.0).expect("not flat")
    }

    #[test]
    fn learns_templates_that_classify_the_training_keys() {
        let samples = samples();
        let report = fit(&samples, &prior(), &settings(1e-3));
        let initial_loss = Objective {
            samples: &samples,
            prior: &prior().templates,
            l2: 1e-3,
            exact_share: 1.0,
            learns_tonic_weight: true,
            learns_minor_bias: true,
        }
        .evaluate(&prior())
        .0;
        assert!(
            report.loss < initial_loss,
            "{} vs {initial_loss}",
            report.loss
        );
        for sample in &samples {
            assert_eq!(key_index(report.model.predict(sample)), sample.labels[0]);
        }
        let true_minor = centered_unit(&MINOR_TRIAD_AND_FOURTH).expect("not flat");
        let fitted_match = rotated_dot(&true_minor, &report.model.templates[1], 0);
        let prior_match = rotated_dot(&true_minor, &prior().templates[1], 0);
        assert!(
            fitted_match > prior_match,
            "minor template moved away from the data: {fitted_match} vs {prior_match}"
        );
    }

    #[test]
    fn strong_l2_keeps_the_prior() {
        let report = fit(&samples(), &prior(), &settings(1e3));
        for (fitted, prior) in report.model.templates.iter().zip(prior().templates) {
            for (a, b) in fitted.iter().zip(prior) {
                assert!((a - b).abs() < 0.02, "{fitted:?}");
            }
        }
    }

    #[test]
    fn gradient_matches_finite_differences() {
        let samples = samples();
        let model = prior();
        let objective = Objective {
            samples: &samples,
            prior: &model.templates,
            l2: 0.1,
            exact_share: 0.5,
            learns_tonic_weight: true,
            learns_minor_bias: true,
        };
        let gradient = objective.evaluate(&model).1;
        let epsilon = 1e-6;
        let shifted = ProfileModel {
            tonic_weight: model.tonic_weight + epsilon,
            ..model
        };
        let numeric = (objective.evaluate(&shifted).0 - objective.evaluate(&model).0) / epsilon;
        assert!(
            (numeric - gradient.tonic_weight).abs() < 1e-4,
            "{numeric} vs {}",
            gradient.tonic_weight
        );
        let scaled = ProfileModel {
            scale: model.scale * epsilon.exp(),
            ..model
        };
        let numeric = (objective.evaluate(&scaled).0 - objective.evaluate(&model).0) / epsilon;
        assert!(
            (numeric - gradient.log_scale).abs() < 1e-4,
            "{numeric} vs {}",
            gradient.log_scale
        );
    }

    #[test]
    fn display_templates_span_zero_to_one() {
        let shown = prior().display_templates();
        for template in shown {
            assert!(template.iter().all(|value| (0.0..=1.0).contains(value)));
            assert!((template[0] - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn rejects_flat_chroma() {
        assert!(ProfileSample::new(&FLAT, &FLAT, &[key(0, Mode::Major)]).is_none());
    }
}
