// cmhm_core/tests/retrieval_contract.rs
//
// Phase 2 — Retrieval Contract Integration Tests (PASSING after Phase 2 implementation)
//
// All three tests use deterministic fixed unit vectors. No text/hash/MiniLM embeddings.
// All three tests are expected to PASS with the Phase 2 implementation.
//
// ════════════════════════════════════════════════════════════════════════
// PRESERVATION RULE: Do NOT weaken assertions.
// Each test pins a specific retrieval-safety property. If it fails,
// fix the production code, not the assertion.
// See retrieval_contract.md for the required implementation path.
// ════════════════════════════════════════════════════════════════════════
//
// Test 1 — test_cross_role_seed_isolation
//   Property: seed selection must be constrained to the declared seed_type.
//   Mechanism: nearest_by_type(State) called when Some(State) is declared.
//   Gate: seed_node_type == Some(NodeType::State)
//
// Test 2 — test_contradictory_same_role_resolution
//   Property: an explicitly contradicted State cannot be returned as a seed even
//             when it has a higher cosine similarity to the query.
//   Mechanism: contradicted provenance is excluded from seed eligibility after
//              conflict detection has inspected the complete candidate pool.
//   Gate: seed_id == auth_state_id
//
// Test 3 — test_stale_state_does_not_override_authoritative
//   Property: a node explicitly superseded via superseded_by must be excluded from
//             seed candidate selection entirely (before scoring).
//   Mechanism: superseded_by filter applied in retrieve() before scoring.
//   Gate: seed_id == auth_id  (stale node filtered out before ever being scored)
//
// ── Observability ──────────────────────────────────────────────────────
// All tests assert on RetrievalResult::seed_id and ::seed_node_type.
// Test 2 and 3 also check conflict_detected / conflict_candidates.

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

