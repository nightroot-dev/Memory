use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use crate::{
    node_store::NodeStore,
    edge_store::EdgeStore,
    working_memory::WorkingMemory,
    episode_log::EpisodeLog,
    salience::SalienceEngine,
    dense_core::DenseCore,
    consolidation::Cohorts,
    projection::ProjectionRegistry,
};

/// Master state struct — owned exclusively by the single worker thread.
/// All subsystems live here.
#[derive(Debug, Serialize, Deserialize)]
pub struct CoreState {
    pub node_store: NodeStore,
    pub edge_store: EdgeStore,
    pub working_memory: WorkingMemory,
    pub episode_log: EpisodeLog,
    pub salience_engine: SalienceEngine,
    pub dense_core: DenseCore,
    pub cohorts: Cohorts,
    pub projections: ProjectionRegistry,

    /// Monotonically increasing command sequence number.
    pub cmd_seq: u64,
    /// Unix seconds of last consolidation.
    pub last_consolidation_secs: f64,
    /// Cumulative |Δreinforce| since last consolidation.
    pub cumulative_reinforce: f32,
    /// Global EMA of embeddings (used in node birth blending).
    #[serde(with = "crate::serde_helpers::f32_128")]
    pub global_ema: [f32; 128],

    /// Content-addressed set of applied feedback keys.
    /// Key format: `"{retrieve_id}/{node_id}/{success_as_01}"`
    /// Prevents Beta(α,β) from being updated twice for the same (retrieve, node, outcome) triple.
    /// `#[serde(default)]` ensures old snapshots without this field deserialise to empty set.
    #[serde(default)]
    pub feedback_seen: HashSet<String>,
}

impl CoreState {
    pub fn new(working_memory_size: usize) -> Self {
        Self {
            node_store: NodeStore::new(),
            edge_store: EdgeStore::new(),
            working_memory: WorkingMemory::new(working_memory_size),
            episode_log: EpisodeLog::new(),
            salience_engine: SalienceEngine::new(),
            dense_core: DenseCore::new(),
            cohorts: Cohorts::new(),
            projections: ProjectionRegistry::new(),
            cmd_seq: 0,
            last_consolidation_secs: 0.0,
            cumulative_reinforce: 0.0,
            global_ema: [0.0f32; 128],
            feedback_seen: HashSet::new(),
        }
    }

    /// Check if any consolidation trigger is met.
    pub fn should_consolidate(&self, cfg: &crate::config::Config) -> bool {
        let now = now_secs();
        let hours_since = (now - self.last_consolidation_secs) / 3600.0;
        hours_since > cfg.consolidation_interval_hours
            || self.node_store.len() > cfg.consolidation_node_trigger
            || self.cumulative_reinforce > cfg.consolidation_reinforce_trigger
    }
}

/// Current Unix time in seconds (f64).
pub fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
