//! Protocol v2.0 Section 14.7 — Pool-Based Detection and ProvenancePair Gap-Independence Tests
//!
//! Verifies the v2.0 amended contract:
//! - Detection operates on the post-filter pool (Step 2b), BEFORE composite scoring.
//! - Contradicted D- at scored rank 6 is still detected (pool-present even if score-suppressed).
//! - ProvenancePair fires unconditionally — abstained=true regardless of |auth_a - auth_b|.

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
    // Verify production defaults (v2.0 unchanged from v1.9):
    assert!(cfg.abstain_on_conflict, "Config::default().abstain_on_conflict must be true");
    assert_eq!(cfg.conflict_detection_threshold, 0.0, "Config::default().conflict_detection_threshold must be 0.0");
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
    // Test Case A: Small score gap (gap ≈ 0.000)
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
    // Node 1 high authority, Node 2 low — LC still fires because pool-based detection
    // sees both nodes before scoring suppresses D- by authority weight.
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

// ---------------------------------------------------------------------------
// v2.0 §14.7 item 2: Pool-based detection — contradicted D- at SCORED rank 6
//   still triggers ProvenancePair → abstained=true
//   (Previously required D- in scored top-2; v2.0 requires D- in post-filter pool.)
// ---------------------------------------------------------------------------
#[test]
fn test_pool_detection_fires_for_rank6_contradicted() {
    let (mut rt, _dir) = make_rt("pool_rank6");
    let ctx = make_ctx(&mut rt, "pool_rank6");

    // Build a pool where one contradicted node is buried by 5 high-authority nodes.
    // The contradicted node will be at scored rank 6 but is present in the post-filter pool.
    // Under v2.0 pool-based detection, ProvenancePair fires → abstained=true.
    // Under v1.9 scored-top-2 detection, ProvenancePair would NOT fire → abstained=false.
    let q_emb = emb(77);

    // 5 high-authority user_explicit nodes → push contradicted to rank 6 in scoring
    for i in 0..5u32 {
        let auth = 0.90 - (i as f32) * 0.01; // 0.90, 0.89, 0.88, 0.87, 0.86
        learn_node(&mut rt, &ctx, q_emb.clone(), auth, "user_explicit", None, None);
    }
    // The contradicted node: authority clamped to <= 0.30 by authority.rs.
    // Its composite score = 0.7*1.0 + 0.3*cos + 0.8*<=0.30 ≈ 0.70 + 0.24 + 0.24 ≈ 1.18
    // vs top user_explicit: 0.7*1.0 + 0.3*cos + 0.8*0.90 ≈ 0.70 + 0.24 + 0.72 = 1.66
    // → contradicted lands at rank 6 in scoring.
    let neg_id = learn_node(&mut rt, &ctx, q_emb.clone(), 0.99, "contradicted", None, None);

    let (result, _) = rt.retrieve_unscoped(Some(&ctx), q_emb, "hash-128d")
        .expect("retrieve");

    // G10 invariant: D- must be present in raw_nn
    assert!(result.raw_nn_candidates.contains(&neg_id),
        "D- must appear in raw_nn");

    // Core assertion: pool-based detection must fire despite D- at scored rank 6
    assert!(result.conflict_detected,
        "ProvenancePair must fire: contradicted node in pool (pool-based detection)");
    assert!(result.abstained,
        "Engine must abstain: HIGH_TRUST + LOW_TRUST pair detected in pool");
    assert_eq!(result.conflict_candidates.len(), 2,
        "conflict_candidates must contain exactly the 2 competing nodes");
    assert_eq!(result.conflict_candidates[0].conflict_type, ConflictType::AuthorityPolarity,
        "conflict_type must be AuthorityPolarity (ProvenancePair signal)");

    // Confirm D- was NOT in scored top-2 (this is the scenario v1.9 detection missed)
    let contradicted_rank = result.scored_candidates.iter()
        .position(|(id, _)| *id == neg_id);
    if let Some(rank) = contradicted_rank {
        println!("[POOL-TEST PASS] Contradicted D- at scored rank {} — pool-based detection fired correctly", rank + 1);
        assert!(rank >= 2,
            "Contradicted D- should be suppressed past rank 2 by authority weight; actual rank {}", rank + 1);
    } else {
        panic!("Contradicted D- not found in scored_candidates");
    }
}