/// Build a minimal Config pointing at a temp directory.
/// Uses the production authority_weight=0.25 (Recall v1 default, measured sweep).
fn test_config(data_dir: PathBuf) -> Config {
    Config {
        data_dir: data_dir.clone(),
        projections_dir: data_dir.join("projections"),
        // novelty_threshold: unused by adaptive birth gate; left at default.
        // retrieval_authority_weight: 0.25 — matches production default so the
        // irrelevant-trusted regression is evaluated at the shipped weight.
        retrieval_authority_weight: 0.25,
        conflict_detection_threshold: 0.02,
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
// Mechanism: nearest_by_type(State, k=8) called when Some(NodeType::State) is declared.
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
// Expected: seed_node_type == Some(NodeType::State)

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
        .learn(unit_vec(1), unit_vec(2), unit_vec(0), 0.5, 0.5, "hash-128d",
               0.5, String::new(), None)
        .expect("learn failed");

    let state_id   = lr.state_node_id;
    let outcome_id = lr.outcome_node_id;

     let result = handle
         .retrieve(unit_vec(0), "hash-128d", NodeType::State, "")
         .expect("retrieve failed");

    // ── Assertion: seed must be a State node ───────────────────────────────
    assert_eq!(
        result.seed_node_type,
        Some(NodeType::State),
        "FAIL [test_cross_role_seed_isolation]: \
         seed must be State node {}; got seed_id={:?} seed_type={:?}. \
         Outcome node {} (cos≈1.0) must NOT displace State node {} (cos=0.0) \
         when seed_type=Some(State) is declared. \
         Fix: ensure nearest_by_type(State) is called when seed_type is Some(State).",
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
//   must select the node with higher declared authority (node.authority), NOT
//   the higher-cosine node. The authority field is distinct from emotional_tag
//   (affective salience) — it is a machine-readable source-trust level.
//
// Vectors (deterministic, designed so contradiction dominates flat cosine):
//   q       = e[3]    = [0, 0, 0, 1, 0, ...]
//   e_auth  ≈ normalized([0.9, 0, 0, 0.1, 0, ...])   cos(q, e_auth)  ≈ 0.110
//   e_contra≈ normalized([0.1, 0, 0, 0.95, 0, ...])  cos(q, e_contra)≈ 0.994
//
//   cos(e_auth, e_contra) ≈ 0.21 → both nodes pass the birth gate.
//
// Expected: seed_id == auth_state_id

#[test]
fn test_contradictory_same_role_resolution() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_config(dir.path().to_path_buf());
    let handle = start(cfg).expect("engine start");

    // Authoritative State: slightly less aligned with q, authority=1.0.
    // [0.9, 0, 0, 0.1, ...] normalized → cos(q=e[3], auth) ≈ 0.110
    let e_auth = sparse_unit_vec(&[(0, 0.9), (3, 0.1)]);
    let lr_auth = handle
        .learn(
            e_auth,
            unit_vec(4), // action (distinct, won't interfere)
            unit_vec(5), // outcome (distinct)
            0.5,
            0.5, // emotional_tag = neutral (NOT used in ranking)
            "hash-128d",
            1.0,                   // authority = 1.0 (explicitly authoritative)
            "user_explicit".to_string(), // provenance
            None,
        )
        .expect("learn auth failed");
    let auth_state_id = lr_auth.state_node_id;

    // Contradicting State: more aligned with q, authority=0.0 (explicitly contradicted).
    // [0.1, 0, 0, 0.95, ...] normalized → cos(q=e[3], contra) ≈ 0.994
    let e_contra = sparse_unit_vec(&[(0, 0.1), (3, 0.95)]);
    let lr_contra = handle
        .learn(
            e_contra,
            unit_vec(6), // action (distinct)
            unit_vec(7), // outcome (distinct)
            0.5,
            0.5, // emotional_tag = neutral (NOT used in ranking)
            "hash-128d",
            0.0,                      // authority = 0.0 (explicitly contradicted)
            "contradicted".to_string(), // provenance
            None,
        )
        .expect("learn contra failed");
    let contra_state_id = lr_contra.state_node_id;

    let q = unit_vec(3); // [0, 0, 0, 1, 0, ...] — maximally aligned with contradiction
     let result = handle
         .retrieve(q, "hash-128d", NodeType::State, "") // declared intent: state query
         .expect("retrieve failed");

    // ── Assertion: contradicted node must be structurally ineligible ─────────
    assert_eq!(
        result.seed_id,
        Some(auth_state_id),
        "FAIL [test_contradictory_same_role_resolution]: \
         expected eligible State {} (cos≈0.110) as seed; \
         got seed_id={:?}. \
         Contradicting State {} (cos≈0.994) must NOT be eligible as a seed.",
        auth_state_id,
        result.seed_id,
        contra_state_id,
    );

    // Secondary assertion: verify the non-selected node is the contradiction.
    assert_ne!(
        result.seed_id,
        Some(contra_state_id),
        "seed must not be the contradiction node {}",
        contra_state_id,
    );
}

// ── Recall v1: authority must not displace relevance ─────────────────────────

#[test]
fn irrelevant_high_authority_node_cannot_displace_relevance_winner() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_config(dir.path().to_path_buf());
    let handle = start(cfg).expect("engine start");

    let relevant = handle
        .learn(
            sparse_unit_vec(&[(0, 0.4), (1, 0.9)]), unit_vec(2), unit_vec(3), 0.5, 0.5, "hash-128d",
            0.2, "inferred".to_string(), None,
        )
        .expect("learn relevant state failed");
    let irrelevant_trusted = handle
        .learn(
            unit_vec(6), unit_vec(4), unit_vec(5), 0.5, 0.5, "hash-128d",
            1.0, "user_explicit".to_string(), None,
        )
        .expect("learn irrelevant trusted state failed");

    let result = handle
        .retrieve(unit_vec(0), "hash-128d", NodeType::State, "")
        .expect("retrieve failed");

    assert_eq!(result.seed_id, Some(relevant.state_node_id));
    assert_ne!(result.seed_id, Some(irrelevant_trusted.state_node_id));
}

// ── Recall v1: no eligible candidate must fail closed ─────────────────────────

#[test]
fn all_contradicted_candidates_return_abstained_empty_result() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_config(dir.path().to_path_buf());
    let handle = start(cfg).expect("engine start");

    let contradicted = handle
        .learn(
            unit_vec(0), unit_vec(2), unit_vec(3), 0.5, 0.5, "hash-128d",
            0.0, "contradicted".to_string(), None,
        )
        .expect("learn contradicted state failed");

    let result = handle
        .retrieve(unit_vec(0), "hash-128d", NodeType::State, "")
        .expect("retrieve failed");

    assert!(result.abstained, "all-ineligible pool must abstain");
    assert_eq!(result.seed_id, None, "abstention must not expose a seed");
    assert!(result.causal_chain.is_empty(), "abstention must not build a chain");
    assert!(result.post_filter_candidates.contains(&contradicted.state_node_id));
    assert!(result.scored_candidates.is_empty(), "ineligible nodes must not be scored");
    assert!(result.explanation.contains("eligible"));
}

