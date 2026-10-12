// cmhm_core/src/migration.rs
//! Snapshot migration engine.
//!
//! Provides deterministic migration from earlier snapshot formats to current SNAPSHOT_VERSION = 6.
//! Preserves all node IDs, embeddings, types, timestamps, and causal edges, while populating
//! new fields with neutral, unpromoted defaults (authority = 0.50, provenance = "",
//! superseded_by = None, assertion_key = None, polarity = None).

use std::collections::{HashMap, HashSet, VecDeque};
use serde::{Deserialize, Serialize};
use anyhow::Result;

use crate::{
    core_state::CoreState,
    node::{Node, NodeType},
    node_store::NodeStore,
    edge::{Edge, EdgeType, EvidenceType},
    edge_store::EdgeStore,
    working_memory::WorkingMemory,
    episode_log::EpisodeLog,
    salience::SalienceEngine,
    dense_core::DenseCore,
    consolidation::Cohorts,
    projection::{Projection, ProjectionRegistry},
    learn_ledger::{LearnLedger, LearnEpisode},
};

/// Serialized Node representation in SNAPSHOT_VERSION = 2.
/// Pre-dates authority, provenance, superseded_by, assertion_key, polarity.
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeV2 {
    pub embedding: Vec<f32>,
    pub model_id: String,
    pub salience: f32,
    pub timestamp: i64,
    pub emotional_tag: f32,
    pub node_type: NodeType,
    pub n_retrievals: u32,
    pub last_retrieved: i64,
    pub half_life_days: f32,
    pub max_prediction_error: f32,
    pub bandit_alpha: f32,
    pub bandit_beta: f32,
}

/// Serialized NodeStore in SNAPSHOT_VERSION = 2.
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeStoreV2 {
    pub nodes: HashMap<u64, NodeV2>,
    pub next_id: u64,
    pub global_ema: Vec<f32>,
    pub embed_dim: usize,
}

/// Serialized CoreState in SNAPSHOT_VERSION = 2.
#[derive(Debug, Serialize, Deserialize)]
pub struct CoreStateV2 {
    pub node_store: NodeStoreV2,
    pub edge_store: EdgeStore,
    pub working_memory: WorkingMemory,
    pub episode_log: EpisodeLog,
    pub salience_engine: SalienceEngine,
    pub dense_core: DenseCore,
    pub cohorts: Cohorts,
    pub projections: ProjectionRegistry,
    pub cmd_seq: u64,
    pub last_consolidation_secs: f64,
    pub cumulative_reinforce: f32,
    #[serde(with = "crate::serde_helpers::f32_128")]
    pub global_ema: [f32; 128],
    pub feedback_seen: HashSet<u64>,
    pub birth_q30: f32,
}

