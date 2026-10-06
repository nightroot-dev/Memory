/// Welford online algorithm for running mean and variance of prediction error ε.
/// Used to compute self-calibrating salience as Φ(z-score).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalienceEngine {
    pub count: u64,
    pub mean: f64,
    pub m2: f64,
}

impl SalienceEngine {
    pub fn new() -> Self {
        Self { count: 0, mean: 0.0, m2: 0.0 }
    }

    /// Feed a new prediction error ε and optional emotional tag η.
    /// Returns salience ∈ (0.01, 0.99).
    pub fn update_and_calc(&mut self, epsilon: f64, eta: f64) -> f64 {
        // Warm-up phase: first 100 samples use heuristic
        if self.count < 100 {
            self.welford_update(epsilon);
            let s = (0.5 * epsilon + 0.5 * eta).clamp(0.01, 0.99);
            return s;
        }

        self.welford_update(epsilon);

        // H-05 fix: Bessel's correction — divide by (n-1) for unbiased sample variance.
        // ÷n (biased) underestimates variance → inflated z-scores → salience saturates
        // near 1.0 → consolidation death threshold never triggers → graph grows unbounded.
        let variance = if self.count > 1 { self.m2 / (self.count - 1) as f64 } else { 1.0 };

        let salience = if variance < 1e-12 {
            0.5
        } else {
            let z = (epsilon - self.mean) / variance.sqrt();
            phi(z)
        };

        let salience = salience.clamp(0.01, 0.99);

        // Boost extreme failures
        if epsilon > 0.9 {
            (salience * 1.5).min(1.0)
        } else {
            salience
        }
    }

    fn welford_update(&mut self, epsilon: f64) {
        self.count += 1;
        let delta = epsilon - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = epsilon - self.mean;
        self.m2 += delta * delta2;
    }
}

/// Φ(z) — cumulative standard normal using libm::erf.
fn phi(z: f64) -> f64 {
    0.5 * (1.0 + libm::erf(z / std::f64::consts::SQRT_2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warmup_phase() {
        let mut engine = SalienceEngine::new();
        // During warmup (< 100 samples), result is 0.5*ε + 0.5*η
        let s = engine.update_and_calc(0.8, 0.2);
        assert!((s - 0.5).abs() < 0.01, "warmup: expected ~0.5, got {}", s);
    }

    #[test]
    fn test_extreme_failure_boost() {
        let mut engine = SalienceEngine::new();
        // Feed 200 samples to get past warmup
        for _ in 0..200 {
            engine.update_and_calc(0.5, 0.5);
        }
        // High error should get boosted salience
        let s = engine.update_and_calc(0.95, 0.9);
        assert!(s > 0.5, "extreme failure should have high salience, got {}", s);
    }
}