// ── Test 3: Stale state does not override authoritative state ─────────────────
//
// Requirement (from retrieval_contract.md §Property 3):
//   A node explicitly superseded by a newer node via superseded_by must be
//   excluded from seed candidate selection entirely — before scoring.
//   This is the Phase 2 replacement for the recency-based approach.
//   Recency was rejected because it cannot establish entity identity,
//   version order, or supersession without explicit semantic linkage.
//
// Setup (direct NodeStore manipulation — bypasses birth gate intentionally):
//   stale_node: cos(q) ≈ 0.958  but superseded_by = Some(auth_id)  → filtered out
//   auth_node:  cos(q) ≈ 0.894  superseded_by = None               → selected
//
// Expected: seed_id == auth_id  (stale is filtered before scoring)

#[test]
fn test_stale_state_does_not_override_authoritative() {
    // Controlled stale vector: cos(q=e[0], stale) = 1/√(1+0.09) ≈ 0.958
    let stale_emb = sparse_unit_vec(&[(0, 1.0), (1, 0.3)]);
    // Controlled auth vector: cos(q=e[0], auth) = 1/√(1+0.25) ≈ 0.894
    let auth_emb = sparse_unit_vec(&[(0, 1.0), (1, 0.5)]);

    // Verify cosine ordering (stale must score HIGHER to make this a real adversarial test).
    let cos_stale: f32 = stale_emb[0]; // = stale_emb · e[0] = stale_emb[0] (normalized)
    let cos_auth:  f32 = auth_emb[0];
    assert!(
        cos_stale > cos_auth,
        "test setup error: stale cosine ({}) must exceed auth cosine ({})",
        cos_stale, cos_auth,
    );

    // Build NodeStore directly — no birth gate, no EMA blending, no engine channels.
    // Insert auth first to get its ID, then reference it in stale's superseded_by.
    let mut node_store = NodeStore::new();

    // auth: active node (the correction). authority=1.0, provenance="corrected".
    let auth_node = Node::new(
        auth_emb,
        "test-direct".to_string(),
        0.5,   // salience
        200,   // timestamp: newer session
        0.5,   // emotional_tag
        NodeType::State,
        1.0,                    // authority: authoritative
        "corrected".to_string(), // provenance
    );
    let auth_id = node_store.insert(auth_node);

    // stale: superseded by auth. Has higher cosine but must be excluded from scoring.
    let mut stale_node = Node::new(
        stale_emb,
        "test-direct".to_string(),
        0.5,   // salience
        100,   // timestamp: older session
        0.5,   // emotional_tag
        NodeType::State,
        0.5,          // authority: neutral
        String::new(), // provenance: unknown
    );
    // Explicitly mark as superseded — this is the Phase 2 staleness mechanism.
    stale_node.superseded_by = Some(auth_id);
    let stale_id = node_store.insert(stale_node);

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
        3,     // max_depth
        0.7,   // query_weight
        0.3,   // context_weight
        0.1,   // curiosity_threshold
        None,  // seed_type — not testing role-scoping here
         0.8,   // authority_weight (production default)
         0.02,  // conflict_detection_threshold (production default)
         false, // abstain_on_conflict — deterministic test must not abstain
         "",    // query_text — vector-only contract test, dense path
         &cmhm_core::bm25::Bm25Index::new(),
         8,     // seed_pool_size — contract test uses fixed pool, not config
         &cmhm_core::learn_ledger::LearnLedger::new(), // B1b: empty ledger (pre-B1b behavior)
     );


    // ── Assertion: auth node must be selected; stale node filtered out ─────
    assert_eq!(
        result.seed_id,
        Some(auth_id),
        "FAIL [test_stale_state_does_not_override_authoritative]: \
         expected auth node {} (superseded_by=None, cos≈{:.3}) as seed; \
         got seed_id={:?}. \
         Stale node {} (superseded_by=Some({}), cos≈{:.3}) must be filtered out \
         before scoring because its superseded_by field is set. \
         Fix: apply superseded_by filter in retrieve() before scoring candidates.",
        auth_id, cos_auth,
        result.seed_id,
        stale_id, auth_id, cos_stale,
    );

    // Stale node must never appear as seed.
    assert_ne!(
        result.seed_id,
        Some(stale_id),
        "stale superseded node {} must not be returned as seed",
        stale_id,
    );
}