/// Migrate a v2 snapshot payload directly to v3 CoreState.
///
/// Ensures exact identity preservation:
/// - Node IDs are unchanged
/// - Embedding vectors and model IDs are unchanged
/// - Node types, salience, and timestamps are unchanged
/// - Beta bandit priors, retrieval counts, and half-lives are unchanged
/// - Neutral authority (0.50), empty provenance (""), and None supersession are assigned.
pub fn migrate_v2_to_v3(v2_payload: &[u8]) -> Result<CoreState> {
    let v2: CoreStateV2 = bincode::deserialize(v2_payload)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize v2 CoreState: {e}"))?;

    let mut migrated_nodes = HashMap::with_capacity(v2.node_store.nodes.len());
    for (id, n2) in v2.node_store.nodes {
        let node = Node {
            embedding:            n2.embedding,
            model_id:             n2.model_id,
            salience:             n2.salience,
            timestamp:            n2.timestamp,
            emotional_tag:        n2.emotional_tag,
            node_type:            n2.node_type,
            n_retrievals:         n2.n_retrievals,
            last_retrieved:       n2.last_retrieved,
            half_life_days:       n2.half_life_days,
            max_prediction_error: n2.max_prediction_error,
             bandit_alpha:         n2.bandit_alpha,
             bandit_beta:          n2.bandit_beta,
             // Phase 2 new fields assigned neutral unpromoted defaults
             authority:            0.5,
             provenance:           String::new(),
             superseded_by:        None,
             assertion_key:        None,
             polarity:             None,
             text:                 String::new(),
         };
         migrated_nodes.insert(id, node);
     }

    let node_store = NodeStore {
        nodes: migrated_nodes,
        next_id: v2.node_store.next_id,
        global_ema: v2.node_store.global_ema,
        embed_dim: v2.node_store.embed_dim,
    };

     Ok({
         let mut state = CoreState {
         node_store,
         edge_store:              v2.edge_store,
         working_memory:          v2.working_memory,
         episode_log:             v2.episode_log,
         salience_engine:         v2.salience_engine,
         dense_core:              v2.dense_core,
         cohorts:                 v2.cohorts,
         projections:             v2.projections,
         cmd_seq:                 v2.cmd_seq,
         last_consolidation_secs: v2.last_consolidation_secs,
         cumulative_reinforce:    v2.cumulative_reinforce,
         global_ema:              v2.global_ema,
         feedback_seen:           v2.feedback_seen,
         birth_q30:               v2.birth_q30,
         bm25_index:              crate::bm25::Bm25Index::new(),
         // B1b: v2 states predate the learn ledger → empty ledger (no retained
         // episodes; pre-B1b nodes resolve to "no triple recorded").
         learn_ledger:            LearnLedger::new(),
         last_applied_wal_seq:    0,
         };
         state.rebuild_bm25_index();
         state.rebuild_ledger_index();
         state
     })
}

/// Serialized CoreState in SNAPSHOT_VERSION = 3 (pre-B1b layout).
///
/// Identical to the v4 [`CoreState`] minus the learn ledger: same field order
/// and types, so bincode positional layout matches v3 snapshot bytes exactly.
/// The BM25 index is `serde(skip)` in both (never on disk).
#[derive(Debug, Serialize, Deserialize)]
pub struct CoreStateV3 {
    pub node_store: NodeStore,
    pub edge_store: EdgeStore,
    pub working_memory: WorkingMemory,
    pub episode_log: EpisodeLog,
    pub salience_engine: SalienceEngine,
    pub dense_core: DenseCore,
    pub cohorts: Cohorts,
    pub projections: ProjectionRegistry,
    pub cmd_seq: u64,
    pub last_consolidation_secs: f64,
    pub cumulative_reinforce: f32,
    #[serde(with = "crate::serde_helpers::f32_128")]
    pub global_ema: [f32; 128],
    pub feedback_seen: HashSet<u64>,
    pub birth_q30: f32,
}

/// Migrate a v3 snapshot payload directly to v4 CoreState.
///
/// Preserves every store bit-for-bit; attaches an empty learn ledger (v3
/// states predate B1b, so no episodes were ever recorded — pre-B1b nodes
/// resolve to "no triple recorded (learned before B1b)"). Rebuilds both
/// transient indexes (BM25 + ledger) before returning.
pub fn migrate_v3_to_v4(v3_payload: &[u8]) -> Result<CoreState> {
    let v3: CoreStateV3 = bincode::deserialize(v3_payload)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize v3 CoreState: {e}"))?;
    let mut state = CoreState {
        node_store:              v3.node_store,
        edge_store:              v3.edge_store,
        working_memory:          v3.working_memory,
        episode_log:             v3.episode_log,
        salience_engine:         v3.salience_engine,
        dense_core:              v3.dense_core,
        cohorts:                 v3.cohorts,
        projections:             v3.projections,
        bm25_index:              crate::bm25::Bm25Index::new(),
        cmd_seq:                 v3.cmd_seq,
        last_consolidation_secs: v3.last_consolidation_secs,
        cumulative_reinforce:    v3.cumulative_reinforce,
        global_ema:              v3.global_ema,
        feedback_seen:           v3.feedback_seen,
        birth_q30:               v3.birth_q30,
        learn_ledger:            LearnLedger::new(),
        last_applied_wal_seq:    0,
    };
    state.rebuild_bm25_index();
    state.rebuild_ledger_index();
    Ok(state)
}

