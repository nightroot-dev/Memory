// cmhm_core/src/migration.rs
//! Snapshot migration engine.
//!
//! Provides deterministic migration from earlier snapshot formats to current SNAPSHOT_VERSION = 3.
//! Preserves all node IDs, embeddings, types, timestamps, and causal edges, while populating
//! new fields with neutral, unpromoted defaults (authority = 0.50, provenance = "",
//! superseded_by = None, assertion_key = None, polarity = None).

use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use anyhow::Result;

use crate::{
    core_state::CoreState,
    node::{Node, NodeType},
    node_store::NodeStore,
    edge_store::EdgeStore,
    working_memory::WorkingMemory,
    episode_log::EpisodeLog,
    salience::SalienceEngine,
    dense_core::DenseCore,
    consolidation::Cohorts,
    projection::ProjectionRegistry,
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
        };
        migrated_nodes.insert(id, node);
    }

    let node_store = NodeStore {
        nodes: migrated_nodes,
        next_id: v2.node_store.next_id,
        global_ema: v2.node_store.global_ema,
        embed_dim: v2.node_store.embed_dim,
    };

    Ok(CoreState {
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
    })
}

/// Helper to generate a valid v2 snapshot envelope payload for fixture creation and testing.
pub fn create_test_v2_snapshot_envelope_bytes() -> Vec<u8> {
    let mut nodes = HashMap::new();
    let mut emb = vec![0.0f32; 128];
    emb[0] = 1.0;
    nodes.insert(101u64, NodeV2 {
        embedding: emb.clone(),
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

    let v2_state = CoreStateV2 {
        node_store: NodeStoreV2 {
            nodes,
            next_id: 102,
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
        cmd_seq: 5,
        last_consolidation_secs: 50.0,
        cumulative_reinforce: 0.8,
        global_ema: [0.0f32; 128],
        feedback_seen: HashSet::new(),
        birth_q30: 0.30,
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

        // 3. Component store replay-equivalent state comparison via serialized bytes.
        //    These store types do not implement PartialEq; byte-level serialization
        //    equivalence proves they round-trip identically (replay-equivalent state).
        //    Note: the v2 fixture has empty/default edge_store, working_memory, episode_log,
        //    salience_engine, dense_core, cohorts, and projections (the fixture contains
        //    only one node; these stores were empty at generation time).
        let v2_edge_bytes = bincode::serialize(&v2.edge_store).expect("v2 edge_store must serialize");
        let v3_edge_bytes = bincode::serialize(&v3.edge_store).expect("v3 edge_store must serialize");
        assert_eq!(v2_edge_bytes, v3_edge_bytes, "edge_store bytes must be identical after migration");

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

        let v2_proj_bytes = bincode::serialize(&v2.projections).expect("v2 projections must serialize");
        let v3_proj_bytes = bincode::serialize(&v3.projections).expect("v3 projections must serialize");
        assert_eq!(v2_proj_bytes, v3_proj_bytes, "projections bytes must be identical after migration");
    }
}


