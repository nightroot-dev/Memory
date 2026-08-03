use serde::{Deserialize, Serialize};

/// Edge type discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EdgeType {
    /// Temporal sequence (State→Action or any co-occurrence).
    Sequence = 0,
    /// Agent-driven causal link (Action→Outcome).
    CausalHypothesis = 1,
}

impl EdgeType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::CausalHypothesis,
            _ => Self::Sequence,
        }
    }
}

/// How this edge's evidence was collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EvidenceType {
    /// Inferred from temporal co-occurrence + cosine similarity.
    Inferred = 0,
    /// Provided explicitly by the host as a structured (state, action, outcome, success) tuple.
    Exact = 1,
}

impl EvidenceType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Exact,
            _ => Self::Inferred,
        }
    }
}

/// A directed causal edge between two nodes, modelled as a Beta-Bernoulli process.
///
/// Mean weight:  µ = α / (α + β)
/// Variance:     σ² = αβ / ((α+β)² · (α+β+1))
///
/// 16-byte packed layout (matches spec):
///   alpha: f32, beta: f32, last_traversal_time: f32, edge_type: u8, evidence_type: u8, is_intervention: u8, _pad: u8
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Beta-Bernoulli success pseudo-count.
    pub alpha: f32,
    /// Beta-Bernoulli failure pseudo-count.
    pub beta: f32,
    /// Unix seconds (f32 precision sufficient for relative deltas).
    pub last_traversal_time: f32,
    pub edge_type: EdgeType,
    pub evidence_type: EvidenceType,
    /// True when the source node was an agent-chosen Action (enables do-calculus).
    pub is_intervention: bool,
}

impl Edge {
    pub fn new(alpha: f32, beta: f32, now: f32, edge_type: EdgeType, evidence_type: EvidenceType, is_intervention: bool) -> Self {
        Self { alpha, beta, last_traversal_time: now, edge_type, evidence_type, is_intervention }
    }

    /// Mean weight of this edge — used for retrieval ranking and death criteria.
    #[inline]
    pub fn weight(&self) -> f32 {
        self.alpha / (self.alpha + self.beta)
    }

    /// Variance — used to flag "curious" high-uncertainty edges.
    #[inline]
    pub fn variance(&self) -> f32 {
        let n = self.alpha + self.beta;
        (self.alpha * self.beta) / (n * n * (n + 1.0))
    }

    /// Thompson sampling: draw a single sample from Beta(α, β).
    /// Uses Johnk's method implemented via gamma ratio.
    pub fn sample(&self, rng: &mut impl rand::Rng) -> f32 {
        let ga = rand_gamma(self.alpha, rng);
        let gb = rand_gamma(self.beta, rng);
        let s = ga / (ga + gb);
        s.clamp(1e-6, 1.0 - 1e-6)
    }

    /// Check edge death conditions.
    /// An edge is dead if:
    ///   - mean weight < 0.04, OR
    ///   - total evidence < 3 AND unused for > 7 days
    pub fn is_dead(&self, now_secs: f64) -> bool {
        let mean = self.weight();
        if mean < 0.04 {
            return true;
        }
        let total_evidence = self.alpha + self.beta;
        let days_unused = (now_secs - self.last_traversal_time as f64) / 86400.0;
        total_evidence < 3.0 && days_unused > 7.0
    }
}

/// Sample from Gamma(shape, 1) using Marsaglia-Tsang method.
fn rand_gamma(shape: f32, rng: &mut impl rand::Rng) -> f32 {
    if shape < 1.0 {
        // Use the relation: Gamma(shape) = Gamma(shape+1) * U^(1/shape)
        return rand_gamma(shape + 1.0, rng) * rng.gen::<f32>().powf(1.0 / shape);
    }
    let d = shape - 1.0 / 3.0;
    let c = (1.0 / (9.0 * d)).sqrt();
    loop {
        let x: f32 = rng.gen::<f32>() * 2.0 - 1.0; // simplified; use Box-Muller for production
        // Use a proper normal sample
        let u: f32 = rng.gen();
        let z = normal_sample(rng);
        let v = (1.0 + c * z).powi(3);
        if v > 0.0 {
            let dv = d * v;
            let x2 = z * z;
            if u < 1.0 - 0.0331 * x2 * x2 {
                return dv;
            }
            if u.ln() < 0.5 * x2 + d * (1.0 - v + v.ln()) {
                return dv;
            }
        }
        let _ = x; // suppress unused warning
    }
}

/// Box-Muller normal sample.
fn normal_sample(rng: &mut impl rand::Rng) -> f32 {
    use std::f32::consts::PI;
    let u1: f32 = rng.gen::<f32>().max(1e-10);
    let u2: f32 = rng.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}
