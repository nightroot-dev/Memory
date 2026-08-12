// cmhm_core/tests/retrieval_contract.rs
//
// Phase 2 — Retrieval Contract Integration Tests
//
// All three tests use deterministic fixed unit vectors. No text/hash/MiniLM embeddings.
// All three tests are expected to FAIL with the current implementation.
//
// ════════════════════════════════════════════════════════════════════════
// PRESERVATION RULE: Do NOT modify assertions to make tests pass.
// Implement mitigations in production code instead.
// See retrieval_contract.md for the required implementation path.
// ════════════════════════════════════════════════════════════════════════
//
// Test 1 — test_cross_role_seed_isolation
//   Fails because: nearest(q, model_id, k) is unscoped — all NodeTypes compete.
//   Required fix:  seed selection must be constrained to a declared seed_type
//                  (add seed_type: Option<NodeType> to Command::Retrieve and
//                   use nearest_by_type when Some).
//
// Test 2 — test_contradictory_same_role_resolution
//   Fails because: flat cosine ranking selects the contradiction (higher cos)
//                  over the authoritative node (lower cos, higher emotional_tag).
//   Required fix:  retrieval ranking must incorporate an authority/salience signal,
//                  or the system must surface a conflict flag so callers can abstain.
//
// Test 3 — test_stale_state_does_not_override_authoritative
//   Fails because: no recency term in the ranking formula — the stale node (higher
//                  cosine, lower timestamp) always wins over the authoritative node
//                  (lower cosine, higher timestamp).
//   Required fix:  add a recency weight to seed scoring, OR add explicit version/
//                  supersession semantics (superseded_by field or a Supersedes edge type).
//
// ── Observability ──────────────────────────────────────────────────────
// All tests assert on RetrievalResult::seed_id and ::seed_node_type.
// These fields were added in Phase 2 as pure observability (no ranking change).
// Each test also checks that the causal-chain root matches when a chain is present.

use cmhm_core::{Config, NodeType, EMBED_DIM};
use cmhm_core::engine::start;
use cmhm_core::node::Node;
use cmhm_core::node_store::NodeStore;
use cmhm_core::edge_store::EdgeStore;
use cmhm_core::dense_core::DenseCore;
use cmhm_core::retrieval;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use std::path::PathBuf;

// ── Helpers ────────────────────────────────────────────────────────────────

/// Returns a unit vector with 1.0 at dimension `dim`, 0.0 elsewhere.
fn unit_vec(dim: usize) -> Vec<f32> {
    assert!(dim < EMBED_DIM, "dim {} >= EMBED_DIM {}", dim, EMBED_DIM);
    let mut v = vec![0.0f32; EMBED_DIM];
    v[dim] = 1.0;
    v
}

/// Returns a normalized vector with the given non-zero components.
/// Components: &[(dim_index, value)] — all other dimensions are 0.
fn sparse_unit_vec(components: &[(usize, f32)]) -> Vec<f32> {
    let mut v = vec![0.0f32; EMBED_DIM];
    for &(dim, val) in components {
        v[dim] = val;
    }
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(norm > 0.0, "zero vector not allowed");
    v.iter_mut().for_each(|x| *x /= norm);
    v
}

/// Build a minimal Config pointing at a temp directory, with high novelty_threshold
/// so all distinct vectors pass the birth gate in tests.
fn test_config(data_dir: PathBuf) -> Config {
    Config {
        data_dir: data_dir.clone(),
        projections_dir: data_dir.join("projections"),
        // Set to 1.0 so any two distinct vectors (cosine < 1.0) pass the birth gate.
        // This is intentional test scaffolding — production value is 0.55.
        novelty_threshold: 1.0,
        // Keep salience_threshold high so salience override doesn't confound results.
        salience_threshold: 1.0,
        ..Config::default()
    }
}

