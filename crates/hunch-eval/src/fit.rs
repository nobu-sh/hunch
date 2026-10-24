//! Conditional-logit fitter for the tempo octave decision.
//!
//! Each sound has a few octave candidates; the model scores each with `weights · standardized
//! features` and takes a softmax over that sound's candidates. Fitting maximizes the mean
//! log-probability of the correct candidates with L2 regularization. A bias would add the same
//! amount to every candidate of a sound, so it cancels in the softmax and is left out.

use serde::Serialize;

/// Standard deviations below this mean the feature is constant; it is left unscaled.
const MIN_STD: f64 = 1e-12;
const INITIAL_STEP: f64 = 1.0;
const MAX_STEP: f64 = 1e3;
/// Armijo sufficient-decrease constant for the backtracking line search.
const ARMIJO: f64 = 0.5;
const MAX_HALVINGS: usize = 60;
const NEWTON_ITERATIONS: usize = 50;
/// Stop Newton's method once a step is shorter than this.
const NEWTON_TOLERANCE: f64 = 1e-10;
const MIN_PIVOT: f64 = 1e-12;

/// One sound's octave candidates.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateSet {
    pub id: u64,
    /// One feature vector per candidate.
    pub features: Vec<Vec<f64>>,
    pub is_correct: Vec<bool>,
}

impl CandidateSet {
    pub fn has_correct(&self) -> bool {
        self.is_correct.iter().any(|&correct| correct)
    }
}

pub struct FitOptions {
    pub l2: f64,
    pub max_iterations: usize,
    /// Stop once the gradient norm falls below this.
    pub tolerance: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OctaveModel {
    pub mean: Vec<f64>,
    pub std: Vec<f64>,
    pub weights: Vec<f64>,
}

pub struct FitResult {
    pub model: OctaveModel,
    pub iterations: usize,
    pub loss: f64,
}

impl OctaveModel {
    pub fn score(&self, features: &[f64]) -> f64 {
        features
            .iter()
            .zip(&self.mean)
            .zip(&self.std)
            .zip(&self.weights)
            .map(|(((x, mean), std), weight)| (x - mean) / std * weight)
            .sum()
    }

    /// Softmax of the candidates' scores.
    pub fn probabilities(&self, set: &CandidateSet) -> Vec<f64> {
        let scores: Vec<f64> = set.features.iter().map(|x| self.score(x)).collect();
        softmax(&scores, |_| true).probabilities
    }

    /// Index of the highest-scoring candidate, the first on ties.
    pub fn pick(&self, set: &CandidateSet) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (index, features) in set.features.iter().enumerate() {
            let score = self.score(features);
            if best.is_none_or(|(_, best_score)| score > best_score) {
                best = Some((index, score));
            }
        }
        best.map(|(index, _)| index)
    }

    /// Fraction of `sets` whose top-scored candidate is correct.
    pub fn accuracy(&self, sets: &[CandidateSet]) -> Option<f64> {
        if sets.is_empty() {
            return None;
        }
        let correct = sets
            .iter()
            .filter(|set| self.pick(set).is_some_and(|index| set.is_correct[index]))
            .count();
        Some(correct as f64 / sets.len() as f64)
    }
}

/// Fits on `sets`, all of which must have `dimensions` features per candidate and a correct one.
pub fn fit(
    sets: &[CandidateSet],
    dimensions: usize,
    options: &FitOptions,
) -> Result<FitResult, String> {
    validate(sets, dimensions)?;
    let (mean, std) = standardization(sets, dimensions);
    let standardized: Vec<Vec<Vec<f64>>> = sets
        .iter()
        .map(|set| {
            set.features
                .iter()
                .map(|x| {
                    x.iter()
                        .zip(&mean)
                        .zip(&std)
                        .map(|((x, m), s)| (x - m) / s)
                        .collect()
                })
                .collect()
        })
        .collect();
    let objective = Objective {
        sets,
        standardized: &standardized,
        dimensions,
        l2: options.l2,
    };

    let mut weights = vec![0.0; dimensions];
    let (mut loss, mut gradient) = objective.evaluate(&weights);
    let mut step = INITIAL_STEP;
    let mut iterations = 0;
    while iterations < options.max_iterations && norm(&gradient) >= options.tolerance {
        iterations += 1;
        let Some((next, next_loss, next_gradient, taken)) =
            line_search(&objective, &weights, loss, &gradient, step)
        else {
            break;
        };
        weights = next;
        loss = next_loss;
        gradient = next_gradient;
        step = (taken * 2.0).min(MAX_STEP);
    }

    Ok(FitResult {
        model: OctaveModel { mean, std, weights },
        iterations,
        loss,
    })
}

