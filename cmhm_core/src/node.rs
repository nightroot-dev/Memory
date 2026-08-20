use serde::{Deserialize, Serialize};

/// Node type discriminant.
/// 0 = State, 1 = Action, 2 = Outcome, 3 = Skill (compiled)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum NodeType {
    State = 0,
    Action = 1,
    Outcome = 2,
    Skill = 3,
}

impl NodeType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::State,
            1 => Self::Action,
            2 => Self::Outcome,
            3 => Self::Skill,
            _ => Self::State,
        }
    }

    /// Parse a lower-case string retrieval intent into a `NodeType`.
    ///
    /// Used by MCP tool handlers to convert the required `retrieval_intent`
    /// field (`"state"` | `"action"` | `"outcome"`) into a typed enum value.
    /// Returns `None` for any unrecognised string so callers can return
    /// MCP error `-32602` (`invalid_params`) without panicking.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "state"   => Some(Self::State),
            "action"  => Some(Self::Action),
            "outcome" => Some(Self::Outcome),
            _         => None,
        }
    }
}

/// A single causal memory node.
///
/// ## Memory layout (approximate, post-Stage-1B)
/// embedding Vec<f32>: 24 bytes (ptr+len+cap) + dim*4 on heap
///   - hash/128-dim:  24 + 512   =  536 bytes
///   - MiniLM/384-dim: 24 + 1536 = 1560 bytes
/// model_id String:  ~24 bytes (ptr+len+cap) + len on heap
/// Other fields:     ~40 bytes
/// → ~600 bytes (128-dim) or ~1624 bytes (384-dim) per node
/// → ~60 MB for 100k nodes (128-dim), ~163 MB (384-dim) — both acceptable
///
/// ## New fields (Stage 3)
/// - `n_retrievals` / `last_retrieved`: enable spacing-effect half-life update.
/// - `half_life_days`: controls power-law decay rate; updated on outcome feedback.
/// - `max_prediction_error`: δ-immunity — nodes with high surprise are never culled.
/// - `bandit_alpha` / `bandit_beta`: per-node Beta posterior for Thompson-sampled scoring.
///
/// ## New field (Stage 1B — N3 compliance)
/// - `model_id`: stamped at write time. Nodes from different embedders must NEVER be
///   compared via cosine similarity — mixed-geometry produces silently wrong scores.
///   Default "hash-128d" for backward compatibility with pre-Stage-1B snapshots.
///
/// ## Backward compatibility
/// All new fields use `#[serde(default)]` so existing snapshots deserialize with
/// safe initial values (0 retrievals, 180-day half-life, uniform bandit prior, "hash-128d").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// L2-normalised embedding vector. Length = embedder.dim().
    ///
    /// Vec<f32> allows both 128-dim (hash) and 384-dim (MiniLM) without code changes.
    /// Serde serialises this natively as a JSON array / bincode sequence.
    ///
    /// **Do not compare nodes from different model_ids via cosine similarity (N3).**
    pub embedding: Vec<f32>,

    /// ID of the embedder that produced this node's embedding vector (N3).
    ///
    /// Values: "hash-128d" (hash default) | "minilm-l6-v2-int8" (ONNX MiniLM)
    /// MUST match the active Embedder::model_id() before any similarity comparison.
    #[serde(default = "default_model_id")]
    pub model_id: String,

    /// Importance ∈ (0, 1). Updated by Salience Engine.
    pub salience: f32,
    /// Creation time, Unix seconds.
    pub timestamp: i64,
    /// Emotional tag / task criticality ∈ [0, 1].
    pub emotional_tag: f32,
    /// Node category.
    pub node_type: NodeType,

    // ── Stage 3: Retrieval tracking ──────────────────────────────────────────

    /// Number of times this node has been returned in a retrieval result.
    /// Used to determine if a node is "useful in practice" (n_retrievals > 0 protects from cull).
    #[serde(default)]
    pub n_retrievals: u32,

    /// Unix timestamp of the last retrieval. Used for spacing-effect half-life updates.
    /// 0 = never retrieved.
    #[serde(default)]
    pub last_retrieved: i64,

    // ── Stage 3: Forgetting ───────────────────────────────────────────────────

    /// Half-life for power-law salience decay, in days.
    ///
    /// Updated by `memory_feedback`:
    /// - success → `half_life *= 2.0` (capped at 3650 days = 10 years)
    /// - failure → `half_life *= 0.7` (floor at 1 day)
    ///
    /// Default: 180 days (≈ 6 months, conservative starting point).
    #[serde(default = "default_half_life")]
    pub half_life_days: f32,

    // ── Stage 3: δ-immunity ───────────────────────────────────────────────────

    /// Maximum `prediction_error` ever observed for a learn that touched this node.
    ///
    /// Nodes with `max_prediction_error > 0.8` are **δ-immune**: they bypass ALL cull
    /// conditions. A high-surprise memory may be the most important one in the store.
    ///
    /// Updated on every `learn` call that upserts this node.
    #[serde(default)]
    pub max_prediction_error: f32,

    // ── Stage 3: Usefulness bandit ────────────────────────────────────────────

    /// Beta posterior α for Thompson-sampled retrieval scoring.
    ///
    /// Init: 1.0 (uniform prior). Updated by `memory_feedback`:
    /// - success after retrieval → `alpha += 1`
    ///
    /// Beta(α, β) models P(this node is useful | retrieved).
    #[serde(default = "default_bandit_prior")]
    pub bandit_alpha: f32,

    /// Beta posterior β for Thompson-sampled retrieval scoring.
    ///
    /// Init: 1.0 (uniform prior). Updated by `memory_feedback`:
    /// - failure after retrieval → `beta += 1`
    #[serde(default = "default_bandit_prior")]
    pub bandit_beta: f32,

    // ── Phase 2: Provenance, authority, supersession ──────────────────────────

    /// Explicit source-trust level ∈ [0, 1]. Distinct from `emotional_tag` (affective salience).
    ///
    /// 0.0 = explicitly unreliable / contradicted.
    /// 0.5 = neutral / unknown (default for all pre-v3 nodes recovered from WAL).
    /// 1.0 = explicitly authoritative.
    ///
    /// Set at learn time by the caller via `Command::Learn { authority, .. }`.
    /// Used in retrieval seed scoring: `score += authority_weight × node.authority`.
    #[serde(default = "default_authority")]
    pub authority: f32,

    /// Caller-supplied machine-readable source class.
    ///
    /// Examples: `"user_explicit"`, `"corrected"`, `"external_feed"`, `""` (unknown).
    /// Used for conflict detection: two candidates with non-empty differing provenance
    /// and near-equal scores trigger `RetrievalResult::conflict_detected = true`.
    /// Empty string = unknown (default for all pre-v3 nodes recovered from WAL).
    #[serde(default)]
    pub provenance: String,

    /// ID of the node that supersedes this node, if any.
    ///
    /// When `Some(id)`, this node has been explicitly superseded by the node with that ID.
    /// **Superseded nodes are excluded from seed candidate selection entirely** — no score
    /// comparison is performed. This handles staleness without a recency term in the score.
    ///
    /// Set by `process_learn` when `Command::Learn { supersedes: Some(old_id), .. }` is
    /// received: the engine sets `old_node.superseded_by = Some(new_id)` after creating
    /// the new node.
    ///
    /// `None` = active, not superseded (default for all pre-v3 nodes).
    #[serde(default)]
    pub superseded_by: Option<u64>,

    /// Machine-readable assertion / entity key (e.g. `"db_pool_size"`, `"compiler_target"`).
    /// Used for deterministic logical contradiction detection when two nodes share the same key.
    #[serde(default)]
    pub assertion_key: Option<String>,

    /// Epistemic polarity: `+1` (asserted/affirmed), `-1` (refuted/negated), or `None` (neutral).
    /// Opposite polarities on the same `assertion_key` trigger a `LogicalContradiction` conflict.
    #[serde(default)]
    pub polarity: Option<i8>,
}

