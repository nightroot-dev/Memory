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
    learn_ledger::LearnLedger,
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
     /// BM25 lexical index over scrubbed node text (rebuilt from Node.text on load).
     #[serde(skip)]
     pub bm25_index: crate::bm25::Bm25Index,
     /// B1b learn-event ledger: every `process_learn` triple (IDs + timestamp,
     /// no text). Persisted in snapshot v4; the node→episode index inside it is
     /// transient and rebuilt on load.
     #[serde(default)]
     pub learn_ledger: LearnLedger,

    /// Monotonically increasing command sequence number.
    pub cmd_seq: u64,
    /// Unix seconds of last consolidation.
    pub last_consolidation_secs: f64,
    /// Cumulative |Δreinforce| since last consolidation.
    pub cumulative_reinforce: f32,
    /// Dead field — never read. All EMA logic uses NodeStore::global_ema (Vec<f32>).
    /// Kept in serde layout for bincode positional compatibility with on-disk snapshots.
    #[serde(with = "crate::serde_helpers::f32_128")]
    pub global_ema: [f32; 128],

    /// Content-addressed set of applied feedback keys (FNV-1a 64-bit hashes).
    /// Original key format: `"{retrieve_id}/{node_id}/{success_as_01}"`.
    /// Stored as u64 (8 B/entry) instead of String (60 B/entry) — P(collision) ≈ 2⁻⁶⁴.
    /// Prevents Beta(α,β) from being updated twice for the same (retrieve, node, outcome) triple.
    #[serde(default)]
    pub feedback_seen: HashSet<u64>,

    /// Running P₀.₃₀ quantile of observed max_cosine values (one per `birth_node` call).
    ///
    /// Used as the adaptive birth gate threshold: a candidate is admitted when its
    /// `max_cosine < birth_q30`, which targets a ~30% birth rate regardless of store size N.
    ///
    /// ## Estimator (Robbins–Monro stochastic gradient, p = 0.30)
    /// ```text
    ///   err = max_cosine − birth_q30
    ///   birth_q30 += lr · (0.30 − I[err < 0])
    /// ```
    /// With lr = 0.01, the estimator converges in ~100 observations.
    ///
    /// ## Why not τ = constant?
    /// E[max cosine over N random vectors] ≈ σ√(2 ln N), σ = 1/√d.
    /// At d=128: σ = 0.0884. The expected max grows from ~0 at N=1 to ~0.33 at N=10k.
    /// A fixed threshold collapses birth rate to zero as the store grows.
    /// A running quantile is self-calibrating across N, dimension, and embedding backends.
    #[serde(default = "default_birth_q30")]
    pub birth_q30: f32,

    /// S-05 crash-safe replay watermark: number of WAL entries whose effects
    /// are already included in this snapshot.
    ///
    /// The WAL is append-only within a session and truncated only at the
    /// startup compaction point. A mid-life snapshot retains the WAL, so a
    /// crash restart must skip the first `last_applied_wal_seq` WAL entries
    /// (already in the snapshot) and replay only the tail. Terminal snapshots
    /// (shutdown / startup compaction) reset this to 0 because the WAL is
    /// deleted — the snapshot is complete and the fresh WAL starts empty.
    /// Follows the `feedback_seen` exactly-once shape: snapshot-side
    /// high-water mark, no WAL format change.
    #[serde(default)]
    pub last_applied_wal_seq: u64,
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
             bm25_index: crate::bm25::Bm25Index::new(),
            learn_ledger: LearnLedger::new(),
            cmd_seq: 0,
            last_consolidation_secs: 0.0,
            cumulative_reinforce: 0.0,
            global_ema: [0.0f32; 128],
            feedback_seen: HashSet::new(),
            birth_q30: default_birth_q30(),
            last_applied_wal_seq: 0,
        }
    }
 
     /// Rebuild the BM25 lexical index from stored node text.
     ///
     /// Called after snapshot load / migration (index is `serde(skip)`).
     /// Nodes with empty text are skipped (vector-path learns, legacy nodes).
     pub fn rebuild_bm25_index(&mut self) {
         let mut index = crate::bm25::Bm25Index::new();
         for (&id, node) in self.node_store.iter() {
             if !node.text.is_empty() {
                 index.insert(id, &node.text);
             }
         }
         self.bm25_index = index;
     }
    /// Rebuild the transient learn-ledger index after snapshot load or
    /// migration (the index is never persisted).
    pub fn rebuild_ledger_index(&mut self) {
        self.learn_ledger.rebuild_index();
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

/// Default value for `birth_q30`. Set to 0.30 — a reasonable prior before any observations.
/// The Robbins–Monro estimator will adapt it within ~100 learn calls.
fn default_birth_q30() -> f32 { 0.30 }

/// FNV-1a 64-bit hash of a UTF-8 string. Deterministic and stable across builds.
///
/// Used for feedback dedup key compaction: 60 B/entry → 8 B/entry.
/// P(accidental collision for any two distinct keys) ≈ n²/2⁵³ per Bloom argument;
/// for 10M feedback events this is ~10⁻³, far below the 0.1% FP design target.
///
/// # TODO
/// FNV-1a has weak avalanche on structured suffixes (`/{node_id}/{outcome}`).
/// Upgrade to xxh3 (xxhash-rust, no_std, tiny) once added to the workspace.
pub(crate) fn fnv64(s: &str) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

/// Current Unix time in seconds (f64).
pub fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