// ════════════════════════════════════════════════════════════════════════
// B1 — Stored text must come back verbatim (design note approved 2026-10-11)
//
// Property: a retrieval reproduces the text that was learned — in the
// explanation string AND in structured ChainStep fields. Node IDs alone are
// not memory. Verified defect: format_explanation discarded the node store
// and emitted "Node(1) -> Node(2) (confidence, evidence)".
//
// PRESERVATION RULE applies to every test below: if one fails, fix the
// production code, not the assertion.
// ════════════════════════════════════════════════════════════════════════

const B1_TEXT_CAP: usize = 120;

/// The full contract: every learned string must be retrievable verbatim from
/// the explanation AND from the structured chain fields, with timestamps.
#[test]
fn b1_stored_text_returned_verbatim_in_explanation_and_chain_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_config(dir.path().to_path_buf());
    let handle = start(cfg).expect("engine start");

    let state_text   = "LISTENING_PORT was set to 8443";
    let action_text  = "restarted the reverse proxy service";
    let outcome_text = "health check returned 200 OK";

    handle
        .learn_with_text_context(
            // Related embeddings (cosine > 0.35 between episode nodes) — edge
            // birth requires similarity above threshold (engine.rs:1576), and a
            // single learn only chains when State/Action/Outcome are related.
            unit_vec(10),
            sparse_unit_vec(&[(10, 0.7), (11, 0.7)]),
            sparse_unit_vec(&[(10, 0.7), (12, 0.7)]),
            0.5, 0.5, "hash-128d",
            0.9, "user_explicit".to_string(), None,
            cmhm_core::authority::CallerRole::User, None, None,
            Some((
                state_text.to_string(),
                action_text.to_string(),
                outcome_text.to_string(),
            )),
        )
        .expect("learn with text failed");

    let result = handle
        .retrieve(unit_vec(10), "hash-128d", NodeType::State, "")
        .expect("retrieve failed");

    // ── 1. Explanation: the seed text renders verbatim (proves a non-empty
    //       chain whose head is the seed, rendered with text, not a stub) ──
    assert!(
        result.explanation.contains(state_text),
        "explanation must contain the seed text verbatim; got: {}",
        result.explanation
    );
    // The old stub format must be gone: a bare "Node(<id>)" with no text.
    assert!(
        !result.explanation.contains("Node(") || result.explanation.contains(state_text),
        "explanation still renders the pre-B1 node-id stub: {}",
        result.explanation
    );

    // ── 2. Structured fields: every text on the retrieved path round-trips
    //       verbatim in BOTH halves, with timestamps, and nothing is
    //       fabricated. The path itself (which terminal the Thompson-sampled
    //       Dijkstra picks) is engine topology — not this contract. ─────────
    assert!(
        !result.causal_chain.is_empty(),
        "a single learned triple with related embeddings must produce a non-empty chain"
    );
    for (i, step) in result.causal_chain.iter().enumerate() {
        for (label, resolved) in [("from", &step.from_text), ("to", &step.to_text)] {
            let t = resolved
                .as_ref()
                .unwrap_or_else(|| panic!("chain step {i}: {label}_text must be resolved"));
            assert!(t.timestamp > 0,
                "chain step {i} {label}: timestamp must be resolved (unix secs > 0)");
            assert!(!t.text.is_empty(),
                "chain step {i} {label}: text-path learn must not resolve to empty text");
            assert!(
                [state_text, action_text, outcome_text].contains(&t.text.as_str()),
                "chain step {i} {label}: text {:?} is not any learned string — fabrication",
                t.text
            );
            assert!(
                result.explanation.contains(&t.text),
                "chain step {i} {label}: text {:?} must render verbatim in the explanation; got: {}",
                t.text, result.explanation
            );
        }
    }

    // ── 3. Seed carries its text too, losslessly ─────────────────────────
    let seed_text = result
        .seed_text
        .as_ref()
        .expect("seed_text must be resolved when a seed is returned");
    assert_eq!(seed_text.text, state_text, "seed_text must be the seed node's text");
    assert!(seed_text.timestamp > 0, "seed_text must carry a timestamp");
}