// ── Test 1: Cross-role seed isolation ─────────────────────────────────────────
//
// Requirement (from retrieval_contract.md §Property 1):
//   Retrieval seeding must not select Outcome or Action nodes when State nodes exist
//   and the retrieval intent is a state query. The seed selection must be constrained
//   to NodeType::State when a declared seed_type is provided.
//
// Authority signal required: an explicit seed_type parameter on Command::Retrieve /
//   EngineHandle::retrieve. Currently the API carries no query-role declaration.
//   "This embedding is a state query" is an unverifiable inference — it must be declared.
//
// Vectors (deterministic unit vectors, no embedder involved):
//   q           = e[0] = [1, 0, 0, ..., 0]
//   state_emb   = e[1] = [0, 1, 0, ..., 0]  → cos(q, state)   = 0.0
//   action_emb  = e[2] = [0, 0, 1, ..., 0]  → cos(q, action)  = 0.0
//   outcome_emb = e[0] = [1, 0, 0, ..., 0]  → cos(q, outcome) = 1.0  ← adversarial
//
// Under unscoped nearest(): Outcome wins seed selection (cos 1.0 > 0.0).
// Under state-scoped nearest_by_type(State): only State nodes are candidates;
//   State wins with cos=0.0 as the sole candidate.
//
// Expected post-mitigation: seed_node_type == Some(NodeType::State)
// Current behavior:         seed_node_type == Some(NodeType::Outcome)   ← FAILS

#[test]
fn test_cross_role_seed_isolation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_config(dir.path().to_path_buf());
    let handle = start(cfg).expect("engine start");

    //  Triple layout:
    //    learn(state=e[1], action=e[2], outcome=e[0])
    //    → State  node stored with embedding ≈ e[1] (cos=0.0 with q)
    //    → Action node stored with embedding ≈ e[2] (cos=0.0 with q)
    //    → Outcome node stored with embedding ≈ e[0] (cos≈1.0 with q)
    //
    //  EMA starts at zeros; first triple gets near-identity blend.
    //  This is the first and only triple — no birth gate interference.

    let lr = handle
        .learn(unit_vec(1), unit_vec(2), unit_vec(0), 0.5, 0.5, "hash-128d")
        .expect("learn failed");

    let state_id   = lr.state_node_id;
    let outcome_id = lr.outcome_node_id;

    let result = handle
        .retrieve(unit_vec(0), "hash-128d")
        .expect("retrieve failed");

    // ── Assertion: seed must be a State node ───────────────────────────────
    // CURRENTLY FAILS: seed_node_type is Some(NodeType::Outcome)
    // Outcome node {} was selected because nearest() is unscoped.
    assert_eq!(
        result.seed_node_type,
        Some(NodeType::State),
        "FAIL [test_cross_role_seed_isolation]: \
         seed must be State node {}; got seed_id={:?} seed_type={:?}. \
         Outcome node {} (cos≈1.0) displaced State node {} (cos=0.0) \
         because nearest() applies no role filter. \
         Fix: add seed_type: Option<NodeType> to Command::Retrieve and \
         call nearest_by_type(State) when Some(State) is declared.",
        state_id,
        result.seed_id,
        result.seed_node_type,
        outcome_id,
        state_id,
    );
}

// ── Test 2: Contradictory same-role resolution ────────────────────────────────
//
// Requirement (from retrieval_contract.md §Property 2):
//   When two State nodes share the same topic but opposite polarity, the system
//   must not silently return the higher-cosine contradiction. It must either:
//     (a) select the node with higher declared authority (emotional_tag used here),
//   or
//     (b) surface a conflict signal (has_conflict: bool) so callers can abstain.
//
// Authority signal: emotional_tag (f32, passed at learn time, stored on Node).
//   authoritative node:  emotional_tag = 1.0  (high authority)
//   contradiction node:  emotional_tag = 0.0  (low authority)
//   emotional_tag is NOT currently used in retrieval ranking.
//
// Vectors (deterministic, designed so contradiction dominates flat cosine):
//   q       = e[3]    = [0, 0, 0, 1, 0, ...]
//   e_auth  ≈ normalized([0.9, 0, 0, 0.1, 0, ...])   cos(q, e_auth)  ≈ 0.110
//   e_contra≈ normalized([0.1, 0, 0, 0.95, 0, ...])  cos(q, e_contra)≈ 0.994
//
//   cos(e_auth, e_contra) ≈ 0.21 → both nodes pass the birth gate (< 0.55 threshold).
//
// Under flat cosine ranking: contra wins (cos 0.994 > 0.110).
// Under authority-weighted ranking: auth wins (higher emotional_tag).
//
// Expected post-mitigation: seed_id == auth_state_id
// Current behavior:         seed_id == contra_state_id    ← FAILS