// ---------------------------------------------------------------------------
// v2.0 §14.7 item 3: TrustCompromised-removed — ProvenancePair fires and
//   produces abstained=true regardless of abs(auth_a - auth_b).
//   Test at gap=0.01 and gap=0.80.
// ---------------------------------------------------------------------------
#[test]
fn test_provenance_pair_unconditional_abstention() {
    // Case A: Small auth gap (≈ 0.01) — previously covered by TrustCompromised path, still abstains.
    {
        let (mut rt, _dir) = make_rt("pp_small_gap");
        let ctx = make_ctx(&mut rt, "pp_small_gap");

        let q_emb = emb(40);
        // user_explicit auth 0.31; contradicted auth clamped ≤ 0.30 → gap ≈ 0.01
        learn_node(&mut rt, &ctx, q_emb.clone(), 0.31, "user_explicit", None, None);
        learn_node(&mut rt, &ctx, q_emb.clone(), 0.99, "contradicted", None, None);

        let (result, _) = rt.retrieve_unscoped(Some(&ctx), q_emb, "hash-128d")
            .expect("retrieve");

        assert!(result.conflict_detected, "ProvenancePair must fire (small auth gap)");
        assert_eq!(result.conflict_candidates.len(), 2);
        assert_eq!(result.conflict_candidates[0].conflict_type, ConflictType::AuthorityPolarity);
        assert!(result.abstained,
            "ProvenancePair must abstain unconditionally — TrustCompromised gap check is DELETED (v2.0)");

        println!("[PP-TEST PASS] ProvenancePair abstains at small auth gap");
    }

    // Case B: Large auth gap (≈ 0.80) — previously NOT abstaining (TrustCompromised gap 0.55 > 0.10).
    //   Under v2.0, ProvenancePair fires unconditionally → still abstains.
    {
        let (mut rt, _dir) = make_rt("pp_large_gap");
        let ctx = make_ctx(&mut rt, "pp_large_gap");

        let q_emb = emb(50);
        // user_explicit auth 0.90; contradicted clamped ≤ 0.30 → effective gap ≥ 0.60
        learn_node(&mut rt, &ctx, q_emb.clone(), 0.90, "user_explicit", None, None);
        learn_node(&mut rt, &ctx, q_emb.clone(), 0.99, "contradicted", None, None);

        let (result, _) = rt.retrieve_unscoped(Some(&ctx), q_emb, "hash-128d")
            .expect("retrieve");

        assert!(result.conflict_detected, "ProvenancePair must fire (large auth gap)");
        assert_eq!(result.conflict_candidates.len(), 2);
        assert_eq!(result.conflict_candidates[0].conflict_type, ConflictType::AuthorityPolarity);
        assert!(result.abstained,
            "ProvenancePair must abstain unconditionally — gap 0.80 does NOT exempt (v2.0)");

        let auth_gap = (result.conflict_candidates[0].authority - result.conflict_candidates[1].authority).abs();
        assert!(auth_gap > 0.40,
            "Auth gap should reflect clamp vs user_explicit: {auth_gap:.3}");
        println!("[PP-TEST PASS] ProvenancePair abstains at large auth gap ({auth_gap:.3}) — TrustCompromised correctly absent");
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
    assert_eq!(result.conflict_candidates.len(), 2);
    assert_eq!(result.conflict_candidates[0].conflict_type, ConflictType::AuthorityPolarity);
    assert!(result.abstained, "AdversarialPair must trigger abstention");

    let gap = (result.scored_candidates[0].1 - result.scored_candidates[1].1).abs();
    println!("[GAP-TEST PASS] AdversarialPair at gap ({gap:.4}) detected AuthorityPolarity and abstained");
}