// Serde default helpers (functions, not closures — required by serde's default attribute)
fn default_half_life() -> f32 { 180.0 }
fn default_bandit_prior() -> f32 { 1.0 }
fn default_model_id() -> String { "hash-128d".to_string() }
/// Default authority: 0.5 = neutral / unknown. Applies to all pre-v3 nodes recovered from WAL.
fn default_authority() -> f32 { 0.5 }

impl Node {
    pub fn new(
        embedding: Vec<f32>,
        model_id: String,
        salience: f32,
        timestamp: i64,
        emotional_tag: f32,
        node_type: NodeType,
        authority: f32,
        provenance: String,
    ) -> Self {
        Self {
            embedding,
            model_id,
            salience,
            timestamp,
            emotional_tag,
            node_type,
            // Stage 3 defaults
            n_retrievals:         0,
            last_retrieved:       0,
            half_life_days:       180.0,
            max_prediction_error: 0.0,
            bandit_alpha:         1.0,
            bandit_beta:          1.0,
            // Phase 2 provenance/supersession
            authority,
            provenance,
            superseded_by:        None,
            assertion_key:        None,
            polarity:             None,
        }
    }

    pub fn with_assertion(mut self, assertion_key: Option<String>, polarity: Option<i8>) -> Self {
        self.assertion_key = assertion_key;
        self.polarity = polarity;
        self
    }