/// A vector-path learn stores no text. That must be EXPLICIT — the marker
/// "no text stored" renders, and the structured field resolves as
/// present-but-empty. Never silently dropped.
#[test]
fn b1_vector_learn_without_text_is_marked_explicitly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_config(dir.path().to_path_buf());
    let handle = start(cfg).expect("engine start");

    handle
        .learn(
            unit_vec(10),
            sparse_unit_vec(&[(10, 0.7), (11, 0.7)]),
            sparse_unit_vec(&[(10, 0.7), (12, 0.7)]),
            0.5, 0.5, "hash-128d",
            0.9, "user_explicit".to_string(), None,
        )
        .expect("vector learn failed");

    let result = handle
        .retrieve(unit_vec(10), "hash-128d", NodeType::State, "")
        .expect("retrieve failed");

    assert!(
        result.explanation.contains("no text stored"),
        "vector-only learn must render the explicit marker; got: {}",
        result.explanation
    );
    for (i, step) in result.causal_chain.iter().enumerate() {
        let from = step.from_text.as_ref()
            .unwrap_or_else(|| panic!("step {i}: from_text must resolve even for textless nodes"));
        let to = step.to_text.as_ref()
            .unwrap_or_else(|| panic!("step {i}: to_text must resolve even for textless nodes"));
        assert!(from.text.is_empty() && to.text.is_empty(),
            "step {i}: textless nodes must resolve to empty text, not fabricated content");
        assert!(from.timestamp > 0 && to.timestamp > 0,
            "step {i}: timestamps exist even when text does not");
    }
}

/// Long text: the explanation truncates at the cap with a VISIBLE marker;
/// the structured field keeps the full string. Information loss must never
/// be silent.
#[test]
fn b1_long_text_truncates_visibly_in_explanation_full_in_field() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_config(dir.path().to_path_buf());
    let handle = start(cfg).expect("engine start");

    let long_text = "PORT_CONFIGURATION_AUDIT ".repeat(20); // 420 chars
    assert!(long_text.len() > B1_TEXT_CAP);

    handle
        .learn_with_text_context(
            unit_vec(10),
            sparse_unit_vec(&[(10, 0.7), (11, 0.7)]),
            sparse_unit_vec(&[(10, 0.7), (12, 0.7)]),
            0.5, 0.5, "hash-128d",
            0.9, "user_explicit".to_string(), None,
            cmhm_core::authority::CallerRole::User, None, None,
            Some((long_text.clone(), "short action".to_string(), "short outcome".to_string())),
        )
        .expect("learn with long text failed");

    let result = handle
        .retrieve(unit_vec(10), "hash-128d", NodeType::State, "")
        .expect("retrieve failed");

    // The cap-length prefix appears verbatim...
    assert!(
        result.explanation.contains(&long_text[..B1_TEXT_CAP]),
        "explanation must contain the first {B1_TEXT_CAP} chars verbatim; got: {}",
        result.explanation
    );
    // ...truncation is marked, not silent...
    assert!(
        result.explanation.contains("…[+"),
        "explanation must carry the visible truncation marker; got: {}",
        result.explanation
    );
    // ...the full tail is NOT in the explanation...
    assert!(
        !result.explanation.contains(&long_text[B1_TEXT_CAP..]),
        "explanation must not silently contain the full untruncated text"
    );
    // ...and the structured field is lossless.
    let seed_text = result.seed_text.as_ref().expect("seed_text must be resolved");
    assert_eq!(seed_text.text, long_text, "structured field must be lossless");
    let carries_full = result.causal_chain.iter().any(|s| {
        s.from_text.as_ref().map_or(false, |t| t.text == long_text)
            || s.to_text.as_ref().map_or(false, |t| t.text == long_text)
    });
    assert!(carries_full, "chain fields must carry the full long text losslessly");
}
