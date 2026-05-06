//! Score calibration (Stage D).
//!
//! Detector scores are not calibrated probabilities — a 0.78 from the
//! TF-IDF tier and a 0.78 from the cross-encoder don't mean the same
//! thing. Calibration maps raw scores onto an empirical
//! `P(operator confirms | score)` derived from past services.
//!
//! We use **isotonic regression** because:
//!
//! 1. It's monotonic by construction: a higher raw score is never
//!    mapped to a lower probability. That preserves the partial order
//!    operators learn to trust.
//! 2. It's distribution-free: no Gaussian / Platt assumption.
//! 3. It works on tiny samples — 50–200 rows is enough to be useful;
//!    1k+ is excellent.
//!
//! Inputs come from `calibration_samples` (transcript / expected /
//! outcome). The fitter takes `(score, label)` pairs and produces a
//! piecewise-constant non-decreasing function. Lookup is O(log N).

use serde::{Deserialize, Serialize};

/// One calibration data point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationSample {
    pub score: f32,
    /// 1.0 = operator confirmed (true positive), 0.0 = operator
    /// rejected / corrected (false positive). Smoothing weights between
    /// 0 and 1 are accepted (e.g. partial credit for "corrected to a
    /// nearby verse").
    pub label: f32,
}

/// A calibrated mapping from raw score → confirmed-probability.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct IsotonicCalibration {
    /// Sorted by `x` ascending. `y` is non-decreasing.
    knots: Vec<(f32, f32)>,
}

impl IsotonicCalibration {
    /// Identity calibration — `lookup(s) == s`. Used when there are too
    /// few samples to fit responsibly.
    pub fn identity() -> Self {
        Self { knots: Vec::new() }
    }

    /// Fit an isotonic regression to the (score, label) samples using
    /// the pool-adjacent-violators algorithm (PAVA). Returns
    /// [`Self::identity`] if there are fewer than `min_samples` points
    /// (default: 25).
    pub fn fit(samples: &[CalibrationSample], min_samples: usize) -> Self {
        let min = min_samples.max(2);
        if samples.len() < min {
            return Self::identity();
        }
        // Sort by score ascending; ties broken by label.
        let mut sorted: Vec<CalibrationSample> = samples.to_vec();
        sorted.sort_by(|a, b| {
            a.score
                .total_cmp(&b.score)
                .then(a.label.total_cmp(&b.label))
        });

        // Pool-adjacent-violators with weights = 1.
        let mut blocks: Vec<(f32, f32, f32)> = sorted
            .iter()
            .map(|s| (s.score, s.label.clamp(0.0, 1.0), 1.0_f32))
            .collect();
        let mut i = 0;
        while i + 1 < blocks.len() {
            let (xi, yi, wi) = blocks[i];
            let (xj, yj, wj) = blocks[i + 1];
            if yi <= yj {
                i += 1;
                continue;
            }
            let merged_w = wi + wj;
            let merged_y = (yi * wi + yj * wj) / merged_w;
            // Use the higher x as the block's x, since lookup is on
            // upper boundary. (Using mean would mis-place the step.)
            let merged_x = xj.max(xi);
            blocks[i] = (merged_x, merged_y, merged_w);
            blocks.remove(i + 1);
            i = i.saturating_sub(1);
        }
        let knots: Vec<(f32, f32)> = blocks.into_iter().map(|(x, y, _)| (x, y)).collect();
        Self { knots }
    }

    /// Apply the calibration. Identity passthrough when not fit.
    pub fn lookup(&self, score: f32) -> f32 {
        if self.knots.is_empty() {
            return score.clamp(0.0, 1.0);
        }
        // Binary search for the upper boundary that covers `score`.
        let key = score;
        let pos = self.knots.partition_point(|(x, _)| *x < key);
        if pos >= self.knots.len() {
            return self.knots.last().map(|(_, y)| *y).unwrap_or(score);
        }
        // Linear interpolation between this knot and previous.
        let (x_hi, y_hi) = self.knots[pos];
        if pos == 0 {
            return y_hi.clamp(0.0, 1.0);
        }
        let (x_lo, y_lo) = self.knots[pos - 1];
        if x_hi <= x_lo {
            return y_hi.clamp(0.0, 1.0);
        }
        let t = ((key - x_lo) / (x_hi - x_lo)).clamp(0.0, 1.0);
        (y_lo + (y_hi - y_lo) * t).clamp(0.0, 1.0)
    }

    /// Returns the smallest raw score that the calibration maps to ≥
    /// `target_probability`. Used to derive operator-facing thresholds
    /// (e.g. "what raw score corresponds to 92% confirmed?"). Returns
    /// `None` if the calibration never reaches the target.
    pub fn raw_for_probability(&self, target_probability: f32) -> Option<f32> {
        if self.knots.is_empty() {
            return Some(target_probability.clamp(0.0, 1.0));
        }
        for (x, y) in &self.knots {
            if *y >= target_probability {
                return Some(*x);
            }
        }
        None
    }

    /// Number of monotone steps in the fit. Useful for diagnostics.
    pub fn knot_count(&self) -> usize {
        self.knots.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(score: f32, label: f32) -> CalibrationSample {
        CalibrationSample { score, label }
    }

    #[test]
    fn too_few_samples_returns_identity() {
        let cal = IsotonicCalibration::fit(&[sample(0.5, 1.0)], 25);
        assert_eq!(cal.knot_count(), 0);
        assert!((cal.lookup(0.7) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn perfectly_separable_calibration() {
        // Lows are wrong, highs are right — standard well-calibrated case.
        let mut samples = Vec::new();
        for i in 0..50 {
            samples.push(sample(0.10 + 0.005 * i as f32, 0.0));
        }
        for i in 0..50 {
            samples.push(sample(0.60 + 0.005 * i as f32, 1.0));
        }
        let cal = IsotonicCalibration::fit(&samples, 25);
        assert!(cal.lookup(0.20) < 0.20);
        assert!(cal.lookup(0.75) > 0.75 - 1e-3);
    }

    #[test]
    fn monotonicity_holds() {
        let samples: Vec<CalibrationSample> = (0..100)
            .map(|i| {
                let s = i as f32 / 100.0;
                let l = if i > 60 { 1.0 } else { 0.0 };
                sample(s, l)
            })
            .collect();
        let cal = IsotonicCalibration::fit(&samples, 25);
        let mut last = -1.0f32;
        for i in 0..=100 {
            let x = i as f32 / 100.0;
            let y = cal.lookup(x);
            assert!(
                y >= last - 1e-5,
                "isotonic not monotone: y({x})={y} < last={last}"
            );
            last = y;
        }
    }

    #[test]
    fn raw_for_probability_threshold() {
        let samples: Vec<CalibrationSample> = (0..100)
            .map(|i| {
                let s = i as f32 / 100.0;
                let l = if i >= 80 { 1.0 } else { 0.0 };
                sample(s, l)
            })
            .collect();
        let cal = IsotonicCalibration::fit(&samples, 25);
        // At ~80% confirmed, the threshold should be near 0.79.
        let raw = cal.raw_for_probability(0.99).expect("should reach");
        assert!(raw >= 0.78, "got {raw}");
    }

    #[test]
    fn lookup_is_clamped_to_unit() {
        let samples: Vec<CalibrationSample> = (0..30)
            .map(|i| sample(0.5 + (i as f32) * 0.01, 1.0))
            .collect();
        let cal = IsotonicCalibration::fit(&samples, 25);
        assert!((0.0..=1.0).contains(&cal.lookup(2.0)));
        assert!((0.0..=1.0).contains(&cal.lookup(-1.0)));
    }
}