    /// Effective salience after power-law temporal decay.
    ///
    /// Formula: `S_eff = salience · (1 + Δt/τ)^(-0.35)`
    /// where `τ = half_life_days × 86400` (converted to seconds).
    ///
    /// Properties:
    /// - At `Δt = 0`: `S_eff = salience` (no decay yet)
    /// - At `Δt = half_life_days`: `S_eff ≈ 0.784 · salience`  (not half — power law, not exponential)
    /// - At `Δt → ∞`: `S_eff → 0` (monotone decay)
    ///
    /// This is a power law (Ebbinghaus-Wozniak model), not exponential decay,
    /// because human memory and agent context forgetting follow power laws empirically.
    pub fn effective_salience(&self, now_secs: i64) -> f32 {
        let tau_secs = self.half_life_days * 86400.0;
        let delta_t  = (now_secs - self.timestamp).max(0) as f32;
        let decay    = (1.0 + delta_t / tau_secs).powf(-0.35);
        self.salience * decay
    }

    /// True if this node should never be culled, regardless of age or salience.
    ///
    /// δ-immune condition: the node was ever associated with a high-surprise event.
    /// The intuition: if an outcome was completely unexpected (prediction_error > 0.8),
    /// forgetting that memory is exactly the wrong thing to do.
    pub fn is_delta_immune(&self) -> bool {
        self.max_prediction_error > 0.8
    }

    /// Thompson sample from the node's Beta(α, β) usefulness posterior.
    ///
    /// Returns a draw θ̃ ∈ (0, 1) representing the estimated probability
    /// that retrieving this node leads to a successful outcome.
    ///
    /// Uses Johnk's method for Beta sampling — pure Rust, no libm calls.
    /// Caller multiplies this into the seed-selection score.
    pub fn thompson_draw(&self, rng: &mut impl rand::Rng) -> f32 {
        // Johnk's method: θ = X / (X + Y) where X ~ Gamma(α), Y ~ Gamma(β)
        // For integer/small α,β: use sum of exponentials (exact, fast)
        // General case: use the log-exponential trick
        let alpha = self.bandit_alpha.max(0.01);
        let beta  = self.bandit_beta.max(0.01);

        // Fast path for uniform prior (most new nodes)
        if (alpha - 1.0).abs() < 1e-4 && (beta - 1.0).abs() < 1e-4 {
            return rng.gen::<f32>();
        }

        // General Beta(α, β) via ratio of Gamma samples
        // Gamma(k) ≈ -ln(product of k uniforms) for integer k
        // For non-integer, use Marsaglia-Tsang (approximate via log method)
        let x = gamma_sample(alpha, rng);
        let y = gamma_sample(beta,  rng);
        let total = x + y;
        if total < 1e-10 { 0.5 } else { (x / total).clamp(0.0, 1.0) }
    }

    /// Update this node after a retrieval outcome.
    ///
    /// Called by `memory_feedback` handler.
    /// - `success = true`:  alpha += 1, half_life doubled (spaced repetition reward)
    /// - `success = false`: beta  += 1, half_life shortened (this memory misled the agent)
    pub fn update_from_outcome(&mut self, success: bool) {
        if success {
            self.bandit_alpha  += 1.0;
            self.half_life_days = (self.half_life_days * 2.0).min(3650.0); // cap at 10 years
        } else {
            self.bandit_beta   += 1.0;
            self.half_life_days = (self.half_life_days * 0.7).max(1.0);    // floor at 1 day
        }
    }

