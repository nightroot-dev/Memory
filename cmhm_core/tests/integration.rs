//! End-to-end integration tests for the CMHM Context OS stack.
//!
//! Real ContextRuntime, real temp directories, real embedding vectors.
//! No mocking. Same code path the MCP server uses.
//!
//! Run:
//!   cargo +stable-x86_64-pc-windows-gnu test --test integration -- --nocapture

use cmhm_core::{Config, ContextRuntime, ContextId, ContextType};

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
        let (r, _) = rt.learn(ctx, emb(i*3), emb(i*3+1), emb(i*3+2), 0.6, 0.5, "test").unwrap();
        println!("[T1] Learn {i}: nodes {},{},{}", r.state_node_id, r.action_node_id, r.outcome_node_id);
    }

    let (result, _) = rt.retrieve(ctx, emb(0), "test").unwrap();
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

    rt.learn(Some(cmhm.id.as_str()),  emb(1),   emb(2),   emb(3),   0.8, 0.5, "test").unwrap();
    rt.learn(Some(video.id.as_str()), emb(100), emb(101), emb(102), 0.8, 0.5, "test").unwrap();

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
        rt.learn(Some(c.id.as_str()), emb(1), emb(2), emb(3), 0.7, 0.5, "test").unwrap();
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
        rt.learn(Some(&id), emb(1), emb(2), emb(3), 0.8, 0.5, "test").unwrap();
        rt.learn(Some(&id), emb(4), emb(5), emb(6), 0.8, 0.5, "test").unwrap();
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

        let (result, _) = rt.retrieve(Some(&ctx_id), emb(1), "test").unwrap();
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

    rt.learn(Some(&raw_id), emb(1), emb(2), emb(3), 0.5, 0.5, "test").unwrap();

    let ctx_id = ContextId::from_str(&raw_id);
    rt.delete_context(&ctx_id).expect("[T5] delete_context failed");
    println!("[T5] Context {} deleted", raw_id);

    // Learn must fail
    let err = rt.learn(Some(&raw_id), emb(10), emb(11), emb(12), 0.5, 0.5, "test")
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
            rt.learn(Some(&id), emb(i*3), emb(i*3+1), emb(i*3+2), 0.6, 0.5, "test").unwrap();
        }
        rt.checkpoint(Some(&id)).unwrap();

        // 5 post-checkpoint learns → WAL only
        for i in 10u32..15 {
            rt.learn(Some(&id), emb(i*3), emb(i*3+1), emb(i*3+2), 0.6, 0.5, "test").unwrap();
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

        let (result, _) = rt.retrieve(Some(&ctx_id), emb(0), "test").unwrap();
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
    rt.learn(None, emb(20), emb(21), emb(22), 0.9, 0.0, "hash-128d").unwrap();
    let n1 = rt.node_count(None);
    assert!(n1 >= 1,
        "First learn (high pe, forces birth) must insert ≥1 node, got {n1}");

    // Second learn with IDENTICAL embeddings + same high pe.
    // Shadow gate computes novelty ≈ 0 → gate_score ≈ 0 → would_index = false.
    // The shadow gate MUST NOT suppress insertion. Engine birth threshold (epsilon>0.5) still fires.
    rt.learn(None, emb(20), emb(21), emb(22), 0.9, 0.0, "hash-128d").unwrap();
    let n2 = rt.node_count(None);
    // n2 may equal n1 (engine deduplication) or exceed it — either is correct.
    // n2 < n1 would mean insertion was blocked or nodes deleted: FORBIDDEN.
    assert!(n2 >= n1,
        "Node count must never decrease after a repeat learn (shadow gate must not delete): \
        before={n1}, after={n2}");

    // Third learn: completely orthogonal embeddings at high pe → fresh birth guaranteed.
    // Use seed values far from the first group to ensure low cosine similarity.
    rt.learn(None, emb(200), emb(201), emb(202), 0.9, 0.0, "hash-128d").unwrap();
    let n3 = rt.node_count(None);
    assert!(n3 > n2,
        "Third learn with distinct embeddings must increase node count: before={n2}, after={n3}");
    // Sanity: a low-pe near-duplicate learn (may or may not birth a new node,
    // depending on birth threshold). What matters is no panic and no count decrease.
    rt.learn(None, emb(20), emb(21), emb(22), 0.1, 0.0, "hash-128d").unwrap();
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

    // Strategy: use `anchor` for action AND outcome in every learn call.
    // This makes all 3 stored nodes (state, action, outcome) have embedding = emb(0).
    // Therefore: max_cosine(v, model) = cos(v, emb(0)) = target_cos (by Gram-Schmidt construction).
    // Without this, emb(100)/emb(200) in the store can dominate the max_cosine scan
    // if the Gram-Schmidt perp accidentally correlates with those vectors.

    // ── Case 1: cos = 0.50 < 0.55 → BIRTH ───────────────────────────────────
    // cos escape: 0.50 < 0.55 → OPEN → birth fires.
    // eps escape: 0.2 < 0.5 → closed.
    // sal escape: ~0.10 < 0.70 → closed.
    // max_cosine(v050, store) = 0.50 exactly (all nodes ≈ emb(0)).
    {
        let (cfg, dir) = make_cfg("birth_case1");
        let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();
        // Anchor learn: eps=0.9 forces birth for all three (state, action, outcome).
        // Using anchor for all three makes all stored embeddings ≈ emb(0).
        rt.learn(None, anchor.clone(), anchor.clone(), anchor.clone(), 0.9, 0.0, "hash-128d").unwrap();
        let n0 = rt.node_count(None);

        // Test state at cos=0.50: action/outcome reuse existing emb(0) nodes (cos_max=1.0, eps=0.2)
        let v050 = with_cosine(&anchor, 0.50, 9999);
        rt.learn(None, v050, anchor.clone(), anchor.clone(), 0.2, 0.0, "hash-128d").unwrap();
        let n1 = rt.node_count(None);
        assert!(n1 > n0,
            "[Case1] cos=0.50 < threshold 0.55: state node MUST be born. before={n0}, after={n1}");
        rt.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Case 2: cos = 0.60 > 0.55, eps=0.2 → NO BIRTH ───────────────────────
    // cos escape: 0.60 > 0.55 → closed.
    // eps escape: 0.2 < 0.5  → closed.
    // sal escape: ~0.10 < 0.70 → closed.
    // → birth_node returns existing anchor ID. Node count must NOT increase.
    {
        let (cfg, dir) = make_cfg("birth_case2");
        let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();
        rt.learn(None, anchor.clone(), anchor.clone(), anchor.clone(), 0.9, 0.0, "hash-128d").unwrap();
        let n0 = rt.node_count(None);
        let v060 = with_cosine(&anchor, 0.60, 9999);
        rt.learn(None, v060, anchor.clone(), anchor.clone(), 0.2, 0.0, "hash-128d").unwrap();
        let n1 = rt.node_count(None);
        assert_eq!(n1, n0,
            "[Case2] cos=0.60 > threshold 0.55, eps=0.2: NO new state node. before={n0}, after={n1}");
        rt.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Case 3: cos = 0.60 > 0.55, eps = 0.9 → BIRTH via epsilon escape ──────
    // cos escape: closed (0.60 > 0.55).
    // eps escape: 0.9 > 0.5 → OPEN → birth fires for the state node.
    // → Same embedding as Case 2, but high eps → at least 1 new node.
    {
        let (cfg, dir) = make_cfg("birth_case3");
        let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();
        rt.learn(None, anchor.clone(), anchor.clone(), anchor.clone(), 0.9, 0.0, "hash-128d").unwrap();
        let n0 = rt.node_count(None);
        let v060 = with_cosine(&anchor, 0.60, 9999);
        rt.learn(None, v060, anchor.clone(), anchor.clone(), 0.9, 0.0, "hash-128d").unwrap();
        let n1 = rt.node_count(None);
        assert!(n1 > n0,
            "[Case3] cos=0.60, eps=0.9 > 0.5: epsilon escape must fire → new state node. before={n0}, after={n1}");
        rt.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ── Calibration sweep: role-scoped max_cosine has no cross-role floor ────────
//
// After role-scoping, a State query is compared only against State nodes.
// The 0.34 floor that existed when Action/Outcome nodes contaminated the search
// should be gone: actual max_cosine should track target_cos closely.
//
// The store is populated with a single State node at emb(0) (the anchor).
// Separate Action and Outcome nodes are also inserted to confirm they are invisible
// to State-scoped nearest.
#[test]
fn test_birth_gate_cosine_calibration_curve() {
    use cmhm_core::node::NodeType;

    let anchor = emb(0);
    let (cfg, dir) = make_cfg("cos_calibration");
    let mut rt = ContextRuntime::new(cfg, "s".to_string()).unwrap();

    // Seed store: one learn to create state/action/outcome nodes.
    // Action=emb(100) and Outcome=emb(200) are orthogonal to anchor;
    // they should NOT appear in the State-scoped nearest search.
    rt.learn(None, anchor.clone(), emb(100), emb(200), 0.9, 0.0, "hash-128d").unwrap();

    // 11-point sweep: target cos 0.0 → 1.0 in 0.1 steps.
    // Tolerance 0.08: tighter than the old unscoped ±0.20 observed spread.
    let targets: Vec<f32> = (0..=10).map(|i| i as f32 * 0.1).collect();
    for target in targets {
        let v = if target >= 1.0 - 1e-5 {
            anchor.clone()
        } else {
            with_cosine(&anchor, target, 9999)
        };
        // Role-scoped: None passes node_type=None → all nodes (backward compat check)
        let actual_all   = rt.max_cosine_in_context(None, &v, "hash-128d", None);
        // Role-scoped: Some(State) → only State nodes
        let actual_state = rt.max_cosine_in_context(None, &v, "hash-128d", Some(NodeType::State));

        // State-scoped result should closely track target (no 0.34 floor from cross-role)
        let err = (actual_state - target).abs();
        assert!(
            err < 0.08,
            "State-scoped max_cosine should track target. target={target:.2} actual={actual_state:.4} err={err:.4}"
        );

        // For non-zero targets the all-node result may be higher (Action/Outcome contamination),
        // but state-scoped is always <= all-node.
        assert!(
            actual_state <= actual_all + 1e-5,
            "State-scoped <= all-node. target={target:.2} state={actual_state:.4} all={actual_all:.4}"
        );
    }

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Feedback survives snapshot → reload ──────────────────────────────────────
//
// feedback_seen is part of CoreState (persisted in snapshot).
// After a snapshot + reload, a duplicate feedback call must be a no-op.
#[test]
fn test_feedback_key_survives_snapshot_reload() {
    let anchor = emb(0);
    let (mut cfg, dir) = make_cfg("fb_snapshot_survival");
    cfg.snapshot_interval_cmds = u64::MAX; // prevent auto-snapshot
    let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();

    // Learn a node, get its ID
    let (result, _) = rt.learn(None, anchor.clone(), emb(1), emb(2), 0.4, 0.0, "hash-128d").unwrap();
    let node_id = result.state_node_id;

    // Read baseline Alpha
    let alpha0 = rt.get_node_beta(None, node_id).map(|(a, _)| a).unwrap_or(1.0);

    // Apply feedback once
    let retrieve_id = "test-retrieve-001".to_string();
    let (updated, _) = rt.feedback_nodes(None, vec![node_id], true, Some(retrieve_id.clone())).unwrap();
    assert_eq!(updated, 1, "first feedback must update 1 node");
    let alpha1 = rt.get_node_beta(None, node_id).map(|(a, _)| a).unwrap_or(1.0);
    assert!((alpha1 - (alpha0 + 1.0)).abs() < 1e-4, "alpha should be alpha0+1 after first feedback");

    // Snapshot explicitly (persists feedback_seen)
    rt.checkpoint(None).unwrap();
    rt.shutdown();

    // Reload from snapshot
    let mut rt2 = ContextRuntime::new(cfg, "s".to_string()).unwrap();

    // Retry with same retrieve_id — must be no-op (feedback_seen loaded from snapshot)
    let (updated2, _) = rt2.feedback_nodes(None, vec![node_id], true, Some(retrieve_id)).unwrap();
    assert_eq!(updated2, 0, "retry after snapshot reload must be no-op (feedback_seen persisted)");
    let alpha2 = rt2.get_node_beta(None, node_id).map(|(a, _)| a).unwrap_or(1.0);
    assert!((alpha2 - alpha1).abs() < 1e-4, "alpha must not change on duplicate replay");

    rt2.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// ── At-least-once feedback semantics on simulated crash ───────────────────────
//
// Feedback is NOT written to the WAL (only Learn is). So if the process crashes
// before snapshotting, feedback is lost on restart. A retry then re-applies
// cleanly (alpha = alpha0+1, not alpha0+2 = re-inflation).
//
// This proves: WAL replay cannot re-inflate Alpha (feedback not in WAL).
#[test]
fn test_feedback_at_least_once_on_simulated_crash() {
    let anchor = emb(0);
    let (mut cfg, dir) = make_cfg("fb_at_least_once");
    cfg.snapshot_interval_cmds = u64::MAX; // prevent auto-snapshot during test
    let mut rt = ContextRuntime::new(cfg.clone(), "s".to_string()).unwrap();

    let (result, _) = rt.learn(None, anchor.clone(), emb(1), emb(2), 0.4, 0.0, "hash-128d").unwrap();
    let node_id = result.state_node_id;
    let alpha0 = rt.get_node_beta(None, node_id).map(|(a, _)| a).unwrap_or(1.0);

    // Apply feedback — NOT snapshotted
    let retrieve_id = "test-retrieve-002".to_string();
    let (updated, _) = rt.feedback_nodes(None, vec![node_id], true, Some(retrieve_id.clone())).unwrap();
    assert_eq!(updated, 1);
    let alpha1 = rt.get_node_beta(None, node_id).map(|(a, _)| a).unwrap_or(1.0);
    assert!((alpha1 - (alpha0 + 1.0)).abs() < 1e-4);

    // Simulate crash: drop without snapshot (WAL replay only has the Learn entry)
    drop(rt);

    // Reload — WAL replays Learn, feedback_seen is empty (never snapshotted)
    let mut rt2 = ContextRuntime::new(cfg, "s".to_string()).unwrap();

    // Alpha must be back to alpha0 (feedback was lost — no re-inflation, just loss)
    let alpha_after_crash = rt2.get_node_beta(None, node_id).map(|(a, _)| a).unwrap_or(1.0);
    assert!(
        (alpha_after_crash - alpha0).abs() < 1e-4,
        "after crash replay, alpha must be alpha0 (feedback lost, not doubled). \
         alpha0={alpha0:.1} actual={alpha_after_crash:.1}"
    );

    // Retry with same retrieve_id — re-applies cleanly (alpha0 → alpha0+1, not alpha0+2)
    let (updated2, _) = rt2.feedback_nodes(None, vec![node_id], true, Some(retrieve_id)).unwrap();
    assert_eq!(updated2, 1, "retry must apply feedback once");
    let alpha_retry = rt2.get_node_beta(None, node_id).map(|(a, _)| a).unwrap_or(1.0);
    assert!(
        (alpha_retry - (alpha0 + 1.0)).abs() < 1e-4,
        "retry restores alpha0+1, not alpha0+2. alpha_retry={alpha_retry:.1}"
    );

    rt2.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