/// Serialized CoreState in SNAPSHOT_VERSION = 4 (pre-R1 watermark layout).
///
/// Identical to the v5 [`CoreState`] minus `last_applied_wal_seq`: same field
/// order and types, so bincode positional layout matches v4 snapshot bytes
/// exactly. The BM25 index is `serde(skip)` in both (never on disk).
#[derive(Debug, Serialize, Deserialize)]
pub struct CoreStateV4 {
    pub node_store: NodeStore,
    pub edge_store: EdgeStore,
    pub working_memory: WorkingMemory,
    pub episode_log: EpisodeLog,
    pub salience_engine: SalienceEngine,
    pub dense_core: DenseCore,
    pub cohorts: Cohorts,
    pub projections: ProjectionRegistry,
    pub learn_ledger: LearnLedger,
    pub cmd_seq: u64,
    pub last_consolidation_secs: f64,
    pub cumulative_reinforce: f32,
    #[serde(with = "crate::serde_helpers::f32_128")]
    pub global_ema: [f32; 128],
    pub feedback_seen: HashSet<u64>,
    pub birth_q30: f32,
}

/// Serialized LearnLedger in SNAPSHOT_VERSION = 5 (pre-B6b layout).
///
/// Identical to the v6 ledger minus the `meta` sidecar table: same field
/// order and types, so bincode positional layout matches v5 bytes exactly.
/// The transient index/set are `serde(skip)` in both (never on disk).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnLedgerV5 {
    pub episodes: VecDeque<LearnEpisode>,
    pub next_seq: u64,
    pub evicted_up_to: u64,
}

/// Serialized CoreState in SNAPSHOT_VERSION = 5 (pre-B6b layout).
///
/// Identical to the v6 [`CoreState`] except its ledger is [`LearnLedgerV5`].
#[derive(Debug, Serialize, Deserialize)]
pub struct CoreStateV5 {
    pub node_store: NodeStore,
    pub edge_store: EdgeStore,
    pub working_memory: WorkingMemory,
    pub episode_log: EpisodeLog,
    pub salience_engine: SalienceEngine,
    pub dense_core: DenseCore,
    pub cohorts: Cohorts,
    pub projections: ProjectionRegistry,
    pub learn_ledger: LearnLedgerV5,
    pub cmd_seq: u64,
    pub last_consolidation_secs: f64,
    pub cumulative_reinforce: f32,
    #[serde(with = "crate::serde_helpers::f32_128")]
    pub global_ema: [f32; 128],
    pub feedback_seen: HashSet<u64>,
    pub birth_q30: f32,
    pub last_applied_wal_seq: u64,
}

/// Migrate a v5 snapshot payload directly to v6 CoreState.
///
/// Preserves every store bit-for-bit (including ledger episodes); attaches an
/// EMPTY meta table — v5 states predate typed metadata, so pre-B6b episodes
/// resolve as unclassified (filters exclude them only when active; unfiltered
/// retrieval is byte-identical behavior). Rebuilds both transient indexes.
pub fn migrate_v5_to_v6(v5_payload: &[u8]) -> Result<CoreState> {
    use crate::learn_ledger::LearnLedger;
    let v5: CoreStateV5 = bincode::deserialize(v5_payload)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize v5 CoreState: {e}"))?;
    let mut ledger = LearnLedger::from_episodes_retaining_seqs(
        v5.learn_ledger.episodes.into_iter().collect(),
        v5.learn_ledger.next_seq,
        v5.learn_ledger.evicted_up_to,
    );
    let mut state = CoreState {
        node_store:              v5.node_store,
        edge_store:              v5.edge_store,
        working_memory:          v5.working_memory,
        episode_log:             v5.episode_log,
        salience_engine:         v5.salience_engine,
        dense_core:              v5.dense_core,
        cohorts:                 v5.cohorts,
        projections:             v5.projections,
        bm25_index:              crate::bm25::Bm25Index::new(),
        learn_ledger:            ledger,
        cmd_seq:                 v5.cmd_seq,
        last_consolidation_secs: v5.last_consolidation_secs,
        cumulative_reinforce:    v5.cumulative_reinforce,
        global_ema:              v5.global_ema,
        feedback_seen:           v5.feedback_seen,
        birth_q30:               v5.birth_q30,
        last_applied_wal_seq:    v5.last_applied_wal_seq,
    };
    state.rebuild_bm25_index();
    state.rebuild_ledger_index();
    Ok(state)
}
pub fn migrate_v4_to_v5(v4_payload: &[u8]) -> Result<CoreState> {
    let v4: CoreStateV4 = bincode::deserialize(v4_payload)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize v4 CoreState: {e}"))?;
    let mut state = CoreState {
        node_store:              v4.node_store,
        edge_store:              v4.edge_store,
        working_memory:          v4.working_memory,
        episode_log:             v4.episode_log,
        salience_engine:         v4.salience_engine,
        dense_core:              v4.dense_core,
        cohorts:                 v4.cohorts,
        projections:             v4.projections,
        bm25_index:              crate::bm25::Bm25Index::new(),
        learn_ledger:            v4.learn_ledger,
        cmd_seq:                 v4.cmd_seq,
        last_consolidation_secs: v4.last_consolidation_secs,
        cumulative_reinforce:    v4.cumulative_reinforce,
        global_ema:              v4.global_ema,
        feedback_seen:           v4.feedback_seen,
        birth_q30:               v4.birth_q30,
        last_applied_wal_seq:    0,
    };
    state.rebuild_bm25_index();
    state.rebuild_ledger_index();
    Ok(state)
}