    /// Record that this node was retrieved at `now_secs`.
    pub fn record_retrieval(&mut self, now_secs: i64) {
        // N24: Clock skew guard. If last_retrieved > now_secs, a backward step occurred
        // (NTP, VM migration). We silently skip the update to avoid salience increase.
        // The debug_assert surfaces this in test/dev builds so the issue is visible.
        debug_assert!(
            self.last_retrieved <= now_secs,
            "clock went backward: last_retrieved={} > now_secs={}",
            self.last_retrieved, now_secs,
        );
        if self.last_retrieved > now_secs {
            return;
        }
        self.n_retrievals  = self.n_retrievals.saturating_add(1);
        self.last_retrieved = now_secs;
    }

    /// Update δ-immunity tracking with the prediction error of the current learn.
    pub fn update_max_prediction_error(&mut self, pe: f32) {
        if pe > self.max_prediction_error {
            self.max_prediction_error = pe;
        }
    }

    /// Legacy salience accessor (used by consolidation for culling decisions).
    pub fn salience(&self) -> f32 {
        self.salience
    }
}

// ── Gamma sampling helper ─────────────────────────────────────────────────────

/// Approximate Gamma(shape) sample using Marsaglia-Tsang method.
///
/// For shape >= 1: uses the squeeze method (fast, single-pass usually).
/// For shape < 1:  uses the shape+1 trick: Gamma(k) = Gamma(k+1) × U^(1/k).
///
/// This is the standard algorithm — see Marsaglia & Tsang (2000) ACM TOMS.
fn gamma_sample(shape: f32, rng: &mut impl rand::Rng) -> f32 {
    let shape = shape as f64;

    if shape < 1.0 {
        // Reduction: Gamma(k) = Gamma(k+1) × U^(1/k)
        let u: f64 = rng.gen::<f64>();
        return (gamma_sample_ge1(shape + 1.0, rng) * u.powf(1.0 / shape)) as f32;
    }
    gamma_sample_ge1(shape, rng) as f32
}

