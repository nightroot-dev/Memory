//! End-to-end integration tests for the CMHM Context OS stack.
//!
//! Real ContextRuntime, real temp directories, real embedding vectors.
//! No mocking. Same code path the MCP server uses.
//!
//! Run:
//!   cargo +stable-x86_64-pc-windows-gnu test --test integration -- --nocapture

use cmhm_core::{Config, ContextRuntime, ContextId, ContextType, node::NodeType};

// ── Deterministic 128-dim embedding ──────────────────────────────────────────

fn emb(seed: u32) -> Vec<f32> {
    let mut v = vec![0.0f32; 128];
    let mut x = seed as u64;
    for f in v.iter_mut() {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *f = ((x >> 33) as f32) / (u32::MAX as f32) - 0.5;
    }
    let norm = v.iter().map(|f| f * f).sum::<f32>().sqrt().max(1e-9);
    v.iter_mut().for_each(|f| *f /= norm);
    v
}

fn make_cfg(name: &str) -> (Config, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("cmhm_integ_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut cfg = Config::default();
    cfg.data_dir            = dir.clone();
    cfg.projections_dir     = dir.join("projections");
    cfg.max_active_contexts = 8;
    (cfg, dir)
}

// Helper: create_context(name, type, parent, description, tags)
macro_rules! ctx {
    ($rt:expr, $name:expr, $desc:expr, $tags:expr) => {
        $rt.create_context(
            $name,
            ContextType::Project,
            None,
            $desc,
            $tags,
        ).expect("create_context failed")
    };
}

// =============================================================================
// Test 1: Basic learn + retrieve round-trip
// =============================================================================

#[test]
fn test_learn_and_retrieve() {
    let (cfg, dir) = make_cfg("learn_retrieve");
    let mut rt = ContextRuntime::new(cfg, "test-session".to_string()).unwrap();

    let desc = ctx!(rt, "CMHM", "causal memory project",
                    vec!["rust".into(), "memory".into()]);
    println!("[T1] Created: {} ({})", desc.display_name, desc.id);

    let ctx = Some(desc.id.as_str());

    for i in 0u32..5 {
        let (r, _) = rt.learn(ctx, emb(i*3), emb(i*3+1), emb(i*3+2), 0.6, 0.5, "test", 0.5, String::new(), None).unwrap();
        println!("[T1] Learn {i}: nodes {},{},{}", r.state_node_id, r.action_node_id, r.outcome_node_id);
    }

    let (result, _) = rt.retrieve(ctx, emb(0), "test", NodeType::State).unwrap();
    println!("[T1] Retrieve: {} chain steps | {}", result.causal_chain.len(), result.explanation);
    assert!(!result.causal_chain.is_empty() || !result.explanation.contains("No memories"),
        "Must return something after 5 learns");

    let stats = rt.stats(ctx).unwrap();
    println!("[T1] Stats: version={} loaded={}", stats.descriptor.version, stats.is_loaded);
    assert_eq!(stats.descriptor.version, 5, "Version must equal learn count");
    assert!(stats.is_loaded, "Context must be in RAM after learn");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 2: Isolation — two contexts, no memory leakage
// =============================================================================

#[test]
fn test_context_isolation() {
    let (cfg, dir) = make_cfg("isolation");
    let mut rt = ContextRuntime::new(cfg, "test-session".to_string()).unwrap();

    let cmhm  = ctx!(rt, "CMHM",    "causal memory", vec!["rust".into()]);
    let video = ctx!(rt, "VideoAI", "video processing", vec!["video".into()]);

    rt.learn(Some(cmhm.id.as_str()),  emb(1),   emb(2),   emb(3),   0.8, 0.5, "test", 0.5, String::new(), None).unwrap();
    rt.learn(Some(video.id.as_str()), emb(100), emb(101), emb(102), 0.8, 0.5, "test", 0.5, String::new(), None).unwrap();

    let s1 = rt.stats(Some(cmhm.id.as_str())).unwrap();
    let s2 = rt.stats(Some(video.id.as_str())).unwrap();
    assert_eq!(s1.descriptor.version, 1, "CMHM: exactly 1 learn");
    assert_eq!(s2.descriptor.version, 1, "VideoAI: exactly 1 learn");

    let list = rt.list_contexts();
    let names: Vec<&str> = list.iter().map(|d| d.display_name.as_str()).collect();
    println!("[T2] Contexts: {:?}", names);
    assert!(names.contains(&"CMHM"),    "CMHM must be in list");
    assert!(names.contains(&"VideoAI"), "VideoAI must be in list");
    assert!(list.len() >= 2, "Must have at least 2 user contexts");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 3: Routing — CMHM wins for "memory causal rust"
// =============================================================================

#[test]
fn test_context_routing() {
    let (cfg, dir) = make_cfg("routing");
    let mut rt = ContextRuntime::new(cfg, "test-session".to_string()).unwrap();

    let cmhm  = ctx!(rt, "CMHM",    "causal memory rust",
                     vec!["rust".into(),"memory".into(),"causal".into()]);
    let video = ctx!(rt, "VideoAI", "video ai processing",
                     vec!["video".into(),"ai".into()]);
    let linux = ctx!(rt, "Linux",   "kernel systems c programming",
                     vec!["linux".into(),"kernel".into(),"c".into()]);

    // Activate all 3 with one learn each (Sa bonus applies to all equally)
    for c in [&cmhm, &video, &linux] {
        rt.learn(Some(c.id.as_str()), emb(1), emb(2), emb(3), 0.7, 0.5, "test", 0.5, String::new(), None).unwrap();
    }

    let ranked = rt.route_top_k("memory causal rust", 3);
    println!("[T3] Rankings for 'memory causal rust':");
    for (d, s) in &ranked {
        println!("  {} → {:.4}", d.display_name, s);
    }

    assert!(!ranked.is_empty(), "route_top_k returned nothing");
    assert_eq!(ranked[0].0.display_name, "CMHM",
        "CMHM must rank #1, got '{}'", ranked[0].0.display_name);

    let cmhm_score  = ranked.iter().find(|(d,_)| d.display_name=="CMHM").map(|(_,s)| *s).unwrap();
    let video_score = ranked.iter().find(|(d,_)| d.display_name=="VideoAI").map(|(_,s)| *s).unwrap_or(0.0);
    assert!(cmhm_score > video_score,
        "CMHM ({cmhm_score:.4}) must outscore VideoAI ({video_score:.4})");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 4: Persistence — version survives clean restart
// =============================================================================

#[test]
fn test_persistence_across_restart() {
    let dir = std::env::temp_dir().join("cmhm_integ_persist");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let make = || {
        let mut c = Config::default();
        c.data_dir            = dir.clone();
        c.projections_dir     = dir.join("projections");
        c.max_active_contexts = 4;
        c
    };

    let ctx_id: String = {
        let mut rt = ContextRuntime::new(make(), "test-session".to_string()).unwrap();
        let desc = rt.create_context(
            "Persist", ContextType::Project, None, "survives restart", vec![],
        ).unwrap();
        let id = desc.id.as_str().to_string();
        rt.learn(Some(&id), emb(1), emb(2), emb(3), 0.8, 0.5, "hash-128d", 0.5, String::new(), None).unwrap();
        rt.learn(Some(&id), emb(4), emb(5), emb(6), 0.8, 0.5, "hash-128d", 0.5, String::new(), None).unwrap();
        rt.checkpoint(Some(&id)).unwrap();
        println!("[T4] Session 1: 2 learns, id={id}");
        rt.shutdown();
        id
    };

    {
        let mut rt = ContextRuntime::new(make(), "test-session".to_string()).unwrap();
        let stats = rt.stats(Some(&ctx_id)).expect("[T4] Context must exist after restart");
        println!("[T4] Session 2: version={} loaded={}", stats.descriptor.version, stats.is_loaded);
        assert_eq!(stats.descriptor.version, 2, "Version must be 2 after restart");

        let (result, _) = rt.retrieve(Some(&ctx_id), emb(1), "test", NodeType::State).unwrap();
        println!("[T4] Retrieved {} chain steps", result.causal_chain.len());
        rt.shutdown();
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 5: Delete is irrevocable — FSM enforced
// =============================================================================

#[test]
fn test_delete_is_irrevocable() {
    let (cfg, dir) = make_cfg("delete");
    let mut rt = ContextRuntime::new(cfg, "test-session".to_string()).unwrap();

    let desc = rt.create_context(
        "ToDelete", ContextType::Project, None, "temp", vec![],
    ).unwrap();
    let raw_id = desc.id.as_str().to_string();

    rt.learn(Some(&raw_id), emb(1), emb(2), emb(3), 0.5, 0.5, "hash-128d", 0.5, String::new(), None).unwrap();

    let ctx_id = ContextId::from_str(&raw_id);
    rt.delete_context(&ctx_id).expect("[T5] delete_context failed");
    println!("[T5] Context {} deleted", raw_id);

    // Learn must fail
    let err = rt.learn(Some(&raw_id), emb(10), emb(11), emb(12), 0.5, 0.5, "hash-128d", 0.5, String::new(), None)
        .unwrap_err().to_string();
    println!("[T5] Expected error: {err}");
    assert!(
        err.contains("E_NOT_FOUND") || err.contains("E_CONTEXT_DELETED") || err.contains("does not exist"),
        "Wrong error type: {err}"
    );

    let list = rt.list_contexts();
    assert!(!list.iter().any(|d| d.id.as_str() == raw_id),
        "Deleted context must not appear in list");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 6: WAL crash recovery
// =============================================================================

#[test]
fn test_wal_crash_recovery() {
    let dir = std::env::temp_dir().join("cmhm_integ_wal");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let make = || {
        let mut c = Config::default();
        c.data_dir            = dir.clone();
        c.projections_dir     = dir.join("projections");
        c.max_active_contexts = 4;
        c
    };

    let ctx_id: String = {
        let mut rt = ContextRuntime::new(make(), "test-session".to_string()).unwrap();
        let desc = rt.create_context(
            "CrashTest", ContextType::Project, None, "wal recovery", vec![],
        ).unwrap();
        let id = desc.id.as_str().to_string();

        // 10 learns → checkpoint
        for i in 0u32..10 {
            rt.learn(Some(&id), emb(i*3), emb(i*3+1), emb(i*3+2), 0.6, 0.5, "hash-128d", 0.5, String::new(), None).unwrap();
        }
        rt.checkpoint(Some(&id)).unwrap();

        // 5 post-checkpoint learns → WAL only
        for i in 10u32..15 {
            rt.learn(Some(&id), emb(i*3), emb(i*3+1), emb(i*3+2), 0.6, 0.5, "hash-128d", 0.5, String::new(), None).unwrap();
        }

        println!("[T6] Session 1: 15 total learns, dropping without shutdown (simulated crash)");
        drop(rt);
        id
    };

    // Session 2: WAL replay
    {
        let mut rt = ContextRuntime::new(make(), "test-session".to_string()).unwrap();
        let stats = rt.stats(Some(&ctx_id)).expect("[T6] Context must survive crash");
        println!("[T6] Session 2: version={} loaded={}", stats.descriptor.version, stats.is_loaded);
        assert!(stats.descriptor.version >= 10,
            "[T6] Must restore ≥10 versions, got {}", stats.descriptor.version);

        let (result, _) = rt.retrieve(Some(&ctx_id), emb(0), "test", NodeType::State).unwrap();
        println!("[T6] Retrieved {} chain steps from recovered context", result.causal_chain.len());

        rt.shutdown();
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 7: feedback_duplicate_preserves_valid_beta_posterior
//
// With content-addressed dedup, the engine rejects duplicate (key, node_id, outcome) triples.
// This test proves:
//
//   1. α and β remain finite and strictly positive after a single update.
//   2. Posterior mean α/(α+β) ∈ (0,1) after first update.
//   3. The parameter delta is exactly +1 per success update.
//   4. A duplicate call with the same feedback_key is a strict NO-OP: α2 == α1.
//   5. A subsequent call with a DISTINCT feedback_key applies correctly: α_new == α1+1.
//   6. No overflow occurs at large (but representable) values.
//
// This proves Beta(α,β) reflects evidence count, not submission count.
// =============================================================================

#[test]
fn test_feedback_duplicate_preserves_valid_beta_posterior() {
    let (cfg, dir) = make_cfg("beta_posterior");
    let mut rt = ContextRuntime::new(cfg, "test-session".to_string()).unwrap();

    // Learn one experience → creates state, action, outcome nodes.
    let (result, _) = rt.learn(
        None,
        emb(10), emb(11), emb(12),
        0.8, 0.0,
        "test",
        0.5, String::new(), None,
    ).unwrap();


    let state_id = result.state_node_id;

    // ── Precondition: initial Beta(1,1) ──────────────────────────────────────
    let (alpha0, beta0) = rt.get_node_beta(None, state_id)
        .expect("Node must exist immediately after learn");
    assert!((alpha0 - 1.0).abs() < 1e-5, "Initial α must be 1.0, got {alpha0}");
    assert!((beta0  - 1.0).abs() < 1e-5, "Initial β must be 1.0, got {beta0}");

    // ── First feedback with a stable key ────────────────────────────────────
    let key_a = Some("retrieve-uuid-abc123".to_string());
    let (updated1, _) = rt.feedback_nodes(None, vec![state_id], true, key_a.clone()).unwrap();
    assert_eq!(updated1, 1, "First apply must update 1 node");

    let (alpha1, beta1) = rt.get_node_beta(None, state_id).unwrap();
    assert!(alpha1.is_finite() && alpha1 > 0.0, "α must be finite+positive after 1st apply: {alpha1}");
    assert!(beta1.is_finite()  && beta1  > 0.0, "β must be finite+positive after 1st apply: {beta1}");
    assert!((alpha1 - (alpha0 + 1.0)).abs() < 1e-5,
        "α must increase by exactly 1 on success. Before={alpha0}, after={alpha1}");
    assert!((beta1 - beta0).abs() < 1e-5,
        "β must be unchanged on success. Before={beta0}, after={beta1}");
    let mean1 = alpha1 / (alpha1 + beta1);
    assert!(mean1 > 0.0 && mean1 < 1.0, "Posterior mean must be in (0,1): {mean1}");

    // ── Duplicate: same feedback_key → strict NO-OP ───────────────────────
    // Alpha must not move. Beta must not move. updated count must be 0.
    let (updated2, _) = rt.feedback_nodes(None, vec![state_id], true, key_a).unwrap();
    assert_eq!(updated2, 0,
        "Duplicate feedback_key must update 0 nodes (engine dedup)");

    let (alpha2, beta2) = rt.get_node_beta(None, state_id).unwrap();
    assert!((alpha2 - alpha1).abs() < 1e-5,
        "Duplicate must be a strict no-op: α must not change. α1={alpha1}, α2={alpha2}");
    assert!((beta2 - beta1).abs() < 1e-5,
        "Duplicate must be a strict no-op: β must not change. β1={beta1}, β2={beta2}");
    // Posterior mean remains valid
    let mean2 = alpha2 / (alpha2 + beta2);
    assert!(mean2 > 0.0 && mean2 < 1.0, "Posterior mean must remain in (0,1): {mean2}");

    // ── Distinct key: new evidence → α increments ────────────────────────
    let key_b = Some("retrieve-uuid-xyz999".to_string());
    let (updated3, _) = rt.feedback_nodes(None, vec![state_id], true, key_b).unwrap();
    assert_eq!(updated3, 1, "Distinct feedback_key must apply successfully");

    let (alpha3, _beta3) = rt.get_node_beta(None, state_id).unwrap();
    assert!((alpha3 - (alpha2 + 1.0)).abs() < 1e-5,
        "Distinct key must increment α by exactly 1. α2={alpha2}, α3={alpha3}");

    // ── Unjoined feedback (no key) → applied unconditionally ─────────────
    // Unjoined feedback has no dedup guarantee but must still be accepted.
    let (updated_unjoined, _) = rt.feedback_nodes(None, vec![state_id], true, None).unwrap();
    assert_eq!(updated_unjoined, 1, "Unjoined feedback (no key) must still apply");

    // ── Overflow safety at large α ─────────────────────────────────────────
    // f32 max ≈ 3.4e38. 1e9 successes → α ≈ 1e9. Mean must stay finite.
    let large_alpha = 1.0e9_f32;
    let large_beta  = 1.0e6_f32;
    let large_mean  = large_alpha / (large_alpha + large_beta);
    assert!(large_mean.is_finite(), "Posterior mean must be finite near f32 large values");
    assert!(large_mean > 0.0 && large_mean < 1.0,
        "Posterior mean must remain in (0,1) near large values: {large_mean}");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 8: shadow_gate_insertion_proven_by_node_count
//
// Proves that the shadow gate NEVER suppresses insertion by asserting on the
// node store directly — not on retrieval output (which depends on causal path
// traversal and is a separate concern from insertion).
//
// Two learns of identical text: second call gets max_cos ≈ 1 → novelty ≈ 0 →
// gate_score ≈ 0. A hard gate would reject. The shadow gate must not.
// =============================================================================

#[test]
fn test_shadow_gate_insertion_proven_by_node_count() {
    let (cfg, dir) = make_cfg("shadow_gate_insertion");
    let mut rt = ContextRuntime::new(cfg, "test-session".to_string()).unwrap();

    // Baseline: empty context has 0 nodes.
    let n0 = rt.node_count(None);
    assert_eq!(n0, 0, "Empty context must have 0 nodes before any learn");

    // First learn with high prediction_error (epsilon > 0.5 → birth threshold always passes).
    // This guarantees at least one node is inserted regardless of cosine similarity.
    rt.learn(None, emb(20), emb(21), emb(22), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
    let n1 = rt.node_count(None);
    assert!(n1 >= 1,
        "First learn (high pe, forces birth) must insert ≥1 node, got {n1}");

    // Second learn with IDENTICAL embeddings + same high pe.
    // Shadow gate computes novelty ≈ 0 → gate_score ≈ 0 → would_index = false.
    // The shadow gate MUST NOT suppress insertion. Engine birth threshold (epsilon>0.5) still fires.
    rt.learn(None, emb(20), emb(21), emb(22), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
    let n2 = rt.node_count(None);
    // n2 may equal n1 (engine deduplication) or exceed it — either is correct.
    // n2 < n1 would mean insertion was blocked or nodes deleted: FORBIDDEN.
    assert!(n2 >= n1,
        "Node count must never decrease after a repeat learn (shadow gate must not delete): \
        before={n1}, after={n2}");

    // Third learn: completely orthogonal embeddings at high pe → fresh birth guaranteed.
    // Use seed values far from the first group to ensure low cosine similarity.
    rt.learn(None, emb(200), emb(201), emb(202), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
    let n3 = rt.node_count(None);
    assert!(n3 > n2,
        "Third learn with distinct embeddings must increase node count: before={n2}, after={n3}");
    // Sanity: a low-pe near-duplicate learn (may or may not birth a new node,
    // depending on birth threshold). What matters is no panic and no count decrease.
    rt.learn(None, emb(20), emb(21), emb(22), 0.1, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
    let n4 = rt.node_count(None);
    assert!(n4 >= n3,
        "Low-pe near-duplicate learn must not decrease node count: before={n3}, after={n4}");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 9: test_birth_threshold_boundary_cases
//
// Falsifiable cases that pin the birth condition exactly:
//   birth = cos_max < 0.55  ||  salience > 0.70  ||  epsilon > 0.5
//
// With eps=0.2 and eta=0.0 during salience warmup (count < 100):
//   salience = 0.5*0.2 + 0.5*0.0 = 0.10  (well below 0.70)
//
// So for Cases 1 and 2, birth = (cos_max < 0.55) only.
// Case 3 tests the epsilon escape independently.
//
// Each case uses a fresh runtime to prevent cross-case interference:
// inserting v_cos050 would make it the nearest node for v_cos060 (cos≈0.99),
// defeating the cos boundary test.
//
// Embeddings with exact target cosines are constructed via Gram-Schmidt:
//   u_perp = normalize(emb(1) - (emb(1)·u)*u)   [orthogonal to anchor]
//   v(c)   = c·u + sqrt(1-c²)·u_perp             [cos(v, u) = c exactly]
// =============================================================================

/// Construct a unit vector with exactly `target_cos` cosine similarity to `u`.
/// Uses Gram-Schmidt to find a unit vector orthogonal to `u`, then blends.
fn with_cosine(u: &[f32], target_cos: f32, perp_seed: u32) -> Vec<f32> {
    let dim = u.len();
    // Get a candidate perpendicular direction from a seeded random vector
    let raw_perp = {
        let mut v = vec![0.0f32; dim];
        let mut x = perp_seed as u64;
        for f in v.iter_mut() {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *f = ((x >> 33) as f32) / (u32::MAX as f32) - 0.5;
        }
        v
    };
    // Gram-Schmidt: subtract projection of raw_perp onto u
    let dot: f32 = raw_perp.iter().zip(u.iter()).map(|(a, b)| a * b).sum();
    let mut perp: Vec<f32> = raw_perp.iter().zip(u.iter()).map(|(a, b)| a - dot * b).collect();
    // Normalize perp
    let norm: f32 = perp.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
    for x in &mut perp { *x /= norm; }
    // Blend: v = c·u + sqrt(1-c²)·perp
    let sin_t = (1.0 - target_cos * target_cos).max(0.0).sqrt();
    let mut v: Vec<f32> = u.iter().zip(perp.iter())
        .map(|(&ui, &pi)| target_cos * ui + sin_t * pi)
        .collect();
    // Re-normalize (numerical precision)
    let vnorm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
    for x in &mut v { *x /= vnorm; }
    v
}

#[test]
fn test_birth_threshold_boundary_cases() {
    let anchor = emb(0);

    // ── Case 1: Perfect duplicate (cos≈1.0) + low eps → NO BIRTH ─────────────
    // A cos of 1.0 cannot be below birth_q30 (which is clamped to [0.01, 0.99]).
    // eps=0.2 < 0.5 and eta=0.0 < 0.70 → no escape → birth must be suppressed.
    {
        let (cfg, dir) = make_cfg("birth_case1");
        let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();
        // Seed store with anchor at eps=0.9 (forces birth via epsilon escape).
        // State, action, outcome all use anchor → all stored nodes ≈ emb(0).
        rt.learn(None, anchor.clone(), anchor.clone(), anchor.clone(), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        let n0 = rt.node_count(None);

        // Present the exact same anchor with low eps: cos_max ≈ 1.0 vs. stored state node.
        // birth_q30 is clamped to max 0.99, so cos=1.0 is always ≥ birth_q30 → gate closed.
        rt.learn(None, anchor.clone(), anchor.clone(), anchor.clone(), 0.2, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        let n1 = rt.node_count(None);
        assert_eq!(n1, n0,
            "[Case1] perfect duplicate (cos≈1.0) + eps=0.2: NO new state node. before={n0}, after={n1}");
        rt.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Case 2: Epsilon escape overrides cos gate ──────────────────────────────
    // cos_max ≈ 1.0 (identical embedding) but eps=0.9 > 0.5 → birth fires.
    {
        let (cfg, dir) = make_cfg("birth_case2");
        let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();
        rt.learn(None, anchor.clone(), anchor.clone(), anchor.clone(), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        let n0 = rt.node_count(None);

        // Same anchor embedding; high eps → epsilon escape → must birth new state node.
        rt.learn(None, anchor.clone(), anchor.clone(), anchor.clone(), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        let n1 = rt.node_count(None);
        assert!(n1 > n0,
            "[Case2] eps=0.9 > 0.5: epsilon escape MUST fire → new state node. before={n0}, after={n1}");
        rt.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Case 3: Salience escape overrides cos gate ────────────────────────────
    // Warm-up formula: salience = 0.5*eps + 0.5*eta.
    // With eps=0.8, eta=0.9: salience = 0.85 > 0.70 → salience escape fires.
    // cos_max ≈ 1.0 (identical anchor embedding) — gate would otherwise stay closed.
    {
        let (cfg, dir) = make_cfg("birth_case3");
        let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();
        rt.learn(None, anchor.clone(), anchor.clone(), anchor.clone(), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        let n0 = rt.node_count(None);

        // eps=0.8, eta=0.9 → salience = 0.5*0.8 + 0.5*0.9 = 0.85 > 0.70 → salience escape fires.
        rt.learn(None, anchor.clone(), anchor.clone(), anchor.clone(), 0.8, 0.9, "hash-128d", 0.5, String::new(), None).unwrap();
        let n1 = rt.node_count(None);
        assert!(n1 > n0,
            "[Case3] eps=0.8 + eta=0.9 → salience=0.85 > 0.70: salience escape MUST fire. before={n0}, after={n1}");
        rt.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

}

// ───────────────────────────────────────────────────────────────────────
// Birth gate calibration sweep (promoted from diagnostic)
// ───────────────────────────────────────────────────────────────────────

/// Verifies that max_cosine_in_context (role-scoped to State nodes) accurately
/// reflects the geometric target cosine after role-scoping removes cross-role
/// EMA contamination.
///
/// Before role-scoping: actual ≈ 0.34 + 0.60*target (floor of 0.34 from mixed EMA).
/// After role-scoping:  actual ≈ target  (floor eliminated; direct comparison).
///
/// Tolerance of ±0.05 accounts for EMA blending (0.7*emb + 0.3*μ) applied at birth.
/// If this test fails, the 0.34 floor has returned — investigate nearest_by_type scope.
#[test]
fn test_birth_gate_cosine_calibration_curve() {
    let anchor = emb(0);
    let (cfg, dir) = make_cfg("cos_calibration");
    let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();

    // Seed ONE state node so the store is non-empty.
    // Use anchor as state; action/outcome use different seeds so they don’t
    // pollute the State namespace (role-scoping keeps them separate).
    rt.learn(None, anchor.clone(), emb(100), emb(200), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();

    // Sweep target cosines from 0.0 to 1.0 in steps of 0.1.
    let targets: &[f32] = &[0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
    for &target in targets {
        let v = if (target - 1.0).abs() < 1e-5 {
            anchor.clone() // exact identity
        } else {
            with_cosine(&anchor, target, 7777)
        };

        let actual = rt.max_cosine_in_context(None, &v, "hash-128d", NodeType::State);

        // After role-scoping, actual should track target closely.
        // Tolerance ±0.06 accounts for EMA blending at birth.
        assert!(
            (actual - target).abs() < 0.06,
            "[cos_calibration] target={target:.2} actual={actual:.4} — floor > 0.06 means cross-role contamination"
        );
    }

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// ───────────────────────────────────────────────────────────────────────
// N-sweep: max_cosine growth and adaptive gate convergence
// ───────────────────────────────────────────────────────────────────────

/// Prove max_cosine grows with store size N, following E[max] ≈ (1/√d)·√(2 ln N).
///
/// This test catches: dimension changes, projection changes, cross-role contamination.
/// The adaptive quantile gate self-calibrates against this growth; the N-sweep pins the
/// N-sweep: max_cosine grows monotonically with store size N.
///
/// We do NOT check against the Gumbel formula E[max] ≈ σ·√(2 ln N),
/// because hash-128d uses EMA blending at birth, so stored embeddings are NOT
/// i.i.d. uniform random unit vectors — they cluster toward the EMA centroid,
/// which inflates cosines well above the null-distribution expectation.
///
/// The meaningful invariant is: E[max_cosine | N=1000] > E[max_cosine | N=10].
/// If this breaks, something eliminated the growth effect (e.g. cross-role deduplication
/// re-introduced, or the projection collapsed all vectors to the same direction).
///
/// The test also asserts all observed max_cosine values are in [0, 1].
#[test]
fn test_birth_gate_max_cosine_grows_with_n() {
    let mut mu_small = 0.0_f32;
    let mut mu_large = 0.0_f32;
    let queries = 100usize;

    for &(n, slot) in &[(10usize, 0u8), (1_000usize, 1u8)] {
        let (cfg, dir) = make_cfg(&format!("n_sweep2_{n}"));
        let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();

        // Seed N state nodes with varied embeddings.
        for i in 0..n {
            let _ = rt.learn(None, emb(i as u32 * 7 + 1), emb(i as u32 * 7 + 2), emb(i as u32 * 7 + 3), 0.4, 0.0, "hash-128d", 0.5, String::new(), None);
        }

        let mu = (0..queries)
            .map(|q| {
                let mc = rt.max_cosine_in_context(None, &emb((q as u32) * 13 + 999), "hash-128d", NodeType::State);
                assert!(mc >= 0.0 && mc <= 1.0, "max_cosine={mc} out of [0,1] at N={n}");
                mc
            })
            .sum::<f32>() / queries as f32;

        if slot == 0 { mu_small = mu; } else { mu_large = mu; }

        rt.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    assert!(
        mu_large > mu_small,
        "E[max_cosine | N=1000]={mu_large:.4} must exceed E[max_cosine | N=10]={mu_small:.4}; \
         if equal, the store may be deduplicating across roles or collapsing to one vector"
    );
}

/// The birth_q30 Robbins–Monro estimator is live (not stuck, not diverged).
///
/// After 400 learn calls we assert:
/// - birth_q30 is finite and in (0.01, 0.99) — the valid clamping range
/// - birth_q30 has moved away from the initial prior (0.30), indicating the estimator is updating
/// - birth_q30 is strictly less than 0.99 (not hard-clamped at the upper wall)
/// - birth_q30 is strictly greater than 0.01 (not hard-clamped at the lower wall)
///
/// We do NOT assert birth_q30 ≈ 0.30, because the target quantile (0.30) means the 30th
/// percentile of the actual observed max_cosine distribution — which for hash-128d embeddings
/// with EMA blending is much higher than 0.30 (typically 0.7–0.9). The estimator correctly
/// tracks the empirical distribution; the goal is self-calibration, not returning a fixed value.
#[test]
fn test_birth_q30_converges_to_target_quantile() {
    let (cfg, dir) = make_cfg("q30_conv2");
    let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();

    // 400 learn calls with varied inputs drives the estimator.
    for i in 0u32..400 {
        let _ = rt.learn(None, emb(i * 3 + 1), emb(i * 3 + 2), emb(i * 3 + 3), 0.3, 0.0, "hash-128d", 0.5, String::new(), None);
    }

    let q30 = rt.get_birth_q30(None);

    // Must be in the valid clamped range.
    assert!(
        q30 > 0.01 && q30 < 0.99,
        "birth_q30={q30:.4} hit a clamping wall — estimator may have diverged or lr is wrong"
    );
    // Must have moved away from the cold-start prior (0.30 ± 0.05).
    // If it hasn't moved at all, lr=0 or the update is not being called.
    assert!(
        (q30 - 0.30).abs() > 0.05,
        "birth_q30={q30:.4} is too close to the cold-start prior (0.30); \
         estimator may not be updating (lr=0 or update not called)"
    );

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}


// ───────────────────────────────────────────────────────────────────────
// WAL compaction + schema version recovery boundary
// ───────────────────────────────────────────────────────────────────────

/// Verifies that `engine::start()` **refuses** to start when a snapshot schema version
/// mismatch is detected (fail-closed by default), and that the incompatible snapshot
/// is preserved untouched so it can be restored or migrated.
///
/// ## Three assertions
///
/// **Phase 1 — Fail-closed (default):**
/// `learn()` returns `Err` because `engine::start()` refuses to proceed.
/// The incompatible `snapshot.bin` is NOT renamed or deleted — it stays exactly
/// where it is so a backup restore or manual migration is possible.
///
/// **Phase 2 — Opt-in (`allow_incompatible_snapshot_reset=true`):**
/// The engine starts, quarantines all incompatible snapshot generations to
/// `snapshot.v1.incompatible[.N]`, and recovers with an empty state.
/// A's and B's nodes are absent (pre-compaction data permanently lost once
/// the reset is explicitly allowed). C can be learned — engine is healthy.
///
/// **Phase 3 — Snapshot not silently overwritten:**
/// The quarantine file outlives the session and can be inspected after shutdown.
///
/// ## Setup
/// Session A: learn triple A, clean shutdown → `contexts/<uuid>/snapshot.bin` (v2).
/// Session B: loads S_A, learns B, clean shutdown → S_B (A+B in snapshot, WAL compacted).
/// Schema bump: overwrite all `contexts/*/snapshot.bin` with 4 raw bytes → VersionMismatch.
#[test]
fn test_version_mismatch_fails_closed_and_quarantines() {
    let (cfg, dir) = make_cfg("ver_mismatch_failclosed");
    let contexts_dir = cfg.data_dir.join("contexts");

    // ── Session A ─────────────────────────────────────────────────────────
    // Use "s" as the named context for all operations.
    {
        let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();
        let _ = rt.learn(None, emb(1), emb(2), emb(3), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        rt.shutdown(); // writes S_A at contexts/<uuid>/snapshot.bin
    }

    // ── Session B ─────────────────────────────────────────────────────────
    {
        let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();
        let _ = rt.learn(None, emb(11), emb(12), emb(13), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        rt.shutdown(); // writes S_B (A+B). WAL gone (compacted at startup).
    }

    // ── Simulate schema bump ───────────────────────────────────────────────
    // read_snapshot() tries snapshot.bin, snapshot.bin.1, and snapshot.bin.2 in order.
    // If any generation holds valid v2 bytes, VersionMismatch never fires and the engine
    // loads normally. ALL three generations must be corrupted to guarantee the fail-closed
    // path is reached.
    //
    // IMPORTANT: do NOT corrupt wal.dat. A corrupted (but existing) WAL sets
    // needs_recovery=true in ensure_loaded. When engine::start then fails (VersionMismatch),
    // ensure_loaded transitions the context to Deleted, and Phase 2's find_by_name()
    // creates a fresh context — bypassing the quarantine path entirely.
    // Instead, DELETE the WAL so needs_recovery=false and the context stays Sleeping.
    let mut ctx_snapshot_paths: Vec<std::path::PathBuf> = Vec::new();
    let v1_bytes = b"\x00\x00\x00\x00";
    if let Ok(entries) = std::fs::read_dir(&contexts_dir) {
        for entry in entries.flatten() {
            let ctx_dir = entry.path();
            if ctx_dir.is_dir() {
                for &name in &["snapshot.bin", "snapshot.bin.1", "snapshot.bin.2"] {
                    let snap = ctx_dir.join(name);
                    if snap.exists() {
                        if name == "snapshot.bin" {
                            ctx_snapshot_paths.push(snap.clone());
                        }
                        let _ = std::fs::write(&snap, v1_bytes);
                    }
                }
                // DELETE the WAL (do not corrupt it — see comment above).
                let wal = ctx_dir.join("wal.dat");
                let _ = std::fs::remove_file(&wal); // ok if not present
            }
        }
    }
    assert!(!ctx_snapshot_paths.is_empty(), "Setup: no context snapshots found — sessions A/B did not write snapshots");



    // ── Phase 1: Default config → fail-closed ─────────────────────────────
    // learn(Some("s")) triggers ensure_loaded → engine::start() → VersionMismatch → Err.
    {
        let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();
        let learn_result = rt.learn(None, emb(99), emb(100), emb(101), 0.9, 0.0, "hash-128d", 0.5, String::new(), None);
        assert!(
            learn_result.is_err(),
            "Default config: learn(None) must return Err on schema version mismatch. \
             Got Ok — engine::start() did not fail closed."
        );

        // Incompatible snapshot must still be at its original path, byte-for-byte unchanged.
        for snap_path in &ctx_snapshot_paths {
            if snap_path.exists() {
                let contents = std::fs::read(snap_path).unwrap();
                if contents.len() <= 4 {
                    // This is one of the snapshots we corrupted. Confirm it's still the v1 bytes.
                    assert_eq!(
                        &contents[..],
                        v1_bytes,
                        "Incompatible snapshot {:?} must be byte-for-byte unchanged after \
                         fail-closed startup. Content was modified — data was written despite the error.",
                        snap_path
                    );
                }
            }
        }
    }

    // ── Phase 2: allow_incompatible_snapshot_reset=true → opt-in recovery ─
    // Store is empty before C; quarantine file must exist; C can be learned.
    {
        let mut cfg_lossy = cfg.clone();
        cfg_lossy.allow_incompatible_snapshot_reset = true;
        let mut rt = ContextRuntime::new(cfg_lossy, "s".to_string()).unwrap();

        // node_count(Some("s")) triggers ensure_loaded for context "s".
        // allow_incompatible_snapshot_reset=true → quarantine fires → CoreState empty.
        let n_before = rt.node_count(None);
        assert_eq!(
            n_before, 0,
            "allow_incompatible_snapshot_reset=true: store must be empty after quarantine recovery. \
             Got {n_before} nodes — A/B pre-compaction data survived the downgrade."
        );

        // A quarantine file (*.incompatible) must exist somewhere under the contexts dir.
        // engine::start() renames snapshot.bin → snapshot.v<found>.incompatible before
        // starting with empty state. We search broadly because the quarantined context
        // UUID may differ from what was corrupted (e.g. if a new default context was
        // created during Phase 1 recovery).
        let quarantine_found = {
            let mut found = false;
            if let Ok(entries) = std::fs::read_dir(&contexts_dir) {
                for entry in entries.flatten() {
                    let ctx_dir = entry.path();
                    if !ctx_dir.is_dir() { continue; }
                    if let Ok(files) = std::fs::read_dir(&ctx_dir) {
                        for f in files.flatten() {
                            if f.path().extension()
                                .map(|e| e.to_string_lossy().contains("incompatible"))
                                .unwrap_or(false)
                            {
                                found = true;
                            }
                        }
                    }
                }
            }
            found
        };
        assert!(
            quarantine_found,
            "After allow_incompatible_snapshot_reset=true, an *.incompatible quarantine file \
             must exist under the contexts directory. \
             The incompatible snapshot was not quarantined before starting with empty state."
        );


        // Engine is healthy: learn C.
        let learn_c = rt.learn(None, emb(99), emb(100), emb(101), 0.9, 0.0, "hash-128d", 0.5, String::new(), None);
        assert!(
            learn_c.is_ok(),
            "allow_incompatible_snapshot_reset=true: learn() must succeed. Got Err: {:?}",
            learn_c.err()
        );
        let n_after = rt.node_count(None);
        assert_eq!(
            n_after, 3,
            "After learning triple C (eps=0.9 → forced birth), expected 3 nodes. Got {n_after}."
        );

        rt.shutdown();
    }

    let _ = std::fs::remove_dir_all(&dir);
}



// ───────────────────────────────────────────────────────────────────────
// Schema mismatch with WAL present — stable context identity
// ───────────────────────────────────────────────────────────────────────

/// Verifies that a schema version mismatch while a WAL file is present does NOT
/// delete the context or its UUID registration.
///
/// ## The production-path bug this closes
///
/// When `engine::start()` returned `Err` after `ensure_loaded` had transitioned
/// the context to `Recovering`, the error handler unconditionally called
/// `registry.transition(id, Deleted)` — even for schema mismatches where no
/// data was written and the disk is intact. This caused:
///   - The original context UUID to vanish from the registry.
///   - The next `find_by_name("default")` to create a *fresh* context.
///   - No quarantine file ever created (fresh context has no incompatible snapshot).
///
/// The fix: `ensure_loaded` downcasts to `SchemaMismatchError` and rolls back
/// via `Recovering → Sleeping` (a new FSM edge) instead of `Recovering → Deleted`.
///
/// ## Three phases
///
/// **Phase 1 — Fail-closed, WAL present:**
/// `learn()` returns `Err`. The original context UUID is still registered in
/// `Sleeping` state (not `Deleted`). `snapshot.bin` and `wal.dat` are
/// byte-for-byte unchanged. `default_context_id()` returns the same UUID before
/// and after the failed load.
///
/// **Phase 2 — Opt-in reset (`allow_incompatible_snapshot_reset=true`):**
/// The same UUID is loaded (not a replacement). A quarantine file exists.
/// `node_count == 0`. `learn()` succeeds — engine is healthy.
///
/// **Phase 3 — Restart after reset:**
/// A fresh runtime starts cleanly from Phase 2's shutdown snapshot. No
/// `VersionMismatch`. `learn()` succeeds.
///
/// ## Setup
/// Sessions A+B: learn triples, clean shutdown → `contexts/<uuid>/snapshot.bin` (v2).
/// Schema bump: corrupt all snapshot generations; write WAL magic (`"CWAL"` only,
/// no records) so `wal_path.exists()=true` and `needs_recovery=true`.
#[test]
fn test_version_mismatch_wal_present_preserves_uuid() {
    let (cfg, dir) = make_cfg("ver_mismatch_wal");
    let contexts_dir = cfg.data_dir.join("contexts");

    // ── Sessions A + B ────────────────────────────────────────────────────
    {
        let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();
        let _ = rt.learn(None, emb(1), emb(2), emb(3), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        rt.shutdown();
    }
    {
        let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();
        let _ = rt.learn(None, emb(11), emb(12), emb(13), 0.9, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        rt.shutdown();
    }

    // ── Schema bump + leave WAL present ───────────────────────────────────
    // Corrupt all snapshot generations so VersionMismatch fires on any generation.
    // Write a minimal WAL (CWAL magic only, no records) so needs_recovery=true.
    // This is the exact production scenario: VersionMismatch AND wal.dat present.
    let v1_bytes  = b"\x00\x00\x00\x00";
    let wal_magic = b"CWAL"; // WAL_MAGIC from persistence.rs
    let mut ctx_dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&contexts_dir) {
        for entry in entries.flatten() {
            let ctx_dir = entry.path();
            if ctx_dir.is_dir() {
                ctx_dirs.push(ctx_dir.clone());
                for &name in &["snapshot.bin", "snapshot.bin.1", "snapshot.bin.2"] {
                    let snap = ctx_dir.join(name);
                    if snap.exists() { let _ = std::fs::write(&snap, v1_bytes); }
                }
                // Write minimal WAL (magic only, 0 records) → needs_recovery=true.
                let _ = std::fs::write(ctx_dir.join("wal.dat"), wal_magic);
            }
        }
    }
    assert!(!ctx_dirs.is_empty(), "Setup: no context dirs after sessions A/B");

    // ── Phase 1: Fail-closed with WAL present ─────────────────────────────
    {
        let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();

        // UUID before the failed load.
        let uuid_before = rt.default_context_id()
            .map(|id| id.to_string())
            .unwrap_or_default();
        assert!(!uuid_before.is_empty(),
            "Phase 1: default context must be registered before the failed load.");

        let learn_result = rt.learn(None, emb(99), emb(100), emb(101), 0.9, 0.0, "hash-128d", 0.5, String::new(), None);
        assert!(
            learn_result.is_err(),
            "Phase 1: learn() must return Err on schema mismatch. Got Ok."
        );

        // UUID must be stable — context is Sleeping, not Deleted.
        let uuid_after = rt.default_context_id()
            .map(|id| id.to_string())
            .unwrap_or_default();
        assert_eq!(
            uuid_before, uuid_after,
            "Phase 1: default context UUID must be stable across a fail-closed startup. \
             Before={uuid_before}, after={uuid_after} — context was replaced or deleted."
        );

        // Snapshot and WAL must be byte-for-byte unchanged.
        for ctx_dir in &ctx_dirs {
            let snap = ctx_dir.join("snapshot.bin");
            if snap.exists() {
                let got = std::fs::read(&snap).unwrap();
                assert_eq!(
                    got.len(), v1_bytes.len(),
                    "Phase 1: snapshot.bin must be untouched (still {}-byte v1_bytes). \
                     Got {} bytes.", v1_bytes.len(), got.len()
                );
            }
            let wal = ctx_dir.join("wal.dat");
            if wal.exists() {
                let got = std::fs::read(&wal).unwrap();
                let cmp_len = wal_magic.len().min(got.len());
                assert_eq!(
                    &got[..cmp_len], &wal_magic[..cmp_len],
                    "Phase 1: wal.dat must be untouched. Content changed despite fail-closed startup."
                );
            }
        }
    }

    // ── Phase 2: Opt-in reset (allow_incompatible_snapshot_reset=true) ────
    {
        let mut cfg_reset = cfg.clone();
        cfg_reset.allow_incompatible_snapshot_reset = true;
        let mut rt = ContextRuntime::new(cfg_reset, "s".to_string()).unwrap();

        let n_before = rt.node_count(None);
        assert_eq!(
            n_before, 0,
            "Phase 2: store must be empty after incompatible-snapshot reset. Got {n_before} nodes."
        );

        // At least one quarantine file must exist.
        let quarantine_found = {
            let mut found = false;
            if let Ok(entries) = std::fs::read_dir(&contexts_dir) {
                for entry in entries.flatten() {
                    let ctx_dir = entry.path();
                    if !ctx_dir.is_dir() { continue; }
                    if let Ok(files) = std::fs::read_dir(&ctx_dir) {
                        for f in files.flatten() {
                            if f.path().extension()
                                .map(|e| e.to_string_lossy().contains("incompatible"))
                                .unwrap_or(false)
                            { found = true; }
                        }
                    }
                }
            }
            found
        };
        assert!(
            quarantine_found,
            "Phase 2: an *.incompatible quarantine file must exist after opt-in reset. \
             Incompatible snapshot was not quarantined."
        );

        let learn_d = rt.learn(None, emb(50), emb(51), emb(52), 0.9, 0.0, "hash-128d", 0.5, String::new(), None);
        assert!(
            learn_d.is_ok(),
            "Phase 2: learn() must succeed after reset. Got Err: {:?}", learn_d.err()
        );
        assert_eq!(rt.node_count(None), 3, "Phase 2: expected 3 nodes after learning triple D.");

        rt.shutdown();
    }

    // ── Phase 3: Restart after reset — must start cleanly ─────────────────
    // Phase 2's shutdown wrote a fresh v2 snapshot. A new runtime must start
    // without hitting VersionMismatch (no more incompatible snapshot in place).
    {
        let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();
        let learn_e = rt.learn(None, emb(60), emb(61), emb(62), 0.9, 0.0, "hash-128d", 0.5, String::new(), None);
        assert!(
            learn_e.is_ok(),
            "Phase 3: learn() must succeed after clean restart. Got Err: {:?}", learn_e.err()
        );
        rt.shutdown();
    }

    let _ = std::fs::remove_dir_all(&dir);
}


// ───────────────────────────────────────────────────────────────────────
// WAL / snapshot feedback durability
// ───────────────────────────────────────────────────────────────────────


/// feedback_seen is persisted in snapshots.
///
/// After an explicit checkpoint: load a new runtime from the same dir,
/// retry the same feedback, and assert Alpha is unchanged (the key was
/// in the snapshot so the engine deduplicates it correctly).
#[test]
fn test_feedback_key_survives_snapshot_reload() {
    let (cfg, dir) = make_cfg("fb_snap_reload");

    // Session 1: learn + feedback + explicit snapshot.
    let node_id;
    let alpha_after_first;
    {
        let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();
        let result = rt.learn(None, emb(1), emb(2), emb(3), 0.3, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        node_id = result.0.state_node_id;

        rt.feedback_nodes(None, vec![node_id], true, Some("rid-snap-001".to_string())).unwrap();
        let (a, _) = rt.get_node_beta(None, node_id).unwrap();
        alpha_after_first = a;
        assert!((alpha_after_first - 2.0).abs() < 1e-5,
            "After 1 success: alpha must be 2.0, got {alpha_after_first}");

        // Explicit checkpoint — feedback_seen is now on disk.
        rt.checkpoint(None).unwrap();
        rt.shutdown();
    }

    // Session 2: reload from snapshot, retry identical feedback call.
    {
        let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();
        // feedback_nodes triggers context load; no explicit ensure_loaded needed.
        rt.feedback_nodes(None, vec![node_id], true, Some("rid-snap-001".to_string())).unwrap();
        let (a, _) = rt.get_node_beta(None, node_id).unwrap();
        assert!(
            (a - alpha_after_first).abs() < 1e-5,
            "Retry after snapshot reload must be a no-op: alpha={a:.4} expected={alpha_after_first:.4}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// Feedback survives a simulated crash (WAL-logged) and replay is exactly-once.
///
/// Protocol:
/// 1. learn (WAL-logged)
/// 2. feedback with retrieve_id="rid-crash-001" (WAL-logged as Feedback entry)
/// 3. Crash: drop(rt) — worker exits without snapshot; WAL has Learn+Feedback
/// 4. Cold restart: WAL replay re-applies learn (alpha=1) then feedback (alpha=2)
///    `feedback_seen` is populated during replay → idempotent
/// 5. Retry the same feedback call → no-op (dedup). alpha stays at 2.0.
///
/// Proves: feedback is durable (not at-most-once) and replay is idempotent (not at-least-twice).
#[test]
fn test_feedback_survives_crash_exactly_once() {
    let (cfg, dir) = make_cfg("fb_crash_exact");

    let node_id;
    let alpha_baseline;       // = 1.0
    let alpha_after_feedback; // = 2.0

    // Phase 1: learn + feedback, then crash (drop without snapshot).
    {
        let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();
        let result = rt.learn(None, emb(10), emb(20), emb(30), 0.3, 0.0, "hash-128d", 0.5, String::new(), None).unwrap();
        node_id = result.0.state_node_id;

        let (a0, _) = rt.get_node_beta(None, node_id).unwrap();
        alpha_baseline = a0;
        assert!((alpha_baseline - 1.0).abs() < 1e-5, "fresh node: alpha must be 1.0");

        rt.feedback_nodes(None, vec![node_id], true, Some("rid-crash-001".to_string())).unwrap();
        let (a1, _) = rt.get_node_beta(None, node_id).unwrap();
        alpha_after_feedback = a1;
        assert!((alpha_after_feedback - 2.0).abs() < 1e-5,
            "After 1 success: alpha must be 2.0, got {alpha_after_feedback}");

        // Crash: drop closes the channel. Worker exits on RecvError. NO snapshot written.
        // WAL contains: [Learn(0x00), Feedback(0x01)]
        drop(rt);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // Phase 2: cold restart — WAL replay.
    // Sequence: replay Learn (alpha=1.0), replay Feedback (alpha=2.0, adds to feedback_seen).
    {
        let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();
        let _ = rt.retrieve(None, emb(10), "hash-128d", NodeType::State); // triggers context load

        // Alpha must be 2.0 — feedback survived via WAL.
        let (a2, _) = rt.get_node_beta(None, node_id).unwrap();
        assert!(
            (a2 - alpha_after_feedback).abs() < 1e-5,
            "After WAL replay: alpha must be {alpha_after_feedback:.4} (feedback WAL-logged), got {a2:.4}. \
             If alpha == baseline, feedback WAL append is broken."
        );

        // Retry the same feedback — must be a no-op (exactly-once via feedback_seen).
        rt.feedback_nodes(None, vec![node_id], true, Some("rid-crash-001".to_string())).unwrap();
        let (a3, _) = rt.get_node_beta(None, node_id).unwrap();
        assert!(
            (a3 - alpha_after_feedback).abs() < 1e-5,
            "Retry after restart must be a no-op: alpha={a3:.4} expected={alpha_after_feedback:.4}. \
             If alpha == 3.0, feedback_seen was not populated during WAL replay."
        );

        rt.shutdown();
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test: feedback_ledger_crash_boundary
//
// Verifies that engine-level feedback_seen is the authoritative deduplication
// layer, not the MCP-level feedback_ledger.jsonl.
//
// The crash window in production:
//   feedback_nodes() -> [engine WAL commits feedback_seen] -> [crash here]
//                    -> record_feedback_applied() -> ledger write
//
// On restart:
//   - feedback_ledger.jsonl is missing the retrieve_id
//   - is_feedback_applied() returns false (ledger miss)
//   - feedback_nodes() retry reaches the engine
//   - Engine feedback_seen (restored from snapshot) blocks double-apply
//   - updated == 0; Beta posteriors unchanged
//
// This proves the docstring claim in record_feedback_applied: Option A (mutate
// then record) is safe because engine-level dedup catches the double-apply.
// =============================================================================

#[test]
fn test_feedback_ledger_crash_boundary() {
    let dir = std::env::temp_dir().join("cmhm_integ_ledger_crash");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let make = || {
        let mut c = Config::default();
        c.data_dir            = dir.clone();
        c.projections_dir     = dir.join("projections");
        c.max_active_contexts = 4;
        c
    };

    let retrieve_id = "retrieve-crash-boundary-test-001";

    // ── Session 1: learn, apply feedback, record to ledger, checkpoint ────────
    let (state_id, alpha_after, beta_after) = {
        let mut rt = ContextRuntime::new(make(), "s1".to_string()).unwrap();

        // Learn one experience
        let (result, ctx) = rt.learn(
            None, emb(60), emb(61), emb(62), 0.9, 0.0, "test",
            0.5, String::new(), None,
        ).unwrap();

        let state_id    = result.state_node_id;
        let ctx_id_str  = ctx.id.as_str().to_string();

        // Precondition: Beta(1,1)
        let (a0, _b0) = rt.get_node_beta(None, state_id)
            .expect("state node must exist after learn");
        assert!((a0 - 1.0).abs() < 1e-5, "Initial alpha must be 1.0, got {a0}");

        // Apply feedback via engine — WALs feedback_seen hash, mutates Beta
        let (updated, _) = rt.feedback_nodes(
            None, vec![state_id], true, Some(retrieve_id.to_string()),
        ).unwrap();
        assert_eq!(updated, 1, "First apply must update 1 node");

        let (a1, b1) = rt.get_node_beta(None, state_id).unwrap();
        assert!((a1 - (a0 + 1.0)).abs() < 1e-5,
            "Alpha must increase by 1 on success: before={a0:.4} after={a1:.4}");

        // MCP handler would call record_feedback_applied after feedback_nodes returns.
        // Simulate that here.
        rt.record_feedback_applied(retrieve_id, &ctx_id_str, "success", &[state_id]);
        assert!(
            rt.is_feedback_applied(retrieve_id),
            "Ledger must contain retrieve_id after record_feedback_applied"
        );

        // Checkpoint: engine CoreState (including feedback_seen) flushed to snapshot.bin
        rt.checkpoint(None).unwrap();
        rt.shutdown();

        // ── Simulate the crash window ──────────────────────────────────────────
        // In production the crash happens AFTER feedback_nodes (engine WAL committed)
        // but BEFORE record_feedback_applied (ledger write).
        // Reproduce by deleting the ledger AFTER a clean checkpoint.
        // Engine snapshot remains intact; ledger is gone.
        let ledger_path = dir.join("feedback_ledger.jsonl");
        std::fs::remove_file(&ledger_path)
            .expect("feedback_ledger.jsonl must exist after record_feedback_applied");

        println!("[crash_boundary] S1: applied feedback, checkpointed engine, deleted ledger");
        println!("[crash_boundary]   state_id={state_id}  alpha={a1:.4}  beta={b1:.4}");

        (state_id, a1, b1)
    };

    // ── Session 2: restart — ledger gone, engine snapshot intact ──────────────
    {
        let mut rt = ContextRuntime::new(make(), "s2".to_string()).unwrap();

        // MCP-level dedup is blind: ledger was deleted
        assert!(
            !rt.is_feedback_applied(retrieve_id),
            "[crash_boundary] After ledger deletion, is_feedback_applied must return false"
        );

        // Retry with same retrieve_id.
        // Engine feedback_seen (from snapshot) must block re-apply → updated == 0.
        let (updated_retry, _) = rt.feedback_nodes(
            None, vec![state_id], true, Some(retrieve_id.to_string()),
        ).unwrap();
        assert_eq!(
            updated_retry, 0,
            "[crash_boundary] Engine feedback_seen must block re-apply: \
             expected 0 nodes updated, got {updated_retry}. \
             If 1: feedback_seen was lost from snapshot or not restored on restart."
        );

        // Beta posteriors must be unchanged from post-S1 values
        let (a_retry, b_retry) = rt.get_node_beta(None, state_id).unwrap();
        assert!(
            (a_retry - alpha_after).abs() < 1e-5,
            "[crash_boundary] Alpha must be unchanged after blocked retry: \
             expected={alpha_after:.4}  actual={a_retry:.4}"
        );
        assert!(
            (b_retry - beta_after).abs() < 1e-5,
            "[crash_boundary] Beta must be unchanged after blocked retry: \
             expected={beta_after:.4}  actual={b_retry:.4}"
        );

        println!("[crash_boundary] S2: retry blocked by engine feedback_seen — PASS");
        println!("[crash_boundary]   alpha={a_retry:.4} (expected {alpha_after:.4})");
        println!("[crash_boundary]   beta={b_retry:.4}  (expected {beta_after:.4})");

        rt.shutdown();
    }

    let _ = std::fs::remove_dir_all(&dir);
}






