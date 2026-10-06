use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// CMHM runtime configuration.
/// All fields configurable via config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Working memory buffer capacity (default: 5).
    pub working_memory_size: usize,
    /// Edge birth: minimum similarity score to create an inferred edge (default: 0.35).
    pub edge_birth_threshold: f32,
    /// Node birth: maximum cosine to nearest neighbor for novelty (default: 0.55).
    pub novelty_threshold: f32,
    /// Node birth: minimum salience to force creation even if not novel (default: 0.70).
    pub salience_threshold: f32,
    /// Edge forgetting rate λ_c in hr⁻¹ (default: 0.1, half-life ~7h).
    pub forgetting_lambda_c: f64,
    /// Edge birth temporal decay λ_t in s⁻¹ (default: 0.0001).
    pub temporal_decay_lambda_t: f64,
    /// Number of cohort centroids for the superstition filter (default: 512).
    pub cohort_count: usize,
    /// Dense core relation slices (default: 8).
    pub dense_core_slices: usize,
    /// Consolidation: max hours between consolidation passes (default: 24).
    pub consolidation_interval_hours: f64,
    /// Consolidation: node count trigger (default: 8000).
    pub consolidation_node_trigger: usize,
    /// Consolidation: cumulative reinforcement trigger (default: 50.0).
    pub consolidation_reinforce_trigger: f32,
    /// Data directory for WAL and snapshots.
    pub data_dir: PathBuf,
    /// Path to projection matrices directory.
    pub projections_dir: PathBuf,
    /// Max hops in Dijkstra retrieval (default: 3).
    pub retrieval_max_depth: usize,
    // ── Context Runtime ────────────────────────────────────────────────────
    /// Maximum number of contexts to keep loaded in RAM simultaneously (default: 10).
    pub max_active_contexts: usize,
    /// Seconds of inactivity before Active → Idle transition (default: 300 = 5 min).
    pub idle_timeout_secs: u64,
    /// Seconds in Idle before Idle → Sleeping transition (default: 1800 = 30 min).
    pub sleep_timeout_secs: u64,
    /// Snapshot every N learn commands per context (default: 1000).
    pub snapshot_interval_cmds: u64,
    // ── Context Router Weights ─────────────────────────────────────────────
    /// Weight for semantic (keyword/tag) overlap score Sq. Must sum with others to ~1.0.
    pub router_alpha: f32,
    /// Weight for access-frequency score Sh.
    pub router_beta: f32,
    /// Weight for temporal recency score St.
    pub router_gamma: f32,
    /// Weight for active-context bonus Sa.
    pub router_delta: f32,
    /// Half-life in seconds for the recency decay St = exp(-ln(2)/half_life * age).
    /// Default 86400 = 1 day: a context used 1 day ago scores St=0.5.
    pub router_recency_half_life_secs: f32,
    // ── Retrieval Blend Weights ────────────────────────────────────────────
    /// Weight of direct query cosine similarity in seed-node scoring (default: 0.7).
    /// seed_score = retrieval_query_weight * cos(q, node)
    ///            + retrieval_context_weight * cos(dense_ctx, node)
    /// Must sum to 1.0 with retrieval_context_weight.
    #[serde(default = "default_retrieval_query_weight")]
    pub retrieval_query_weight: f32,
    /// Weight of dense context cosine similarity in seed-node scoring (default: 0.3).
    #[serde(default = "default_retrieval_context_weight")]
    pub retrieval_context_weight: f32,
    /// Variance threshold above which an edge is flagged as uncertain/curious (default: 0.1).
    /// Lower → more edges flagged; higher → only highly uncertain edges flagged.
    #[serde(default = "default_retrieval_curiosity_threshold")]
    pub retrieval_curiosity_threshold: f32,

    /// Weight of `node.authority` (machine-readable source-trust level) in seed scoring.
    ///
    /// `seed_score += retrieval_authority_weight × node.authority`
    ///
    /// `node.authority` ∈ [0, 1]. Values closer to 1.0 indicate explicitly authoritative sources;
    /// values closer to 0.0 indicate explicitly contradicted or unreliable sources.
    /// Unlike `emotional_tag` (affective salience), `authority` is machine-readable and
    /// set by the caller at learn time. `emotional_tag` does NOT appear in the score formula.
    ///
    /// Production default: **0.8**. With `authority_weight = 0.8`, a node with
    /// `authority = 1.0` outscores a node with `authority = 0.0` and `cos ≈ 0.994`
    /// even when the authoritative node has `cos ≈ 0.110` (worst-case contradiction test).
    /// Formula: `0.7 * 0.110 + 0.8 * 1.0 = 0.877 > 0.7 * 0.994 + 0.8 * 0.0 = 0.696`.
    #[serde(default = "default_retrieval_authority_weight")]
    pub retrieval_authority_weight: f32,

    /// **DEPRECATED in schema v3** — value is read but not passed to `retrieval::retrieve`.
    ///
    /// Was: weight of the exponential recency term in seed scoring.
    /// Replacement: declare `supersedes: Option<u64>` at learn time to exclude stale nodes
    /// from seed candidate selection entirely.
    /// Default: 0.0 (no-op).
    #[serde(default = "default_retrieval_recency_weight")]
    pub retrieval_recency_weight: f32,

    /// **DEPRECATED in schema v3** — value is read but not passed to `retrieval::retrieve`.
    ///
    /// Was: decay rate (s⁻¹) for the exponential recency term.
    /// Replacement: see `retrieval_recency_weight` — supersession handles staleness.
    /// Default: 0.01 (no-op).
    #[serde(default = "default_retrieval_recency_lambda")]
    pub retrieval_recency_lambda: f32,

    /// Deprecated-in-place as of Protocol v1.9: score-gap proximity is deleted from
    /// the conflict decision path. Retained for schema compatibility. Default: 0.0.
    #[serde(default = "default_conflict_detection_threshold")]
    pub conflict_detection_threshold: f32,

    /// When `true` and `conflict_detected` (with `should_abstain`), retrieval returns an empty causal chain
    /// and `RetrievalResult::abstained = true` rather than returning the contested winner.
    /// `conflict_candidates` is still populated with the competing nodes.
    ///
    /// Default: `true` (Protocol v1.9 Section 14.6: fail-safe abstention on detected conflict).
    #[serde(default = "default_abstain_on_conflict")]
    pub abstain_on_conflict: bool,

    /// If true, `engine::start()` will quarantine the incompatible snapshot and
    /// proceed with an empty `CoreState` when a snapshot schema version mismatch
    /// is detected AND migration fails.
    ///
    /// **Default: false (fail-closed).** With the default, `start()` returns `Err`
    /// on version mismatch, the incompatible snapshot is left byte-for-byte untouched,
    /// and no data is written. The original context UUID is preserved so the operator
    /// can inspect, restore from backup, or provide a migration.
    ///
    /// Set to `true` only when you have:
    ///   1. Confirmed that the data loss (all state before last compaction) is acceptable.
    ///   2. Made a backup of the context directory.
    ///   3. Understood that this option resets the engine to an empty state, not a
    ///      "downgrade" — hence the name `incompatible_snapshot_reset`.
    #[serde(default = "default_allow_incompatible_snapshot_reset")]
    pub allow_incompatible_snapshot_reset: bool,

    /// When true, memory ingestion enforces the admission gate and suppresses
    /// insertion of near-duplicate, low-information experiences when gate_score < gate_threshold.
    /// When false (default), the gate operates in shadow mode: decisions are evaluated and logged
    /// to gate_shadow_log.jsonl, but insertion always proceeds.
    ///
    /// Epistemic boundary properties in enforcement mode (Option A):
    ///   - Hard floor: an uncorroborated virgin memory (N=0) from a caller with authority <= 0.116
    ///     cannot achieve gate_score >= 0.35 even at maximal novelty (1.0) and surprise (|pe|=1.0).
    ///   - Routine novelty floor: a virgin memory with routine surprise (|pe|=0.10) requires
    ///     authority >= 0.292 to be admitted at maximal novelty.
    ///   - Corroboration escape hatch: when content lands near an established node (N >> 1),
    ///     sample-size shrinkage blends authority with empirical Wilson LCB (w = N / (N + 4.0)),
    ///     permitting lower-authority callers to contribute to validated topic neighborhoods.
    ///   - Zero data loss: suppressed experiences are unconditionally archived in Tier 0
    ///     (`learn_log.jsonl`) pre-gate and can be promoted via subsequent high-authority review.
    #[serde(default = "default_enforce_gate")]
    pub enforce_gate: bool,

    /// Ingestion gate admission threshold.
    /// gate_score = novelty * (1.0 + gate_kappa * |prediction_error|) * verifiability.
    /// Default: 0.35 (aligned with edge_birth_threshold).
    #[serde(default = "default_gate_threshold")]
    pub gate_threshold: f32,

    /// Weight on |prediction_error| in the gate score formula.
    /// Default: 2.0.
    #[serde(default = "default_gate_kappa")]
    pub gate_kappa: f32,

    /// Number of nearest-neighbor candidates to request for the seed pool.
    ///
    /// Larger values increase recall (more candidates survive filtering and scoring)
    /// at the cost of O(pool_size) conflict detection and scoring work.
    /// Default: 20. Old hardcoded value was 8 — doubled for recall improvement (R3).
    #[serde(default = "default_retrieval_seed_pool_size")]
    pub retrieval_seed_pool_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            working_memory_size: 5,
            edge_birth_threshold: 0.35,
            novelty_threshold: 0.55,
            salience_threshold: 0.70,
            forgetting_lambda_c: 0.1,
            temporal_decay_lambda_t: 0.0001,
            cohort_count: 512,
            dense_core_slices: 8,
            consolidation_interval_hours: 24.0,
            consolidation_node_trigger: 8000,
            consolidation_reinforce_trigger: 50.0,
            data_dir: PathBuf::from("data"),
            projections_dir: PathBuf::from("data/projections"),
            retrieval_max_depth: 3,
            max_active_contexts: 10,
            idle_timeout_secs: 300,
            sleep_timeout_secs: 1800,
            snapshot_interval_cmds: 1000,
            router_alpha: 0.50,
            router_beta: 0.20,
            router_gamma: 0.20,
            router_delta: 0.10,
            router_recency_half_life_secs: 86400.0,
            retrieval_query_weight: 0.7,      // unchanged (cosine is primary for equal-authority nodes)
            retrieval_context_weight: 0.3,
            retrieval_curiosity_threshold: 0.1,
            retrieval_authority_weight:    0.75, // R2: reduced from 0.8, still authority-safe
            retrieval_recency_weight:      0.0,
            retrieval_recency_lambda:      0.01,
            conflict_detection_threshold:  0.0,
            abstain_on_conflict:           true,
            allow_incompatible_snapshot_reset: false,
            enforce_gate:                  false,
            gate_threshold:                0.35,
            gate_kappa:                    2.0,
            retrieval_seed_pool_size:      20,  // R3: was hardcoded 8
        }
    }
}