fn validate(sets: &[CandidateSet], dimensions: usize) -> Result<(), String> {
    if sets.is_empty() {
        return Err("no sounds to fit".to_string());
    }
    for set in sets {
        if set.features.len() != set.is_correct.len() {
            return Err(format!(
                "sound {}: features and labels differ in length",
                set.id
            ));
        }
        if let Some(bad) = set.features.iter().find(|x| x.len() != dimensions) {
            return Err(format!(
                "sound {}: candidate has {} features, expected {dimensions}",
                set.id,
                bad.len()
            ));
        }
        if !set.has_correct() {
            return Err(format!("sound {} has no correct candidate", set.id));
        }
    }
    Ok(())
}

/// Per-feature mean and standard deviation over every training candidate.
fn standardization(sets: &[CandidateSet], dimensions: usize) -> (Vec<f64>, Vec<f64>) {
    let rows: Vec<&Vec<f64>> = sets.iter().flat_map(|set| &set.features).collect();
    let count = rows.len().max(1) as f64;
    let mean: Vec<f64> = (0..dimensions)
        .map(|d| rows.iter().map(|x| x[d]).sum::<f64>() / count)
        .collect();
    let std = (0..dimensions)
        .map(|d| {
            let variance = rows.iter().map(|x| (x[d] - mean[d]).powi(2)).sum::<f64>() / count;
            let std = variance.sqrt();
            if std < MIN_STD { 1.0 } else { std }
        })
        .collect();
    (mean, std)
}

struct Objective<'a> {
    sets: &'a [CandidateSet],
    standardized: &'a [Vec<Vec<f64>>],
    dimensions: usize,
    l2: f64,
}

impl Objective<'_> {
    /// Mean negative log-likelihood plus `l2 / 2 · |w|²`, and its gradient.
    fn evaluate(&self, weights: &[f64]) -> (f64, Vec<f64>) {
        let mut loss = 0.0;
        let mut gradient = vec![0.0; self.dimensions];
        for (set, candidates) in self.sets.iter().zip(self.standardized) {
            let scores: Vec<f64> = candidates.iter().map(|z| dot(z, weights)).collect();
            let all = softmax(&scores, |_| true);
            let correct = softmax(&scores, |index| set.is_correct[index]);
            loss += all.log_normalizer - correct.log_normalizer;
            for (index, z) in candidates.iter().enumerate() {
                let pull = all.probabilities[index] - correct.probabilities[index];
                for (g, x) in gradient.iter_mut().zip(z) {
                    *g += pull * x;
                }
            }
        }
        let count = self.sets.len() as f64;
        loss = loss / count + 0.5 * self.l2 * dot(weights, weights);
        for (g, w) in gradient.iter_mut().zip(weights) {
            *g = *g / count + self.l2 * w;
        }
        (loss, gradient)
    }
}

struct Softmax {
    probabilities: Vec<f64>,
    log_normalizer: f64,
}

/// Softmax over the candidates `include` selects; excluded candidates get probability 0.
fn softmax(scores: &[f64], include: impl Fn(usize) -> bool) -> Softmax {
    let max = (0..scores.len())
        .filter(|&i| include(i))
        .map(|i| scores[i])
        .fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = (0..scores.len())
        .map(|i| {
            if include(i) {
                (scores[i] - max).exp()
            } else {
                0.0
            }
        })
        .collect();
    let total: f64 = exps.iter().sum();
    Softmax {
        probabilities: exps.iter().map(|e| e / total).collect(),
        log_normalizer: max + total.ln(),
    }
}

type Step = (Vec<f64>, f64, Vec<f64>, f64);

/// Backtracking gradient step. `None` when no step decreases the loss enough.
fn line_search(
    objective: &Objective,
    weights: &[f64],
    loss: f64,
    gradient: &[f64],
    initial_step: f64,
) -> Option<Step> {
    let gradient_norm_squared = dot(gradient, gradient);
    let mut step = initial_step;
    for _ in 0..MAX_HALVINGS {
        let candidate: Vec<f64> = weights
            .iter()
            .zip(gradient)
            .map(|(w, g)| w - step * g)
            .collect();
        let (candidate_loss, candidate_gradient) = objective.evaluate(&candidate);
        if candidate_loss <= loss - ARMIJO * step * gradient_norm_squared {
            return Some((candidate, candidate_loss, candidate_gradient, step));
        }
        step /= 2.0;
    }
    None
}

/// One sound's confidence inputs and whether its estimate was right.
pub struct CalibrationSample {
    pub inputs: Vec<f64>,
    pub is_correct: bool,
    /// Share of the loss relative to the other samples.
    pub weight: f64,
}

/// Logistic map from confidence inputs to the probability the estimate is right.
#[derive(Debug, Clone, Serialize)]
pub struct Calibration {
    pub weights: Vec<f64>,
    pub bias: f64,
}

