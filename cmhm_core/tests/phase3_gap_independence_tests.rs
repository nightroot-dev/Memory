//! Protocol v1.9 Section 14.3 & 14.7 — Conflict Detection Gap-Independence Unit Tests
//!
//! These tests verify that conflict detection and classification are strictly
//! independent of score gap (score proximity) under hash-128d, following the deletion
//! of the outer gap gate in Protocol v1.9.
//!
//! Specifically, two nodes that satisfy a conflict signal condition (such as identical
//! assertion_key with opposing polarities, or ProvenancePair) must produce the exact
//! same ConflictType and abstention decision regardless of whether the score gap is
//! tiny (~0.001) or large (> 0.40).

use cmhm_core::{
    Config, ContextRuntime, ContextType, ConflictType,
    CallerRole,
};
use std::sync::atomic::{AtomicU32, Ordering};

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

static COUNTER: AtomicU32 = AtomicU32::new(100);

fn make_rt(label: &str) -> (ContextRuntime, std::path::PathBuf) {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cmhm_gap_test_{label}_{id:06}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = dir.clone();
    cfg.projections_dir = dir.join("projections");
    // Explicitly verify production defaults:
    assert!(cfg.abstain_on_conflict, "Config::default().abstain_on_conflict must be true in v1.9");
    assert_eq!(cfg.conflict_detection_threshold, 0.0, "Config::default().conflict_detection_threshold must be 0.0 in v1.9");
    let rt = ContextRuntime::new(cfg, format!("gap-{label}")).expect("runtime init");
    (rt, dir)
}

fn make_ctx(rt: &mut ContextRuntime, name: &str) -> String {
    rt.create_context(name, ContextType::default(), None, "", vec![])
        .expect("create context")
        .id
        .to_string()
}

fn learn_node(
    rt: &mut ContextRuntime,
    ctx_id: &str,
    embedding: Vec<f32>,
    authority: f32,
    provenance: &str,
    assertion_key: Option<&str>,
    polarity: Option<i8>,
) -> u64 {
    let (result, _) = rt.learn_with_context(
        Some(ctx_id),
        embedding.clone(),
        emb(9999),
        emb(9998),
        0.9, 0.5,
        "hash-128d",
        authority,
        provenance.to_string(),
        None,
        CallerRole::User,
        assertion_key.map(|s| s.to_string()),
        polarity,
    ).expect("learn_with_context");
    result.state_node_id
}

#[test]
fn test_logical_contradiction_gap_independence() {
    // Test Case A: Small score gap (gap = 0.000)
    // Both nodes share identical embedding and authority, opposing polarities on same assertion_key
    {
        let (mut rt, _dir) = make_rt("lc_small");
        let ctx = make_ctx(&mut rt, "lc_small");

        let base_emb = emb(10);
        let _n1 = learn_node(&mut rt, &ctx, base_emb.clone(), 0.85, "user_explicit", Some("port_cfg"), Some(1));
        let _n2 = learn_node(&mut rt, &ctx, base_emb.clone(), 0.85, "user_explicit", Some("port_cfg"), Some(-1));

        let (result, _) = rt.retrieve_unscoped(Some(&ctx), base_emb, "hash-128d")
            .expect("retrieve");

        assert!(result.conflict_detected, "Conflict must be detected at tiny gap");
        assert!(result.abstained, "Engine must abstain on LogicalContradiction");
        assert_eq!(result.conflict_candidates.len(), 2);
        assert_eq!(result.conflict_candidates[0].conflict_type, ConflictType::LogicalContradiction);

        let gap = (result.scored_candidates[0].1 - result.scored_candidates[1].1).abs();
        assert!(gap < 0.01, "Score gap should be near zero, got {gap}");
        println!("[GAP-TEST PASS] LogicalContradiction at small gap ({gap:.4}) detected and abstained");
    }

    // Test Case B: Large score gap (> 0.40)
    // Both nodes are in top-2 (sharing query embedding), but Node 1 has auth 1.00 and Node 2 has auth 0.35
    // Score gap is 0.8 * (1.00 - 0.35) = 0.52 > 0.40 >> 0.02
    {
        let (mut rt, _dir) = make_rt("lc_large");
        let ctx = make_ctx(&mut rt, "lc_large");

        let q_emb = emb(20);
        let _n1 = learn_node(&mut rt, &ctx, q_emb.clone(), 1.00, "user_explicit", Some("port_cfg"), Some(1));
        let _n2 = learn_node(&mut rt, &ctx, q_emb.clone(), 0.35, "user_explicit", Some("port_cfg"), Some(-1));

        let (result, _) = rt.retrieve_unscoped(Some(&ctx), q_emb, "hash-128d")
            .expect("retrieve");

        assert!(result.conflict_detected, "Conflict must be detected at large gap");
        assert!(result.abstained, "Engine must abstain on LogicalContradiction even with large gap");
        assert_eq!(result.conflict_candidates.len(), 2);
        assert_eq!(result.conflict_candidates[0].conflict_type, ConflictType::LogicalContradiction);

        let gap = (result.scored_candidates[0].1 - result.scored_candidates[1].1).abs();
        assert!(gap > 0.40, "Score gap should be large (>0.40), got {gap}");
        println!("[GAP-TEST PASS] LogicalContradiction at large gap ({gap:.4}) detected and abstained");
    }
}