#[test]
fn test_contradictory_same_role_resolution() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_config(dir.path().to_path_buf());
    let handle = start(cfg).expect("engine start");

    // Authoritative State: slightly less aligned with q, high emotional_tag (authority).
    // [0.9, 0, 0, 0.1, ...] normalized → cos(q=e[3], auth) ≈ 0.110
    let e_auth = sparse_unit_vec(&[(0, 0.9), (3, 0.1)]);
    let lr_auth = handle
        .learn(
            e_auth,
            unit_vec(4), // action (distinct, won't interfere)
            unit_vec(5), // outcome (distinct)
            0.5,
            1.0, // emotional_tag = 1.0 (authority signal — NOT used in current ranking)
            "hash-128d",
        )
        .expect("learn auth failed");
    let auth_state_id = lr_auth.state_node_id;

    // Contradicting State: more aligned with q, low emotional_tag (non-authoritative).
    // [0.1, 0, 0, 0.95, ...] normalized → cos(q=e[3], contra) ≈ 0.994
    let e_contra = sparse_unit_vec(&[(0, 0.1), (3, 0.95)]);
    let lr_contra = handle
        .learn(
            e_contra,
            unit_vec(6), // action (distinct)
            unit_vec(7), // outcome (distinct)
            0.5,
            0.0, // emotional_tag = 0.0 (contradiction signal — NOT used in current ranking)
            "hash-128d",
        )
        .expect("learn contra failed");
    let contra_state_id = lr_contra.state_node_id;

    let q = unit_vec(3); // [0, 0, 0, 1, 0, ...] — maximally aligned with contradiction
    let result = handle
        .retrieve(q, "hash-128d")
        .expect("retrieve failed");

    // ── Assertion: system must NOT return the contradiction as the seed ─────
    // CURRENTLY FAILS: seed_id == Some(contra_state_id)
    // Flat cosine ranking cannot distinguish polarity.
    assert_eq!(
        result.seed_id,
        Some(auth_state_id),
        "FAIL [test_contradictory_same_role_resolution]: \
         expected authoritative State {} (emotional_tag=1.0, cos≈0.110) as seed; \
         got seed_id={:?}. \
         Contradicting State {} (emotional_tag=0.0, cos≈0.994) won flat cosine ranking. \
         Fix: incorporate emotional_tag (or a dedicated authority field) into seed scoring, \
         OR add has_conflict: bool to RetrievalResult and require callers to handle it.",
        auth_state_id,
        result.seed_id,
        contra_state_id,
    );
}

// ── Test 3: Stale state does not override authoritative state ─────────────────
//
// Requirement (from retrieval_contract.md §Property 3):
//   A stale record with higher cosine similarity to the query must not displace
//   a newer authoritative record when temporal evidence (node.timestamp) is available.
//   The ranking formula must include a recency term, or the implementation must
//   provide explicit version/supersession semantics.
//
// Authority signal: node.timestamp (Unix seconds, set at Node::new time).
//   stale node: timestamp = 100  (older)
//   auth node:  timestamp = 200  (newer, authoritative current state)
//
// This test bypasses the engine entirely and calls retrieval::retrieve directly
// with a manually constructed NodeStore. This is necessary because:
//   (a) the birth gate would reject the auth node (high cosine to stale),
//   (b) the engine assigns timestamps from Utc::now() with no override mechanism,
//   (c) we need controlled, distinct timestamps as the authority signal.
//
// Vectors:
//   q     = e[0]   = [1, 0, 0, ...]
//   stale = normalized([1, 0.3, 0, ...])  → cos(q, stale) ≈ 0.958  (more similar, stale)
//   auth  = normalized([1, 0.5, 0, ...])  → cos(q, auth)  ≈ 0.894  (less similar, authoritative)
//
// Under current ranking (no recency): stale wins (cos 0.958 > 0.894).
// Under recency-weighted ranking:     auth wins (timestamp 200 > 100).
//
// Note: once the implementation adds timestamp_override to Command::Learn or
// provides explicit supersession edges, this test should be rewritten to use
// the engine API and remove the direct NodeStore manipulation.
//
// Expected post-mitigation: seed_id == auth_id
// Current behavior:         seed_id == stale_id    ← FAILS