impl Calibration {
    pub fn probability(&self, inputs: &[f64]) -> f64 {
        sigmoid(self.bias + dot(&self.weights, inputs))
    }
}

/// Fits `Calibration` by Newton's method on the weighted mean log-loss plus `l2 / 2 · |weights|²`.
pub fn fit_calibration(samples: &[CalibrationSample], l2: f64) -> Result<Calibration, String> {
    let Some(first) = samples.first() else {
        return Err("no sounds to calibrate on".to_string());
    };
    let dimensions = first.inputs.len() + 1;
    if let Some(bad) = samples.iter().find(|s| s.inputs.len() + 1 != dimensions) {
        return Err(format!(
            "calibration inputs differ in length: {} and {}",
            dimensions - 1,
            bad.inputs.len()
        ));
    }
    // Parameter 0 is the bias, which is not regularized.
    let with_bias = |sample: &CalibrationSample| -> Vec<f64> {
        std::iter::once(1.0)
            .chain(sample.inputs.iter().copied())
            .collect()
    };
    let rows: Vec<Vec<f64>> = samples.iter().map(with_bias).collect();
    let total_weight: f64 = samples.iter().map(|sample| sample.weight).sum();
    if total_weight <= 0.0 {
        return Err("calibration samples have no weight".to_string());
    }
    let mut parameters = vec![0.0; dimensions];
    for _ in 0..NEWTON_ITERATIONS {
        let mut gradient = vec![0.0; dimensions];
        let mut hessian = vec![vec![0.0; dimensions]; dimensions];
        for (row, sample) in rows.iter().zip(samples) {
            let probability = sigmoid(dot(row, &parameters));
            let share = sample.weight / total_weight;
            let residual = probability - f64::from(u8::from(sample.is_correct));
            let curvature = probability * (1.0 - probability);
            for i in 0..dimensions {
                gradient[i] += share * residual * row[i];
                for j in 0..dimensions {
                    hessian[i][j] += share * curvature * row[i] * row[j];
                }
            }
        }
        for i in 1..dimensions {
            gradient[i] += l2 * parameters[i];
            hessian[i][i] += l2;
        }
        let step = solve(hessian, gradient)
            .ok_or("calibration inputs are degenerate: the Newton system is singular")?;
        for (parameter, delta) in parameters.iter_mut().zip(&step) {
            *parameter -= delta;
        }
        if norm(&step) < NEWTON_TOLERANCE {
            break;
        }
    }
    Ok(Calibration {
        bias: parameters[0],
        weights: parameters[1..].to_vec(),
    })
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Solves `matrix · x = rhs` by Gaussian elimination with partial pivoting; `None` if singular.
fn solve(mut matrix: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Option<Vec<f64>> {
    let size = rhs.len();
    for column in 0..size {
        let pivot = (column..size)
            .max_by(|&a, &b| matrix[a][column].abs().total_cmp(&matrix[b][column].abs()))?;
        if matrix[pivot][column].abs() < MIN_PIVOT {
            return None;
        }
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        for row in column + 1..size {
            let factor = matrix[row][column] / matrix[column][column];
            for k in column..size {
                matrix[row][k] -= factor * matrix[column][k];
            }
            rhs[row] -= factor * rhs[column];
        }
    }
    let mut solution = vec![0.0; size];
    for row in (0..size).rev() {
        let known: f64 = (row + 1..size).map(|k| matrix[row][k] * solution[k]).sum();
        solution[row] = (rhs[row] - known) / matrix[row][row];
    }
    Some(solution)
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(v: &[f64]) -> f64 {
    dot(v, v).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANDIDATES_PER_SOUND: usize = 4;

    /// xorshift64*: deterministic test data without a rand dependency.
    struct Rng(u64);

    impl Rng {
        fn uniform(&mut self) -> f64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            let bits = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11;
            bits as f64 / (1u64 << 53) as f64
        }
    }

    fn options() -> FitOptions {
        FitOptions {
            l2: 1e-3,
            max_iterations: 2_000,
            tolerance: 1e-7,
        }
    }

    /// Sounds whose correct candidate maximizes `true_weights · x`. Feature 0 sits far from zero
    /// with a large spread to exercise standardization; the last feature is pure noise.
    fn synthetic(rng: &mut Rng, sounds: usize, true_weights: &[f64]) -> Vec<CandidateSet> {
        (0..sounds as u64)
            .map(|id| {
                let features: Vec<Vec<f64>> = (0..CANDIDATES_PER_SOUND)
                    .map(|_| {
                        let mut x: Vec<f64> = true_weights
                            .iter()
                            .map(|_| rng.uniform() * 2.0 - 1.0)
                            .collect();
                        x[0] = 100.0 + 50.0 * x[0];
                        x
                    })
                    .collect();
                let true_score = |x: &Vec<f64>| {
                    let centered = [(x[0] - 100.0) / 50.0, x[1], x[2]];
                    dot(&centered, true_weights)
                };
                let best = (0..CANDIDATES_PER_SOUND)
                    .max_by(|&a, &b| true_score(&features[a]).total_cmp(&true_score(&features[b])))
                    .expect("candidates are non-empty");
                let is_correct = (0..CANDIDATES_PER_SOUND).map(|i| i == best).collect();
                CandidateSet {
                    id,
                    features,
                    is_correct,
                }
            })
            .collect()
    }

    #[test]
    fn recovers_known_decision_rule() {
        let true_weights = [2.0, -1.0, 0.0];
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let train = synthetic(&mut rng, 600, &true_weights);
        let val = synthetic(&mut rng, 300, &true_weights);

        let result = fit(&train, true_weights.len(), &options()).expect("fit");
        let model = &result.model;

        let train_acc = model.accuracy(&train).expect("train accuracy");
        let val_acc = model.accuracy(&val).expect("val accuracy");
        assert!(train_acc > 0.95, "train accuracy {train_acc}");
        assert!(val_acc > 0.9, "val accuracy {val_acc}");

        let expected_std = 50.0 / 3f64.sqrt();
        assert!((model.mean[0] - 100.0).abs() < 5.0, "mean {:?}", model.mean);
        assert!(
            (model.std[0] - expected_std).abs() < 3.0,
            "std {:?}",
            model.std
        );

        let weights = &model.weights;
        assert!(weights[0] > 0.0 && weights[1] < 0.0, "weights {weights:?}");
        let ratio = weights[0] / -weights[1];
        assert!(
            (ratio - 2.0).abs() < 0.5,
            "weight ratio {ratio}, weights {weights:?}"
        );
        assert!(
            weights[2].abs() < 0.2 * weights[0],
            "noise weight {weights:?}"
        );
    }

    #[test]
    fn stronger_l2_shrinks_weights() {
        let true_weights = [2.0, -1.0, 0.0];
        let mut rng = Rng(42);
        let train = synthetic(&mut rng, 300, &true_weights);
        let weak = fit(&train, 3, &options()).expect("fit");
        let strong_options = FitOptions {
            l2: 1.0,
            ..options()
        };
        let strong = fit(&train, 3, &strong_options).expect("fit");
        assert!(norm(&strong.model.weights) < norm(&weak.model.weights));
        assert!(strong.loss.is_finite());
    }

    #[test]
    fn constant_features_stay_finite() {
        let sets = vec![CandidateSet {
            id: 1,
            features: vec![vec![5.0], vec![5.0]],
            is_correct: vec![false, true],
        }];
        let result = fit(&sets, 1, &options()).expect("fit");
        assert_eq!(result.model.std, vec![1.0]);
        assert!(result.model.weights.iter().all(|w| w.is_finite()));
    }

    #[test]
    fn rejects_sounds_without_a_correct_candidate() {
        let sets = vec![CandidateSet {
            id: 7,
            features: vec![vec![1.0], vec![2.0]],
            is_correct: vec![false, false],
        }];
        let error = fit(&sets, 1, &options()).err().expect("error");
        assert!(error.contains("sound 7"), "{error}");
    }

    #[test]
    fn calibration_recovers_logistic_rule() {
        let mut rng = Rng(7);
        let samples: Vec<CalibrationSample> = (0..4_000)
            .map(|_| {
                let x = rng.uniform() * 6.0 - 3.0;
                let probability = sigmoid(1.5 * x - 0.5);
                CalibrationSample {
                    inputs: vec![x],
                    is_correct: rng.uniform() < probability,
                    weight: 1.0,
                }
            })
            .collect();
        let calibration = fit_calibration(&samples, 0.0).expect("fit");
        assert!(
            (calibration.weights[0] - 1.5).abs() < 0.2,
            "{calibration:?}"
        );
        assert!((calibration.bias + 0.5).abs() < 0.2, "{calibration:?}");
    }

    #[test]
    fn calibration_rejects_constant_inputs_without_l2() {
        let samples: Vec<CalibrationSample> = [true, false]
            .map(|is_correct| CalibrationSample {
                inputs: vec![1.0],
                is_correct,
                weight: 1.0,
            })
            .into();
        assert!(fit_calibration(&samples, 0.0).is_err());
        assert!(fit_calibration(&samples, 1e-2).is_ok());
    }

    #[test]
    fn rejects_feature_length_mismatch() {
        let sets = vec![CandidateSet {
            id: 3,
            features: vec![vec![1.0, 2.0]],
            is_correct: vec![true],
        }];
        let error = fit(&sets, 1, &options()).err().expect("error");
        assert!(error.contains("expected 1"), "{error}");
    }
}