#[test]
fn test_provenance_pair_and_trust_compromised_gap_independence() {
    // SafeAbstention: auth gap <= 0.10 -> TrustCompromised triggers abstention
    // Node 1 (user_explicit, auth 0.35), Node 2 (contradicted, auth clamped to 0.30)
    // |0.35 - 0.30| = 0.05 <= 0.10 -> TrustCompromised fires -> abstained = true
    {
        let (mut rt, _dir) = make_rt("safe_abs");
        let ctx = make_ctx(&mut rt, "safe_abs");

        let q_emb = emb(40);
        let _n1 = learn_node(&mut rt, &ctx, q_emb.clone(), 0.35, "user_explicit", None, None);
        let _n2 = learn_node(&mut rt, &ctx, q_emb.clone(), 0.30, "contradicted", None, None);

        let (result, _) = rt.retrieve_unscoped(Some(&ctx), q_emb, "hash-128d")
            .expect("retrieve");

        assert!(result.conflict_detected, "Conflict must be detected");
        assert_eq!(result.conflict_candidates[0].conflict_type, ConflictType::AuthorityPolarity);
        assert!(result.abstained, "TrustCompromised must trigger abstained=true when auth gap <= 0.10");

        let gap = (result.scored_candidates[0].1 - result.scored_candidates[1].1).abs();
        println!("[GAP-TEST PASS] SafeAbstention at gap ({gap:.4}) correctly classified AuthorityPolarity and abstained");
    }

    // AuthorityPolarity (typical): auth gap > 0.10 -> conflict_detected=true, AuthorityPolarity, but abstained=false
    // Node 1 (user_explicit, auth 0.85), Node 2 (contradicted, auth clamped to 0.30)
    // |0.85 - 0.30| = 0.55 > 0.10 -> TrustCompromised does NOT fire -> abstained = false, winner returned
    {
        let (mut rt, _dir) = make_rt("ap_typical");
        let ctx = make_ctx(&mut rt, "ap_typical");

        let q_emb = emb(50);
        let n1 = learn_node(&mut rt, &ctx, q_emb.clone(), 0.85, "user_explicit", None, None);
        let _n2 = learn_node(&mut rt, &ctx, q_emb.clone(), 0.30, "contradicted", None, None);

        let (result, _) = rt.retrieve_unscoped(Some(&ctx), q_emb, "hash-128d")
            .expect("retrieve");

        assert!(result.conflict_detected, "Conflict must be detected");
        assert_eq!(result.conflict_candidates[0].conflict_type, ConflictType::AuthorityPolarity);
        assert!(!result.abstained, "AuthorityPolarity with gap > 0.10 must NOT abstain (winner returned)");
        assert_eq!(result.seed_id, Some(n1), "Winner must be the higher-authority node");

        let gap = (result.scored_candidates[0].1 - result.scored_candidates[1].1).abs();
        assert!(gap > 0.40, "Score gap should reflect auth margin: {gap}");
        println!("[GAP-TEST PASS] Typical AuthorityPolarity at gap ({gap:.4}) classified AuthorityPolarity without abstaining");
    }
}

#[test]
fn test_adversarial_pair_gap_independence() {
    // AdversarialAuthority: both HIGH_TRUST, corrected vs user_explicit with user_explicit having higher authority
    let (mut rt, _dir) = make_rt("aa_test");
    let ctx = make_ctx(&mut rt, "aa_test");

    let q_emb = emb(60);
    // Corrected fact has authority 0.80
    let _n_pos = learn_node(&mut rt, &ctx, q_emb.clone(), 0.80, "corrected", None, None);
    // Adversarial entry has authority 0.95 (inversion: user_explicit > corrected)
    let _n_neg = learn_node(&mut rt, &ctx, q_emb.clone(), 0.95, "user_explicit", None, None);

    let (result, _) = rt.retrieve_unscoped(Some(&ctx), q_emb, "hash-128d")
        .expect("retrieve");

    assert!(result.conflict_detected, "AdversarialPair conflict must be detected");
    assert_eq!(result.conflict_candidates[0].conflict_type, ConflictType::AuthorityPolarity);
    assert!(result.abstained, "AdversarialPair must trigger abstention");

    let gap = (result.scored_candidates[0].1 - result.scored_candidates[1].1).abs();
    println!("[GAP-TEST PASS] AdversarialPair at gap ({gap:.4}) detected AuthorityPolarity and abstained");
}