#[test]
fn test_stale_state_does_not_override_authoritative() {
    // Controlled stale vector: cos(q=e[0], stale) = 1/√(1+0.09) ≈ 0.958
    let stale_emb = sparse_unit_vec(&[(0, 1.0), (1, 0.3)]);
    // Controlled auth vector: cos(q=e[0], auth) = 1/√(1+0.25) ≈ 0.894
    let auth_emb = sparse_unit_vec(&[(0, 1.0), (1, 0.5)]);

    // Verify cosine ordering (stale must score HIGHER to make this a real test).
    let cos_stale: f32 = stale_emb[0]; // = stale_emb · e[0] = stale_emb[0] (already normalized)
    let cos_auth:  f32 = auth_emb[0];
    assert!(
        cos_stale > cos_auth,
        "test setup error: stale cosine ({}) must exceed auth cosine ({})",
        cos_stale, cos_auth,
    );

    // Build NodeStore directly — no birth gate, no EMA blending, no engine channels.
    let mut node_store = NodeStore::new();

    // stale: timestamp=100 (older session)
    let stale_node = Node::new(
        stale_emb,
        "test-direct".to_string(),
        0.5,   // salience
        100,   // timestamp: older session
        0.5,   // emotional_tag
        NodeType::State,
    );
    let stale_id = node_store.insert(stale_node);

    // auth: timestamp=200 (newer, authoritative session)
    let auth_node = Node::new(
        auth_emb,
        "test-direct".to_string(),
        0.5,   // salience
        200,   // timestamp: newer session — the authority signal
        0.5,   // emotional_tag
        NodeType::State,
    );
    let auth_id = node_store.insert(auth_node);

    let edge_store = EdgeStore::new();
    let dense_core = DenseCore::new();
    let mut rng = SmallRng::seed_from_u64(42); // fixed seed for determinism

    let q_arr: [f32; EMBED_DIM] = {
        let mut arr = [0.0f32; EMBED_DIM];
        arr[0] = 1.0; // e[0]
        arr
    };

    let result = retrieval::retrieve(
        &q_arr,
        "test-direct",
        &node_store,
        &edge_store,
        &dense_core,
        &mut rng,
        3,    // max_depth
        0.7,  // query_weight
        0.3,  // context_weight
        0.1,  // curiosity_threshold
    );

    // ── Assertion: auth node (newer, timestamp=200) must win ───────────────
    // CURRENTLY FAILS: seed_id == Some(stale_id)
    // stale has higher cosine (0.958 > 0.894) and no recency penalty exists.
    assert_eq!(
        result.seed_id,
        Some(auth_id),
        "FAIL [test_stale_state_does_not_override_authoritative]: \
         expected auth node {} (timestamp=200, cos≈0.894) as seed; \
         got seed_id={:?}. \
         Stale node {} (timestamp=100, cos≈0.958) won because the ranking formula \
         contains no recency term and timestamp is not consulted. \
         Fix: add recency_weight * f(node.timestamp, now_secs) to the mixed score, \
         OR add superseded_by: Option<u64> to Node and reject superseded candidates.",
        auth_id,
        result.seed_id,
        stale_id,
    );
}