fn default_retrieval_query_weight()        -> f32   { 0.7 }  // R2: balanced (was 0.7 — unchanged)
fn default_retrieval_context_weight()      -> f32   { 0.3 }
fn default_retrieval_curiosity_threshold() -> f32   { 0.1 }
fn default_retrieval_authority_weight()    -> f32   { 0.75 } // R2: reduced from 0.8, still authority-safe
fn default_retrieval_recency_weight()      -> f32   { 0.0 }
fn default_retrieval_recency_lambda()      -> f32   { 0.01 }
fn default_conflict_detection_threshold()  -> f32   { 0.0 }
fn default_abstain_on_conflict()           -> bool  { true }
fn default_allow_incompatible_snapshot_reset() -> bool { false }
fn default_enforce_gate()                  -> bool  { false }
fn default_gate_threshold()                -> f32   { 0.35 }
fn default_gate_kappa()                    -> f32   { 2.0 }
fn default_retrieval_seed_pool_size()      -> usize { 20 }  // R3: was hardcoded 8


impl Config {
    /// Load from a TOML file. Missing fields fall back to defaults.
    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&contents)?;
        Ok(cfg)
    }

    /// Save default config to a TOML file.
    pub fn save_default(path: &std::path::Path) -> anyhow::Result<()> {
        let cfg = Self::default();
        let toml_str = toml::to_string_pretty(&cfg)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }
}