/// Helper to generate a valid v2 snapshot envelope payload for fixture creation and testing.
pub fn create_test_v2_snapshot_envelope_bytes() -> Vec<u8> {
    let mut nodes = HashMap::new();
    let mut emb0 = vec![0.0f32; 128];
    emb0[0] = 1.0;
    nodes.insert(101u64, NodeV2 {
        embedding: emb0.clone(),
        model_id: "hash-128d".to_string(),
        salience: 0.90,
        timestamp: 1700000000,
        emotional_tag: 0.5,
        node_type: NodeType::State,
        n_retrievals: 3,
        last_retrieved: 1700001000,
        half_life_days: 180.0,
        max_prediction_error: 0.1,
        bandit_alpha: 2.0,
        bandit_beta: 1.0,
    });

    let mut emb1 = vec![0.0f32; 128];
    emb1[1] = 1.0;
    nodes.insert(102u64, NodeV2 {
        embedding: emb1.clone(),
        model_id: "hash-128d".to_string(),
        salience: 0.75,
        timestamp: 1700000010,
        emotional_tag: 0.4,
        node_type: NodeType::Action,
        n_retrievals: 2,
        last_retrieved: 1700001010,
        half_life_days: 90.0,
        max_prediction_error: 0.2,
        bandit_alpha: 1.5,
        bandit_beta: 1.0,
    });

    let mut emb2 = vec![0.0f32; 128];
    emb2[2] = 1.0;
    nodes.insert(103u64, NodeV2 {
        embedding: emb2.clone(),
        model_id: "hash-128d".to_string(),
        salience: 0.65,
        timestamp: 1700000020,
        emotional_tag: 0.8,
        node_type: NodeType::Outcome,
        n_retrievals: 1,
        last_retrieved: 1700001020,
        half_life_days: 30.0,
        max_prediction_error: 0.05,
        bandit_alpha: 4.0,
        bandit_beta: 1.0,
    });

    let mut edge_store = EdgeStore::new();
    edge_store.upsert(101, 102, Edge {
        alpha: 3.5,
        beta: 1.2,
        last_traversal_time: 1700000500.0,
        edge_type: EdgeType::Sequence,
        evidence_type: EvidenceType::Inferred,
        is_intervention: false,
    });
    edge_store.upsert(102, 103, Edge {
        alpha: 6.0,
        beta: 0.8,
        last_traversal_time: 1700000600.0,
        edge_type: EdgeType::CausalHypothesis,
        evidence_type: EvidenceType::Exact,
        is_intervention: true,
    });

    let mut working_memory = WorkingMemory::new(8);
    working_memory.push(101);
    working_memory.push(102);
    working_memory.push(103);

    let mut episode_log = EpisodeLog::new();
    episode_log.log(101, 102, 0.25, true);

    let mut salience_engine = SalienceEngine::new();
    salience_engine.update_and_calc(0.45, 0.55);
    salience_engine.update_and_calc(0.85, 0.15);

    let mut dense_core = DenseCore::new();
    let mut e_arr0 = [0.0f32; 128];
    e_arr0[0] = 1.0;
    let mut e_arr1 = [0.0f32; 128];
    e_arr1[1] = 1.0;
    dense_core.update(0, &e_arr0, &e_arr1, 0.85);

    let mut cohorts = Cohorts::new();
    cohorts.centroids[0][0] = 0.4242;
    cohorts.centroids[1][5] = 0.8484;

    let mut projections = ProjectionRegistry::new();
    let mut pbytes = Vec::new();
    pbytes.extend_from_slice(&(128u32).to_le_bytes());
    pbytes.extend(vec![0u8; 128 * 128 * 4 + 128 * 4]);
    if let Ok(proj) = Projection::from_bytes(&pbytes) {
        projections.register("fixture_v2_model".into(), proj);
    }

    let mut feedback_seen = HashSet::new();
    feedback_seen.insert(9876543210123456789u64);
    feedback_seen.insert(1234567890987654321u64);

    let v2_state = CoreStateV2 {
        node_store: NodeStoreV2 {
            nodes,
            next_id: 104,
            global_ema: vec![0.0f32; 128],
            embed_dim: 128,
        },
        edge_store,
        working_memory,
        episode_log,
        salience_engine,
        dense_core,
        cohorts,
        projections,
        cmd_seq: 42,
        last_consolidation_secs: 150.0,
        cumulative_reinforce: 2.75,
        global_ema: [0.0f32; 128],
        feedback_seen,
        birth_q30: 0.285,
    };

    #[derive(Serialize)]
    struct SnapshotEnvelope {
        version: u32,
        payload: Vec<u8>,
    }

    let envelope = SnapshotEnvelope {
        version: 2,
        payload: bincode::serialize(&v2_state).unwrap(),
    };
    bincode::serialize(&envelope).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v3_to_v4_migration_preserves_stores_and_attaches_empty_ledger() {
        // Build a representative v3 state (all stores populated).
        let mut nodes = HashMap::new();
        let mut emb = vec![0.0f32; 128];
        emb[0] = 1.0;
        let mut node = Node::new(
            emb.clone(),
            "hash-128d".to_string(),
            0.85,
            123456789,
            0.5,
            NodeType::State,
            0.7,
            "user_explicit".to_string(),
        );
        node.text = "v3 state text".to_string();
        nodes.insert(42u64, node);
        let node_store = NodeStore {
            nodes,
            next_id: 43,
            global_ema: vec![0.0f32; 128],
            embed_dim: 128,
        };
        let v3_state = CoreStateV3 {
            node_store,
            edge_store: EdgeStore::new(),
            working_memory: WorkingMemory::new(8),
            episode_log: EpisodeLog::new(),
            salience_engine: SalienceEngine::new(),
            dense_core: DenseCore::new(),
            cohorts: Cohorts::new(),
            projections: ProjectionRegistry::new(),
            cmd_seq: 10,
            last_consolidation_secs: 100.0,
            cumulative_reinforce: 1.5,
            global_ema: [0.0f32; 128],
            feedback_seen: HashSet::new(),
            birth_q30: 0.30,
        };
        let payload = bincode::serialize(&v3_state).unwrap();
        let migrated = migrate_v3_to_v4(&payload).unwrap();
        // Stores preserved bit-for-bit.
        assert_eq!(migrated.node_store.len(), 1);
        let n = migrated.node_store.get(42).expect("node 42 preserved");
        assert_eq!(n.embedding, emb);
        assert_eq!(n.text, "v3 state text");
        assert_eq!(n.authority, 0.7);
        assert_eq!(n.provenance, "user_explicit");
        assert_eq!(migrated.cmd_seq, 10);
        assert_eq!(migrated.birth_q30, 0.30);
        // B1b: empty ledger, rebuilt (empty) index — pre-B1b nodes are NoTriple.
        assert_eq!(migrated.learn_ledger.len(), 0);
        assert_eq!(migrated.learn_ledger.index_len(), 0);
        let (st, ep, _) = migrated.learn_ledger.resolve_node(42);
        assert_eq!(st, crate::learn_ledger::EpisodeStatus::NoTriple);
        assert!(ep.is_none());
    }


    #[test]
    fn test_v2_to_v3_migration_preserves_identity_and_defaults() {
        let mut nodes = HashMap::new();
        let mut emb = vec![0.0f32; 128];
        emb[0] = 1.0;
        nodes.insert(42, NodeV2 {
            embedding: emb.clone(),
            model_id: "hash-128d".to_string(),
            salience: 0.85,
            timestamp: 123456789,
            emotional_tag: 0.5,
            node_type: NodeType::State,
            n_retrievals: 5,
            last_retrieved: 123456000,
            half_life_days: 180.0,
            max_prediction_error: 0.2,
            bandit_alpha: 3.0,
            bandit_beta: 1.0,
        });

        let v2_state = CoreStateV2 {
            node_store: NodeStoreV2 {
                nodes,
                next_id: 43,
                global_ema: vec![0.0f32; 128],
                embed_dim: 128,
            },
            edge_store: EdgeStore::new(),
            working_memory: WorkingMemory::new(8),
            episode_log: EpisodeLog::new(),
            salience_engine: SalienceEngine::new(),
            dense_core: DenseCore::new(),
            cohorts: Cohorts::new(),

            projections: ProjectionRegistry::new(),
            cmd_seq: 10,
            last_consolidation_secs: 100.0,
            cumulative_reinforce: 1.5,
            global_ema: [0.0f32; 128],
            feedback_seen: HashSet::new(),
            birth_q30: 0.30,
        };

        let bytes = bincode::serialize(&v2_state).unwrap();
        let migrated = migrate_v2_to_v3(&bytes).unwrap();

        assert_eq!(migrated.node_store.len(), 1);
        let node = migrated.node_store.get(42).expect("Node 42 must be preserved");
        assert_eq!(node.embedding, emb);
        assert_eq!(node.model_id, "hash-128d");
        assert_eq!(node.salience, 0.85);
        assert_eq!(node.timestamp, 123456789);
        assert_eq!(node.node_type, NodeType::State);
        assert_eq!(node.n_retrievals, 5);
        assert_eq!(node.bandit_alpha, 3.0);
        // Neutral unpromoted defaults:
        assert_eq!(node.authority, 0.5);
        assert_eq!(node.provenance, "");
        assert_eq!(node.superseded_by, None);
        assert_eq!(node.assertion_key, None);
        assert_eq!(node.polarity, None);
        assert_eq!(migrated.node_store.next_id, 43);
    }

    #[test]
    fn test_v2_snapshot_fixture_direct_field_by_field_comparison() {
        let fixture_path = std::path::Path::new("tests/fixtures/v2_snapshot.bin");
        assert!(fixture_path.exists(), "Fixture v2_snapshot.bin must exist");
        let envelope_bytes = std::fs::read(fixture_path).unwrap();

        #[derive(Deserialize)]
        struct Envelope {
            version: u32,
            payload: Vec<u8>,
        }

        let env: Envelope = bincode::deserialize(&envelope_bytes).unwrap();
        assert_eq!(env.version, 2, "v2_snapshot.bin must have version=2");

        let v2: CoreStateV2 = bincode::deserialize(&env.payload).unwrap();
        let v3: CoreState = migrate_v2_to_v3(&env.payload).unwrap();

        // 1. Exact node store and per-node field comparison
        assert_eq!(v3.node_store.nodes.len(), v2.node_store.nodes.len());
        assert_eq!(v3.node_store.next_id, v2.node_store.next_id);
        assert_eq!(v3.node_store.global_ema, v2.node_store.global_ema);
        assert_eq!(v3.node_store.embed_dim, v2.node_store.embed_dim);

        for (id, n2) in &v2.node_store.nodes {
            let n3 = v3.node_store.get(*id).unwrap_or_else(|| panic!("Node ID {} missing from v3 state", id));
            assert_eq!(n3.embedding, n2.embedding, "Embedding for node {} must match bitwise", id);
            assert_eq!(n3.model_id, n2.model_id, "Model ID for node {} must match", id);
            assert_eq!(n3.salience, n2.salience, "Salience for node {} must match", id);
            assert_eq!(n3.timestamp, n2.timestamp, "Timestamp for node {} must match", id);
            assert_eq!(n3.emotional_tag, n2.emotional_tag, "Emotional tag for node {} must match", id);
            assert_eq!(n3.node_type, n2.node_type, "NodeType for node {} must match", id);
            assert_eq!(n3.n_retrievals, n2.n_retrievals, "n_retrievals for node {} must match", id);
            assert_eq!(n3.last_retrieved, n2.last_retrieved, "last_retrieved for node {} must match", id);
            assert_eq!(n3.half_life_days, n2.half_life_days, "half_life_days for node {} must match", id);
            assert_eq!(n3.max_prediction_error, n2.max_prediction_error, "max_prediction_error for node {} must match", id);
            assert_eq!(n3.bandit_alpha, n2.bandit_alpha, "bandit_alpha for node {} must match", id);
            assert_eq!(n3.bandit_beta, n2.bandit_beta, "bandit_beta for node {} must match", id);

            // Verify v3 neutral defaults (v2 structurally had no supersession, authority, or polarity)
            assert_eq!(n3.authority, 0.50, "Migrated node {} must receive neutral authority 0.50", id);
            assert_eq!(n3.provenance, "", "Migrated node {} must receive empty provenance", id);
            assert_eq!(n3.superseded_by, None, "Migrated node {} superseded_by must be None", id);
            assert_eq!(n3.assertion_key, None, "Migrated node {} assertion_key must be None", id);
            assert_eq!(n3.polarity, None, "Migrated node {} polarity must be None", id);
        }

        // 2. Engine-level scalar metadata comparison (exact equality, integer or f32 bit-for-bit)
        assert_eq!(v3.cmd_seq, v2.cmd_seq, "cmd_seq must be preserved");
        assert_eq!(v3.last_consolidation_secs, v2.last_consolidation_secs, "last_consolidation_secs must be preserved");
        assert_eq!(v3.cumulative_reinforce, v2.cumulative_reinforce, "cumulative_reinforce must be preserved");
        assert_eq!(v3.global_ema, v2.global_ema, "global_ema must be preserved");
        assert_eq!(v3.feedback_seen, v2.feedback_seen, "feedback_seen must be preserved");
        assert_eq!(v3.birth_q30, v2.birth_q30, "birth_q30 must be preserved");

        // 3. Populated store invariant assertions: confirm stores are NOT empty
        assert!(v2.node_store.nodes.len() >= 3, "Fixture v2 node_store must be populated with >= 3 nodes");
        assert!(v2.edge_store.len() >= 2, "Fixture v2 edge_store must be populated with >= 2 edges");
        assert!(v2.working_memory.len() >= 3, "Fixture v2 working_memory must be populated with >= 3 items");
        assert!(v2.episode_log.len() >= 1, "Fixture v2 episode_log must be populated with >= 1 item");
        assert!(v2.salience_engine.count >= 2, "Fixture v2 salience_engine count must be >= 2");
        assert!(!v2.feedback_seen.is_empty(), "Fixture v2 feedback_seen must be populated");

        // 4. Detailed component store field-by-field verification
        // 4a. Edge store: verify every edge preserved bitwise
        assert_eq!(v3.edge_store.len(), v2.edge_store.len(), "edge_store len must match");
        for (src, tgt, e2) in v2.edge_store.all_edges() {
            let e3 = v3.edge_store.get(src, tgt).unwrap_or_else(|| panic!("Edge {}->{} missing in v3", src, tgt));
            assert_eq!(e3.alpha, e2.alpha, "Edge {}->{} alpha must match", src, tgt);
            assert_eq!(e3.beta, e2.beta, "Edge {}->{} beta must match", src, tgt);
            assert_eq!(e3.last_traversal_time, e2.last_traversal_time, "Edge {}->{} last_traversal_time must match", src, tgt);
            assert_eq!(e3.edge_type, e2.edge_type, "Edge {}->{} edge_type must match", src, tgt);
            assert_eq!(e3.evidence_type, e2.evidence_type, "Edge {}->{} evidence_type must match", src, tgt);
            assert_eq!(e3.is_intervention, e2.is_intervention, "Edge {}->{} is_intervention must match", src, tgt);
        }

        // 4b. Working memory: verify exact order preserved
        let wm2_recent: Vec<u64> = v2.working_memory.recent().collect();
        let wm3_recent: Vec<u64> = v3.working_memory.recent().collect();
        assert_eq!(wm3_recent, wm2_recent, "working_memory node order must match exactly");
        assert_eq!(v3.working_memory.capacity(), v2.working_memory.capacity(), "working_memory capacity must match");

        // 4c. Salience engine: verify Welford accumulator preserved
        assert_eq!(v3.salience_engine.count, v2.salience_engine.count, "salience count must match");
        assert_eq!(v3.salience_engine.mean, v2.salience_engine.mean, "salience mean must match");
        assert_eq!(v3.salience_engine.m2, v2.salience_engine.m2, "salience m2 must match");

        // 4d. Cohorts: verify centroids preserved
        assert_eq!(v3.cohorts.centroids, v2.cohorts.centroids, "cohorts centroids must match");

        // 5. Component store equivalence verification:
        // 5a. Deterministic binary layout stores (dense_core, working_memory, episode_log, salience_engine, cohorts)
        //     round-trip identically byte-for-byte.
        let v2_wm_bytes = bincode::serialize(&v2.working_memory).expect("v2 working_memory must serialize");
        let v3_wm_bytes = bincode::serialize(&v3.working_memory).expect("v3 working_memory must serialize");
        assert_eq!(v2_wm_bytes, v3_wm_bytes, "working_memory bytes must be identical after migration");

        let v2_ep_bytes = bincode::serialize(&v2.episode_log).expect("v2 episode_log must serialize");
        let v3_ep_bytes = bincode::serialize(&v3.episode_log).expect("v3 episode_log must serialize");
        assert_eq!(v2_ep_bytes, v3_ep_bytes, "episode_log bytes must be identical after migration");

        let v2_sal_bytes = bincode::serialize(&v2.salience_engine).expect("v2 salience_engine must serialize");
        let v3_sal_bytes = bincode::serialize(&v3.salience_engine).expect("v3 salience_engine must serialize");
        assert_eq!(v2_sal_bytes, v3_sal_bytes, "salience_engine bytes must be identical after migration");

        let v2_dc_bytes = bincode::serialize(&v2.dense_core).expect("v2 dense_core must serialize");
        let v3_dc_bytes = bincode::serialize(&v3.dense_core).expect("v3 dense_core must serialize");
        assert_eq!(v2_dc_bytes, v3_dc_bytes, "dense_core bytes must be identical after migration");

        let v2_coh_bytes = bincode::serialize(&v2.cohorts).expect("v2 cohorts must serialize");
        let v3_coh_bytes = bincode::serialize(&v3.cohorts).expect("v3 cohorts must serialize");
        assert_eq!(v2_coh_bytes, v3_coh_bytes, "cohorts bytes must be identical after migration");

        // 5b. DenseCore behavioral query equivalence
        let mut test_q = [0.0f32; 128];
        test_q[0] = 1.0;
        assert_eq!(v3.dense_core.query(&test_q), v2.dense_core.query(&test_q), "dense_core query output must be identical");

        // 5c. ProjectionRegistry behavioral projection equivalence
        let test_vec = vec![1.0f32; 128];
        let p2_res = v2.projections.project("fixture_v2_model", &test_vec).unwrap();
        let p3_res = v3.projections.project("fixture_v2_model", &test_vec).unwrap();
        assert_eq!(p3_res, p2_res, "projections behavior must be identical after migration");
    }
}


