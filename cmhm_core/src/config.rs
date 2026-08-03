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
            retrieval_query_weight: 0.7,
            retrieval_context_weight: 0.3,
            retrieval_curiosity_threshold: 0.1,
        }
    }
}

fn default_retrieval_query_weight()        -> f32 { 0.7 }
fn default_retrieval_context_weight()      -> f32 { 0.3 }
fn default_retrieval_curiosity_threshold() -> f32 { 0.1 }

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