/// Gamma(shape) for shape >= 1 via Marsaglia-Tsang squeeze.
fn gamma_sample_ge1(shape: f64, rng: &mut impl rand::Rng) -> f64 {
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();

    loop {
        // Generate normal(0,1) via Box-Muller
        let u1: f64 = rng.gen::<f64>();
        let u2: f64 = rng.gen::<f64>();
        let x = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();

        let v = 1.0 + c * x;
        if v <= 0.0 { continue; }
        let v3 = v * v * v;
        let u: f64 = rng.gen::<f64>();

        // Squeeze test (avoids expensive log most of the time)
        if u < 1.0 - 0.0331 * x * x * x * x {
            return d * v3;
        }
        // Log test (exact)
        if u.ln() < 0.5 * x * x + d * (1.0 - v3 + v3.ln()) {
            return d * v3;
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    fn test_node() -> Node {
        Node::new(vec![0.0f32; 128], "hash-128d".to_string(), 0.8, 1_000_000, 0.5, NodeType::State, 0.5, String::new())
    }

    // --- effective_salience ---------------------------------------------------

    #[test]
    fn salience_no_decay_at_t0() {
        let n = test_node();
        let s = n.effective_salience(1_000_000);
        assert!((s - 0.8).abs() < 1e-4, "at t=0, effective_salience must equal salience");
    }

    #[test]
    fn salience_decays_monotonically() {
        let n = test_node();
        let s0 = n.effective_salience(1_000_000);
        let s1 = n.effective_salience(1_000_000 + 86400);       // +1 day
        let s2 = n.effective_salience(1_000_000 + 86400 * 30);  // +30 days
        let s3 = n.effective_salience(1_000_000 + 86400 * 365); // +1 year
        assert!(s0 > s1, "salience must decrease after 1 day");
        assert!(s1 > s2, "salience must decrease after 30 days");
        assert!(s2 > s3, "salience must decrease after 1 year");
    }

    #[test]
    fn salience_decay_floor_not_negative() {
        let n = test_node();
        let s = n.effective_salience(i64::MAX);
        assert!(s >= 0.0, "effective_salience must never go negative");
    }

    // --- delta immunity -------------------------------------------------------

    #[test]
    fn delta_immune_above_threshold() {
        let mut n = test_node();
        assert!(!n.is_delta_immune(), "new node must not be immune");
        n.update_max_prediction_error(0.85);
        assert!(n.is_delta_immune(), "node with pe=0.85 must be delta-immune");
    }

    #[test]
    fn delta_immune_exactly_at_threshold_is_not_immune() {
        let mut n = test_node();
        n.update_max_prediction_error(0.8);
        assert!(!n.is_delta_immune(), "pe=0.8 is not strictly > 0.8, so not immune");
    }

    #[test]
    fn max_prediction_error_tracks_highest() {
        let mut n = test_node();
        n.update_max_prediction_error(0.3);
        n.update_max_prediction_error(0.9);
        n.update_max_prediction_error(0.5);
        assert!((n.max_prediction_error - 0.9).abs() < 1e-6, "must track the maximum");
    }

    // --- outcome feedback -----------------------------------------------------

    #[test]
    fn success_extends_half_life() {
        let mut n = test_node();
        let before = n.half_life_days;
        n.update_from_outcome(true);
        assert!(n.half_life_days > before, "success must extend half_life");
        assert!((n.half_life_days - 360.0).abs() < 1e-4, "success must double half_life from 180 to 360");
    }

    #[test]
    fn failure_shortens_half_life() {
        let mut n = test_node();
        let before = n.half_life_days;
        n.update_from_outcome(false);
        assert!(n.half_life_days < before, "failure must shorten half_life");
        assert!((n.half_life_days - 126.0).abs() < 1e-4, "failure must multiply half_life by 0.7");
    }

    #[test]
    fn half_life_capped_at_10_years() {
        let mut n = test_node();
        n.half_life_days = 3000.0;
        n.update_from_outcome(true);
        assert!(n.half_life_days <= 3650.0, "half_life must be capped at 3650 days");
    }

    #[test]
    fn half_life_floored_at_1_day() {
        let mut n = test_node();
        n.half_life_days = 1.2;
        n.update_from_outcome(false);
        assert!(n.half_life_days >= 1.0, "half_life must be floored at 1 day");
    }

    // --- retrieval tracking ---------------------------------------------------

    #[test]
    fn record_retrieval_increments() {
        let mut n = test_node();
        assert_eq!(n.n_retrievals, 0);
        n.record_retrieval(2_000_000);
        assert_eq!(n.n_retrievals, 1);
        assert_eq!(n.last_retrieved, 2_000_000);
        n.record_retrieval(3_000_000);
        assert_eq!(n.n_retrievals, 2);
        assert_eq!(n.last_retrieved, 3_000_000);
    }

    // --- Thompson sampling ----------------------------------------------------

    #[test]
    fn thompson_draw_in_unit_interval() {
        let mut rng = SmallRng::seed_from_u64(42);
        let n = test_node();
        for _ in 0..100 {
            let draw = n.thompson_draw(&mut rng);
            assert!(draw >= 0.0 && draw <= 1.0, "draw must be in [0,1], got {draw}");
        }
    }

    #[test]
    fn thompson_draw_biased_toward_success() {
        let mut rng = SmallRng::seed_from_u64(7);
        let mut n = test_node();
        // Strong success signal: alpha=20, beta=1
        for _ in 0..19 { n.update_from_outcome(true); }
        let mean: f32 = (0..200).map(|_| n.thompson_draw(&mut rng)).sum::<f32>() / 200.0;
        assert!(mean > 0.7, "after 19 successes, mean draw should be > 0.7, got {mean:.3}");
    }

    #[test]
    fn thompson_draw_biased_toward_failure() {
        let mut rng = SmallRng::seed_from_u64(13);
        let mut n = test_node();
        // Strong failure signal: alpha=1, beta=10
        for _ in 0..9 { n.update_from_outcome(false); }
        let mean: f32 = (0..200).map(|_| n.thompson_draw(&mut rng)).sum::<f32>() / 200.0;
        assert!(mean < 0.35, "after 9 failures, mean draw should be < 0.35, got {mean:.3}");
    }
}

