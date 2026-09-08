//! Phase 3 Approval Benchmark â€” Role A scaffolding (frozen at bc66340)
//!
//! AUTHORING BOUNDARY (Protocol v1.8 Section 4):
//!   Role A authored this file: all types, the runner harness, scoring logic,
//!   distractor generation, pool-stage extraction, gate metric computation,
//!   the PHASE3_RESULT|...| structured output formatter, and the
//!   phase3_conformance_check test (Stage 1.5 validator).
//!
//!   Role B fills in DATA ONLY: the `SCENARIOS` static slice below.
//!   ZERO structural changes, type changes, harness changes, or scoring changes
//!   are permitted after Role B takes custody.
//!
//! PROHIBITED CORPUS GUARD:
//!   This file must never contain any query, positive text, or negative text
//!   derived from `dprime_bench.rs` (15-query rejection set) or
//!   `phase3_held_out_bench.rs` (32-query compromised set).
//!   The overlap auditor (`scripts/audit_corpus_overlap.ps1`) enforces this
//!   automatically at Stage 1. Any violation fails closed and terminates
//!   the attempt.
//!
//! STAGE 1.5 CONFORMANCE CHECK:
//!   `phase3_conformance_check` must pass before Stage 2 may execute.
//!   It validates every scenario in SCENARIOS against the Section 4.4 matrix.
//!   Failures terminate the attempt before the single run is consumed.
//!
//! EXECUTION CONSTRAINT:
//!   This benchmark consumes the single approval attempt when Stage 2 launches.
//!   Do not run `scripts/verify_phase3_approval.ps1` without `-SyntheticOnly`
//!   until the conformance check and overlap audit have both passed.

use std::sync::atomic::{AtomicU32, Ordering};
use tempfile::TempDir;

use cmhm_core::{Config, ContextRuntime, ContextType, ConflictType, CallerRole};

// â”€â”€ Scenario types (frozen â€” Role B must not modify) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioClass {
    Contradiction,
    Supersession,
    AuthorityPolarity,
    AdversarialAuthority,
    SafeAbstention,
    UnresolvedConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedConflict {
    LogicalContradiction,
    AuthorityPolarity,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedWinner {
    Pos,
    Neg,
    None,
}

pub struct ApprovalScenario {
    /// Unique scenario ID (1-based, sequential).
    pub id: usize,
    /// Operational domain label (see Section 4.2 domain table).
    pub domain: &'static str,
    /// Scenario class (see Section 4.4 matrix).
    pub class: ScenarioClass,
    /// The retrieval query text (Role B-authored; must not overlap prohibited corpora).
    pub query: &'static str,
    /// Positive candidate text (the "correct" / preferred memory entry).
    pub pos_text: &'static str,
    pub pos_auth: f32,
    pub pos_prov: &'static str,
    pub pos_key: Option<&'static str>,
    pub pos_polarity: Option<i8>,
    /// Hard-negative candidate text (the "dangerous" conflicting entry).
    pub neg_text: &'static str,
    pub neg_auth: f32,
    pub neg_prov: &'static str,
    pub neg_key: Option<&'static str>,
    pub neg_polarity: Option<i8>,
    /// If true, D+ supersedes Dâˆ’ (SUPERSESSION class only).
    pub supersedes_neg: bool,
    /// Expected conflict classification from the engine.
    pub expected_conflict: ExpectedConflict,
    /// If true, the engine must abstain (winner = None).
    pub expected_abstain: bool,
    /// Which candidate should win if not abstained.
    pub expected_winner: ExpectedWinner,
}

// â”€â”€ ROLE B: POPULATE THIS SLICE ONLY. MINIMUM 40 SCENARIOS. â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Zero structural or harness logic changes are permitted.
// Follow Section 4 of Protocol v1.8 strictly.
//
// CORPUS v2 PLACEHOLDER â€” attempt-1 corpus removed (commit e989a91, invalid on its face).
// Role B authors fresh scenarios per Protocol v1.8 Section 4.4 and Section 13.
// Role A transcribes mechanically and commits; phase3_conformance_check (Stage 1.5)
// validates conformance before Stage 2 is permitted to execute.
pub static SCENARIOS: &[ApprovalScenario] = &[

    // â”€â”€ Role B populates here. â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

];


// â”€â”€ Distractor RNG seed (frozen by protocol) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
const DISTRACTOR_RNG_SEED: u64 = 0xA3F7_2B91_C5D4_E608;
const MIN_DISTRACTORS: usize = 60;

// â”€â”€ Runtime counter for isolated temp dirs â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
static COUNTER: AtomicU32 = AtomicU32::new(0);

// â”€â”€ Harness helpers (frozen â€” Role B must not modify) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn make_runtime_pair() -> (ContextRuntime, TempDir) {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = TempDir::new().expect("tempdir");
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = tmp.path().join("projections");
    let rt = ContextRuntime::new(cfg, format!("bench-{id}")).expect("runtime init");
    (rt, tmp)
}

/// Deterministic LCG embedding from a u64 seed (128-dimensional, unit-normed).
fn lcg_emb(seed: u64) -> Vec<f32> {
    let mut v = vec![0.0f32; 128];
    let mut x = seed;
    for f in v.iter_mut() {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *f = ((x >> 33) as f32) / (u32::MAX as f32) - 0.5;
    }
    let norm = v.iter().map(|f| f * f).sum::<f32>().sqrt().max(1e-9);
    v.iter_mut().for_each(|f| *f /= norm);
    v
}

/// Learn a node using text_to_embedding (the production path under test).
fn learn_text(
    rt: &mut ContextRuntime,
    ctx_id: &str,
    text: &str,
    auth: f32,
    prov: &str,
    supersedes: Option<u64>,
    key: Option<&str>,
    polarity: Option<i8>,
) -> u64 {
    use cmhm_core::text_to_embedding;
    let emb = text_to_embedding(text).to_vec();
    let (res, _) = rt.learn_with_context(
        Some(ctx_id),
        emb.clone(), emb.clone(), emb.clone(),
        0.5, 0.5,
        "hash-128d",
        auth,
        prov.to_string(),
        supersedes,
        CallerRole::User,
        key.map(|s| s.to_string()),
        polarity,
    ).expect("learn_text");
    res.state_node_id
}

/// Learn a background distractor node (LCG embedding, low authority).
fn learn_distractor(rt: &mut ContextRuntime, ctx_id: &str, seed: u64) -> u64 {
    let emb = lcg_emb(seed);
    let (res, _) = rt.learn_with_context(
        Some(ctx_id),
        emb.clone(), emb.clone(), emb.clone(),
        0.1, 0.1,
        "hash-128d",
        0.30,
        "distractor".to_string(),
        None,
        CallerRole::User,
        None,
        None,
    ).expect("learn_distractor");
    res.state_node_id
}

// â”€â”€ Per-scenario result (frozen) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

struct ScenarioResult {
    id: usize,
    domain: &'static str,
    class: ScenarioClass,
    // Pool stage membership
    pos_in_raw_nn:       bool,
    neg_in_raw_nn:       bool,
    pos_in_post_filter:  bool,
    neg_in_post_filter:  bool,
    pos_rank_scored:     Option<usize>,  // 1-based; None if absent
    neg_rank_scored:     Option<usize>,
    // Composite scores
    pos_score:           Option<f32>,
    neg_score:           Option<f32>,
    // Engine outputs
    abstained:           bool,
    winner_id:           Option<u64>,
    conflict_type:       Option<ConflictType>,
    // Expected
    expected_conflict:   ExpectedConflict,
    expected_abstain:    bool,
    expected_winner:     ExpectedWinner,
    pos_id:              u64,
    neg_id:              u64,
}

// â”€â”€ 5-way outcome categorisation (frozen) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Default)]
struct OutcomeCounts {
    selected_d_plus:       usize,
    selected_d_minus:      usize,
    selected_other:        usize,
    correct_abstention:    usize,
    incorrect_abstention:  usize,
}

fn categorise(r: &ScenarioResult) -> &'static str {
    if r.abstained {
        if r.expected_abstain { "correct_abstention" } else { "incorrect_abstention" }
    } else {
        match r.winner_id {
            Some(w) if w == r.pos_id => "selected_d_plus",
            Some(w) if w == r.neg_id => "selected_d_minus",
            _                         => "selected_other",
        }
    }
}

// â”€â”€ Bootstrap CI helper (10,000 resamples, frozen) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn bootstrap_ci_proportion(hits: usize, n: usize, iters: usize, seed: u64)
    -> (f64, f64)
{
    if n == 0 { return (0.0, 0.0); }
    let p = hits as f64 / n as f64;
    let mut rng = seed;
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let mut s = 0usize;
        for _ in 0..n {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let u = (rng >> 33) as usize;
            if (u % n) < hits { s += 1; }
        }
        samples.push(s as f64 / n as f64);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lo = samples[(0.025 * iters as f64) as usize];
    let hi = samples[(0.975 * iters as f64) as usize];
    let _ = p;
    (lo, hi)
}

// â”€â”€ Gate evaluation (frozen) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

struct GateResult {
    name:   &'static str,
    passed: bool,
    value:  f64,
    ci_lo:  f64,
    ci_hi:  f64,
    note:   String,
}

// â”€â”€ Main benchmark test â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn phase3_approval_benchmark() {
    assert!(
        !SCENARIOS.is_empty(),
        "SCENARIOS is empty â€” Role B must populate the corpus before running the benchmark."
    );
    assert!(
        SCENARIOS.len() >= 40,
        "SCENARIOS has {} entries; protocol requires â‰¥ 40.",
        SCENARIOS.len()
    );

    let n = SCENARIOS.len();
    let mut results: Vec<ScenarioResult> = Vec::with_capacity(n);

    for scenario in SCENARIOS.iter() {
        // â”€â”€ Isolated runtime per scenario â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let (mut rt, _tmp) = make_runtime_pair();
        let ctx_id = rt.create_context(
            &format!("approval-{}", scenario.id),
            ContextType::default(),
            None, "", vec![],
        ).expect("create_context").id.to_string();

        // â”€â”€ Populate 60+ background distractors â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let base_seed = DISTRACTOR_RNG_SEED
            .wrapping_add((scenario.id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        for i in 0..MIN_DISTRACTORS {
            learn_distractor(&mut rt, &ctx_id, base_seed.wrapping_add(i as u64));
        }

        // â”€â”€ Learn Dâˆ’ (negative / dangerous) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let neg_id = learn_text(
            &mut rt, &ctx_id,
            scenario.neg_text,
            scenario.neg_auth,
            scenario.neg_prov,
            None,
            scenario.neg_key,
            scenario.neg_polarity,
        );

        // â”€â”€ Learn D+ (positive / correct) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        // For SUPERSESSION: D+ supersedes Dâˆ’.
        let supersedes_arg = if scenario.supersedes_neg { Some(neg_id) } else { None };
        let pos_id = learn_text(
            &mut rt, &ctx_id,
            scenario.pos_text,
            scenario.pos_auth,
            scenario.pos_prov,
            supersedes_arg,
            scenario.pos_key,
            scenario.pos_polarity,
        );

        // â”€â”€ Retrieve via production unscoped path â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        use cmhm_core::text_to_embedding;
        let query_emb = text_to_embedding(scenario.query).to_vec();
        let (result, _) = rt
            .retrieve_unscoped(Some(&ctx_id), query_emb, "hash-128d")
            .expect("retrieve_unscoped");

        // â”€â”€ Pool stage membership â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let pos_in_raw_nn      = result.raw_nn_candidates.contains(&pos_id);
        let neg_in_raw_nn      = result.raw_nn_candidates.contains(&neg_id);
        let pos_in_post_filter = result.post_filter_candidates.contains(&pos_id);
        let neg_in_post_filter = result.post_filter_candidates.contains(&neg_id);

        let pos_rank_scored = result.scored_candidates.iter()
            .position(|&(id, _)| id == pos_id)
            .map(|i| i + 1);
        let neg_rank_scored = result.scored_candidates.iter()
            .position(|&(id, _)| id == neg_id)
            .map(|i| i + 1);
        let pos_score = result.scored_candidates.iter()
            .find(|&&(id, _)| id == pos_id)
            .map(|&(_, s)| s);
        let neg_score = result.scored_candidates.iter()
            .find(|&&(id, _)| id == neg_id)
            .map(|&(_, s)| s);

        // â”€â”€ Engine decision â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let abstained    = result.abstained;
        let winner_id    = result.seed_id;
        // conflict_type is stored on ConflictCandidate entries; take from first if present
        let conflict_type: Option<ConflictType> = result.conflict_candidates
            .first()
            .map(|c| c.conflict_type.clone());

        results.push(ScenarioResult {
            id: scenario.id,
            domain: scenario.domain,
            class: scenario.class,
            pos_in_raw_nn, neg_in_raw_nn,
            pos_in_post_filter, neg_in_post_filter,
            pos_rank_scored, neg_rank_scored,
            pos_score, neg_score,
            abstained, winner_id, conflict_type,
            expected_conflict: scenario.expected_conflict,
            expected_abstain: scenario.expected_abstain,
            expected_winner: scenario.expected_winner,
            pos_id, neg_id,
        });
    }

    // â”€â”€ Compute gate metrics â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    // Subpopulation filters
    let all_n = n;
    let supersession: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class == ScenarioClass::Supersession).collect();
    let contradiction: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class == ScenarioClass::Contradiction).collect();
    let auth_polarity_and_adversarial: Vec<&ScenarioResult> = results.iter()
        .filter(|r| matches!(r.class,
            ScenarioClass::AuthorityPolarity | ScenarioClass::AdversarialAuthority)).collect();
    let unresolved: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class == ScenarioClass::UnresolvedConflict).collect();
    let safe_abstention: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class == ScenarioClass::SafeAbstention).collect();

    // S1: LC eligibility â€” D+ and Dâˆ’ both in SCORED top-2, gap < 0.02
    let s1_eligible: Vec<&ScenarioResult> = contradiction.iter().copied()
        .filter(|r| {
            match (r.pos_rank_scored, r.neg_rank_scored, r.pos_score, r.neg_score) {
                (Some(pr), Some(nr), Some(ps), Some(ns))
                    if pr <= 2 && nr <= 2 && (ps - ns).abs() < 0.02 => true,
                _ => false,
            }
        }).collect();

    // G1: Recall@1
    let g1_hits = results.iter().filter(|r| r.pos_rank_scored == Some(1)).count();
    // G2: Recall@5
    let g2_hits = results.iter().filter(|r| r.pos_rank_scored.map_or(false, |r| r <= 5)).count();
    // G3: MRR
    let g3_sum: f64 = results.iter()
        .map(|r| r.pos_rank_scored.map_or(0.0, |r| 1.0 / r as f64)).sum();
    let g3_val = g3_sum / all_n as f64;
    // G4: Contradiction-at-1
    let g4_hits = results.iter().filter(|r| r.neg_rank_scored == Some(1)).count();
    // G5: Hard-Neg Occupancy@5 (200 slots total)
    let g5_slots: usize = 5 * all_n; // 5 slots per query; was hardcoded 200 (scaffold defect fix)
    let g5_neg_in_top5 = results.iter()
        .filter(|r| r.neg_rank_scored.map_or(false, |r| r <= 5)).count();
    // G6: Mean Margin
    let g6_sum: f64 = results.iter()
        .map(|r| (r.pos_score.unwrap_or(0.0) - r.neg_score.unwrap_or(0.0)) as f64).sum();
    let g6_val = g6_sum / all_n as f64;
    // G7: Supersession Safety â€” superseded Dâˆ’ must not appear in post_filter/scored/chain/winner
    let g7_violations = supersession.iter()
        .filter(|r| r.neg_in_post_filter || r.neg_rank_scored.is_some()
            || r.winner_id == Some(r.neg_id)).count();
    // G8a: LogicalContradiction rate over S1-eligible contradiction scenarios
    let g8a_hits = s1_eligible.iter()
        .filter(|r| r.conflict_type == Some(ConflictType::LogicalContradiction)).count();
    // G8b: AuthorityPolarity rate
    let g8b_hits = auth_polarity_and_adversarial.iter()
        .filter(|r| r.conflict_type == Some(ConflictType::AuthorityPolarity)).count();
    // G8c: Unresolved Non-Conflict rate
    let g8c_hits = unresolved.iter()
        .filter(|r| r.conflict_type.is_none()).count();
    // G9: Safe Abstention rate
    let g9_hits = safe_abstention.iter().filter(|r| r.abstained).count();
    // G10: Pool Contamination â€” Dâˆ’ absent from RAW_NN for non-supersession scenarios
    let non_supersession: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class != ScenarioClass::Supersession).collect();
    let g10_violations = non_supersession.iter()
        .filter(|r| !r.neg_in_raw_nn).count();

    // â”€â”€ Bootstrap CIs â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let bs = 10_000usize;
    let (g1_lo, g1_hi) = bootstrap_ci_proportion(g1_hits, all_n, bs, 0x1111);
    let (g2_lo, g2_hi) = bootstrap_ci_proportion(g2_hits, all_n, bs, 0x2222);
    // G3 MRR CI: resample reciprocal ranks
    let g3_vals: Vec<f64> = results.iter()
        .map(|r| r.pos_rank_scored.map_or(0.0, |r| 1.0 / r as f64)).collect();
    let (g3_lo, g3_hi) = {
        let mut rng = 0x3333u64;
        let mut samples = Vec::with_capacity(bs);
        for _ in 0..bs {
            let mut s = 0.0f64;
            for _ in 0..all_n {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                s += g3_vals[(rng >> 33) as usize % all_n];
            }
            samples.push(s / all_n as f64);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (samples[(0.025 * bs as f64) as usize], samples[(0.975 * bs as f64) as usize])
    };
    let (g4_lo, g4_hi) = bootstrap_ci_proportion(g4_hits, all_n, bs, 0x4444);
    let (g5_lo, g5_hi) = bootstrap_ci_proportion(g5_neg_in_top5, g5_slots, bs, 0x5555);
    // G6 margin CI
    let g6_margins: Vec<f64> = results.iter()
        .map(|r| (r.pos_score.unwrap_or(0.0) - r.neg_score.unwrap_or(0.0)) as f64).collect();
    let (g6_lo, g6_hi) = {
        let mut rng = 0x6666u64;
        let mut samples = Vec::with_capacity(bs);
        for _ in 0..bs {
            let mut s = 0.0f64;
            for _ in 0..all_n {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                s += g6_margins[(rng >> 33) as usize % all_n];
            }
            samples.push(s / all_n as f64);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (samples[(0.025 * bs as f64) as usize], samples[(0.975 * bs as f64) as usize])
    };

    // â”€â”€ Evaluate gates â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let gates: Vec<GateResult> = vec![
        GateResult { name: "G1",  passed: g1_lo  >= 0.40,
            value: g1_hits as f64 / all_n as f64, ci_lo: g1_lo,  ci_hi: g1_hi,
            note: format!("{}/{}", g1_hits, all_n) },
        GateResult { name: "G2",  passed: g2_lo  >= 0.67,
            value: g2_hits as f64 / all_n as f64, ci_lo: g2_lo,  ci_hi: g2_hi,
            note: format!("{}/{}", g2_hits, all_n) },
        GateResult { name: "G3",  passed: g3_lo  >  0.40,
            value: g3_val, ci_lo: g3_lo, ci_hi: g3_hi,
            note: format!("mrr={:.4}", g3_val) },
        GateResult { name: "G4",  passed: g4_hi  <  0.40,
            value: g4_hits as f64 / all_n as f64, ci_lo: g4_lo,  ci_hi: g4_hi,
            note: format!("{}/{}", g4_hits, all_n) },
        GateResult { name: "G5",  passed: g5_hi  <  0.40,
            value: g5_neg_in_top5 as f64 / g5_slots as f64,
            ci_lo: g5_lo, ci_hi: g5_hi,
            note: format!("{}/{}", g5_neg_in_top5, g5_slots) },
        GateResult { name: "G6",  passed: g6_lo  >  0.00,
            value: g6_val, ci_lo: g6_lo, ci_hi: g6_hi,
            note: format!("mean_margin={:.4}", g6_val) },
        GateResult { name: "G7",  passed: g7_violations == 0,
            value: g7_violations as f64, ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{} violations / {} supersession", g7_violations, supersession.len()) },
        GateResult { name: "S1",  passed: s1_eligible.len() >= (contradiction.len() * 60 / 100),
            value: s1_eligible.len() as f64 / contradiction.len().max(1) as f64,
            ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{}/{}", s1_eligible.len(), contradiction.len()) },
        GateResult { name: "G8a", passed: g8a_hits >= (s1_eligible.len() * 80 / 100),
            value: g8a_hits as f64 / s1_eligible.len().max(1) as f64,
            ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{}/{} eligible", g8a_hits, s1_eligible.len()) },
        GateResult { name: "G8b", passed: g8b_hits >= (auth_polarity_and_adversarial.len() * 80 / 100),
            value: g8b_hits as f64 / auth_polarity_and_adversarial.len().max(1) as f64,
            ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{}/{}", g8b_hits, auth_polarity_and_adversarial.len()) },
        GateResult { name: "G8c", passed: g8c_hits >= (unresolved.len() * 80 / 100),
            value: g8c_hits as f64 / unresolved.len().max(1) as f64,
            ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{}/{}", g8c_hits, unresolved.len()) },
        GateResult { name: "G9",  passed: g9_hits >= (safe_abstention.len() * 80 / 100),
            value: g9_hits as f64 / safe_abstention.len().max(1) as f64,
            ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{}/{}", g9_hits, safe_abstention.len()) },
        GateResult { name: "G10", passed: g10_violations == 0,
            value: g10_violations as f64, ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{} violations / {} non-supersession", g10_violations, non_supersession.len()) },
    ];

    // â”€â”€ 5-way outcome decomposition â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let mut outcomes = OutcomeCounts::default();
    for r in &results {
        match categorise(r) {
            "selected_d_plus"      => outcomes.selected_d_plus      += 1,
            "selected_d_minus"     => outcomes.selected_d_minus     += 1,
            "selected_other"       => outcomes.selected_other       += 1,
            "correct_abstention"   => outcomes.correct_abstention   += 1,
            "incorrect_abstention" => outcomes.incorrect_abstention += 1,
            _                      => {}
        }
    }

    // â”€â”€ Structured output (parsed by verify_phase3_approval.ps1) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    println!("PHASE3_RESULT|N={}", n);
    for g in &gates {
        println!(
            "PHASE3_RESULT|GATE|{}|passed={}|value={:.6}|ci_lo={:.6}|ci_hi={:.6}|note={}",
            g.name, g.passed, g.value, g.ci_lo, g.ci_hi, g.note
        );
    }
    println!("PHASE3_RESULT|OUTCOME|selected_d_plus={}",      outcomes.selected_d_plus);
    println!("PHASE3_RESULT|OUTCOME|selected_d_minus={}",     outcomes.selected_d_minus);
    println!("PHASE3_RESULT|OUTCOME|selected_other={}",       outcomes.selected_other);
    println!("PHASE3_RESULT|OUTCOME|correct_abstention={}",   outcomes.correct_abstention);
    println!("PHASE3_RESULT|OUTCOME|incorrect_abstention={}", outcomes.incorrect_abstention);

    // â”€â”€ Per-scenario detail â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    for r in &results {
        println!(
            "PHASE3_RESULT|SCENARIO|id={}|domain={}|class={:?}|pos_rank={:?}|neg_rank={:?}|\
             pos_score={:.4}|neg_score={:.4}|abstained={}|conflict={:?}|outcome={}",
            r.id, r.domain, r.class,
            r.pos_rank_scored, r.neg_rank_scored,
            r.pos_score.unwrap_or(0.0), r.neg_score.unwrap_or(0.0),
            r.abstained, r.conflict_type, categorise(r)
        );
    }

    // â”€â”€ Final verdict â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let all_passed = gates.iter().all(|g| g.passed);
    println!("PHASE3_RESULT|VERDICT|{}", if all_passed { "APPROVED" } else { "REJECTED" });

    assert!(
        all_passed,
        "Phase 3 approval benchmark FAILED. Check PHASE3_RESULT|GATE|... lines above for failures."
    );
}

// ── Stage 1.5: Pre-run conformance validator (Protocol v1.8 Section 12) ─────────────
//
// Validates every scenario in SCENARIOS against the Section 4.4 class-spec matrix.
// Called by verify_phase3_approval.ps1 before Stage 2. Failure terminates the attempt
// BEFORE the single run is consumed. No corpus content is inspected by Role A.

#[test]
fn phase3_conformance_check() {
    // ── Global: corpus size ────────────────────────────────────────────────────
    assert!(
        !SCENARIOS.is_empty(),
        "CONFORMANCE_CHECK|FAIL|global|field=corpus_size|value=0|expected=>=40 — SCENARIOS is empty"
    );
    assert!(
        SCENARIOS.len() >= 40,
        "CONFORMANCE_CHECK|FAIL|global|field=corpus_size|value={}|expected=>=40",
        SCENARIOS.len()
    );

    // ── Global: unique IDs ─────────────────────────────────────────────────────
    {
        let mut seen_ids = std::collections::HashSet::new();
        for s in SCENARIOS {
            assert!(
                seen_ids.insert(s.id),
                "CONFORMANCE_CHECK|FAIL|id={}|field=id|value={}|expected=unique",
                s.id, s.id
            );
        }
    }

    // ── Global: unique queries (case-insensitive) ──────────────────────────────
    {
        let mut seen_queries = std::collections::HashSet::new();
        for s in SCENARIOS {
            let lower = s.query.to_lowercase();
            assert!(
                seen_queries.insert(lower.clone()),
                "CONFORMANCE_CHECK|FAIL|id={}|field=query|value=duplicate|expected=unique",
                s.id
            );
        }
    }

    // ── Global: domain cap (<= 6 per domain) ──────────────────────────────────
    {
        let mut domain_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for s in SCENARIOS {
            *domain_counts.entry(s.domain).or_insert(0) += 1;
        }
        for (domain, count) in &domain_counts {
            assert!(
                *count <= 6,
                "CONFORMANCE_CHECK|FAIL|global|field=domain_cap|value={}:{}|expected=<=6",
                domain, count
            );
        }
    }

    // ── Global: per-class minimums ─────────────────────────────────────────────
    {
        use ScenarioClass::*;
        let counts = |cls: ScenarioClass| SCENARIOS.iter().filter(|s| s.class == cls).count();
        let contradiction_n = counts(Contradiction);
        let supersession_n  = counts(Supersession);
        let auth_pol_n      = counts(AuthorityPolarity);
        let adv_auth_n      = counts(AdversarialAuthority);
        let safe_abs_n      = counts(SafeAbstention);
        let unresolved_n    = counts(UnresolvedConflict);

        assert!(contradiction_n >= 10,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_Contradiction|value={}|expected=>=10",
            contradiction_n);
        assert!(supersession_n >= 6,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_Supersession|value={}|expected=>=6",
            supersession_n);
        assert!(auth_pol_n >= 6,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_AuthorityPolarity|value={}|expected=>=6",
            auth_pol_n);
        assert!(adv_auth_n >= 6,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_AdversarialAuthority|value={}|expected=>=6",
            adv_auth_n);
        assert!(safe_abs_n >= 6,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_SafeAbstention|value={}|expected=>=6",
            safe_abs_n);
        assert!(unresolved_n >= 6,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_UnresolvedConflict|value={}|expected=>=6",
            unresolved_n);

        println!(
            "CONFORMANCE_CHECK|PASS|N={}|classes=Contradiction:{},Supersession:{},\
             AuthorityPolarity:{},AdversarialAuthority:{},SafeAbstention:{},\
             UnresolvedConflict:{}",
            SCENARIOS.len(),
            contradiction_n, supersession_n, auth_pol_n,
            adv_auth_n, safe_abs_n, unresolved_n
        );
    }

    // ── Per-scenario validation ────────────────────────────────────────────────
    let mut checked = 0usize;
    for s in SCENARIOS {
        let id = s.id;
        match s.class {

            // ── CONTRADICTION ──────────────────────────────────────────────────
            // Section 12.3: pos_prov=user_explicit, pos_polarity=Some(1), pos_auth>=0.80,
            //   neg_auth in [0.70,0.85], neg_prov=contradicted, neg_polarity=Some(-1),
            //   neg_key==pos_key, supersedes_neg=false.
            ScenarioClass::Contradiction => {
                assert_eq!(s.pos_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} Contradiction field=pos_prov: {:?} -- expected user_explicit",
                    id, s.pos_prov);
                assert_eq!(s.pos_polarity, Some(1),
                    "CONFORMANCE FAIL id={} Contradiction field=pos_polarity: {:?} -- expected Some(1)",
                    id, s.pos_polarity);
                assert!(s.pos_auth >= 0.80,
                    "CONFORMANCE FAIL id={} Contradiction field=pos_auth: {:.3} -- expected >=0.80",
                    id, s.pos_auth);
                assert!(s.neg_auth >= 0.70 && s.neg_auth <= 0.85,
                    "CONFORMANCE FAIL id={} Contradiction field=neg_auth: {:.3} -- expected 0.70..=0.85",
                    id, s.neg_auth);
                assert_eq!(s.neg_prov, "contradicted",
                    "CONFORMANCE FAIL id={} Contradiction field=neg_prov: {:?} -- expected contradicted",
                    id, s.neg_prov);
                assert_eq!(s.neg_polarity, Some(-1),
                    "CONFORMANCE FAIL id={} Contradiction field=neg_polarity: {:?} -- expected Some(-1)",
                    id, s.neg_polarity);
                assert_eq!(s.neg_key, s.pos_key,
                    "CONFORMANCE FAIL id={} Contradiction field=neg_key: {:?} -- expected == pos_key {:?}",
                    id, s.neg_key, s.pos_key);
                assert!(!s.supersedes_neg,
                    "CONFORMANCE FAIL id={} Contradiction field=supersedes_neg: true -- expected false",
                    id);
            }

            // ── SUPERSESSION ───────────────────────────────────────────────────
            // Section 12.3: pos_prov=user_explicit, neg_prov=contradicted,
            //   pos_auth >= neg_auth + 0.05, neg_polarity==pos_polarity,
            //   supersedes_neg=true.
            ScenarioClass::Supersession => {
                assert_eq!(s.pos_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} Supersession field=pos_prov: {:?} -- expected user_explicit",
                    id, s.pos_prov);
                assert_eq!(s.neg_prov, "contradicted",
                    "CONFORMANCE FAIL id={} Supersession field=neg_prov: {:?} -- expected contradicted",
                    id, s.neg_prov);
                assert!(s.pos_auth >= s.neg_auth + 0.05,
                    "CONFORMANCE FAIL id={} Supersession field=auth_gap: pos={:.3} neg={:.3} -- expected pos>=neg+0.05",
                    id, s.pos_auth, s.neg_auth);
                assert_eq!(s.neg_polarity, s.pos_polarity,
                    "CONFORMANCE FAIL id={} Supersession field=polarity_match: neg={:?} pos={:?} -- must be equal (same-fact temporal replacement)",
                    id, s.neg_polarity, s.pos_polarity);
                assert!(s.supersedes_neg,
                    "CONFORMANCE FAIL id={} Supersession field=supersedes_neg: false -- expected true",
                    id);
            }

            // ── AUTHORITY-POLARITY ─────────────────────────────────────────────
            // Section 12.3: pos_auth in [0.80,0.90], pos_prov in {user_explicit,corrected},
            //   neg_auth in [0.70,0.80], neg_prov=contradicted,
            //   neg_key!=pos_key OR neg_key==None, supersedes_neg=false.
            ScenarioClass::AuthorityPolarity => {
                assert!(s.pos_auth >= 0.80 && s.pos_auth <= 0.90,
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=pos_auth: {:.3} -- expected 0.80..=0.90",
                    id, s.pos_auth);
                assert!(s.pos_prov == "user_explicit" || s.pos_prov == "corrected",
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=pos_prov: {:?} -- expected user_explicit or corrected",
                    id, s.pos_prov);
                assert!(s.neg_auth >= 0.70 && s.neg_auth <= 0.80,
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=neg_auth: {:.3} -- expected 0.70..=0.80",
                    id, s.neg_auth);
                assert_eq!(s.neg_prov, "contradicted",
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=neg_prov: {:?} -- expected contradicted",
                    id, s.neg_prov);
                assert!(s.neg_key != s.pos_key || s.neg_key.is_none(),
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=neg_key: {:?} -- expected different from pos_key {:?} or None",
                    id, s.neg_key, s.pos_key);
                assert!(!s.supersedes_neg,
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=supersedes_neg: true -- expected false",
                    id);
            }

            // ── ADVERSARIAL-AUTHORITY ──────────────────────────────────────────
            // Section 12.3: pos_auth in [0.75,0.85], pos_prov=corrected,
            //   neg_auth in [0.90,1.00], neg_prov=user_explicit,
            //   neg_auth > pos_auth, supersedes_neg=false.
            ScenarioClass::AdversarialAuthority => {
                assert!(s.pos_auth >= 0.75 && s.pos_auth <= 0.85,
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=pos_auth: {:.3} -- expected 0.75..=0.85",
                    id, s.pos_auth);
                assert_eq!(s.pos_prov, "corrected",
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=pos_prov: {:?} -- expected corrected",
                    id, s.pos_prov);
                assert!(s.neg_auth >= 0.90 && s.neg_auth <= 1.00,
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=neg_auth: {:.3} -- expected 0.90..=1.00",
                    id, s.neg_auth);
                assert_eq!(s.neg_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=neg_prov: {:?} -- expected user_explicit (adversarial high-authority assertion)",
                    id, s.neg_prov);
                assert!(s.neg_auth > s.pos_auth,
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=auth_ordering: neg={:.3} pos={:.3} -- expected neg_auth > pos_auth",
                    id, s.neg_auth, s.pos_auth);
                assert!(!s.supersedes_neg,
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=supersedes_neg: true -- expected false",
                    id);
            }

            // ── SAFE-ABSTENTION ────────────────────────────────────────────────
            // Section 12.3: pos_auth in [0.75,0.90], pos_prov=user_explicit,
            //   neg_auth in [0.75,0.87], neg_prov=contradicted,
            //   |pos_auth-neg_auth|<=0.10, supersedes_neg=false.
            // NOTE: no pos_auth > neg_auth ordering constraint (near-equal is the spec;
            //   either may be slightly higher -- see Section 12.3 annotation).
            ScenarioClass::SafeAbstention => {
                assert!(s.pos_auth >= 0.75 && s.pos_auth <= 0.90,
                    "CONFORMANCE FAIL id={} SafeAbstention field=pos_auth: {:.3} -- expected 0.75..=0.90",
                    id, s.pos_auth);
                assert_eq!(s.pos_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} SafeAbstention field=pos_prov: {:?} -- expected user_explicit",
                    id, s.pos_prov);
                assert!(s.neg_auth >= 0.75 && s.neg_auth <= 0.87,
                    "CONFORMANCE FAIL id={} SafeAbstention field=neg_auth: {:.3} -- expected 0.75..=0.87",
                    id, s.neg_auth);
                assert_eq!(s.neg_prov, "contradicted",
                    "CONFORMANCE FAIL id={} SafeAbstention field=neg_prov: {:?} -- expected contradicted",
                    id, s.neg_prov);
                assert!((s.pos_auth - s.neg_auth).abs() <= 0.10,
                    "CONFORMANCE FAIL id={} SafeAbstention field=auth_gap: |{:.3}-{:.3}|={:.3} -- expected <=0.10",
                    id, s.pos_auth, s.neg_auth, (s.pos_auth - s.neg_auth).abs());
                assert!(!s.supersedes_neg,
                    "CONFORMANCE FAIL id={} SafeAbstention field=supersedes_neg: true -- expected false",
                    id);
            }

            // ── UNRESOLVED-CONFLICT ────────────────────────────────────────────
            // Section 12.3: pos_auth in [0.80,1.00], pos_prov=user_explicit,
            //   neg_auth in [0.75,0.85], neg_prov=user_explicit,
            //   pos_auth > neg_auth, no polarity clash, supersedes_neg=false.
            ScenarioClass::UnresolvedConflict => {
                assert!(s.pos_auth >= 0.80 && s.pos_auth <= 1.00,
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=pos_auth: {:.3} -- expected 0.80..=1.00",
                    id, s.pos_auth);
                assert_eq!(s.pos_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=pos_prov: {:?} -- expected user_explicit",
                    id, s.pos_prov);
                assert!(s.neg_auth >= 0.75 && s.neg_auth <= 0.85,
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=neg_auth: {:.3} -- expected 0.75..=0.85",
                    id, s.neg_auth);
                assert_eq!(s.neg_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=neg_prov: {:?} -- expected user_explicit",
                    id, s.neg_prov);
                assert!(s.pos_auth > s.neg_auth,
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=auth_ordering: pos={:.3} neg={:.3} -- expected pos_auth > neg_auth",
                    id, s.pos_auth, s.neg_auth);
                // Polarity clash check: must NOT have (pos=+1, neg=-1) or (pos=-1, neg=+1)
                let clash = matches!(
                    (s.pos_polarity, s.neg_polarity),
                    (Some(1), Some(-1)) | (Some(-1), Some(1))
                );
                assert!(!clash,
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=polarity_clash: pos={:?} neg={:?} -- expected no clash",
                    id, s.pos_polarity, s.neg_polarity);
                assert!(!s.supersedes_neg,
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=supersedes_neg: true -- expected false",
                    id);
            }
        }
        checked += 1;
    }

    // ── Self-integrity: all scenarios were checked ─────────────────────────────
    assert_eq!(checked, SCENARIOS.len(),
        "CONFORMANCE_CHECK|FAIL|global|field=self_integrity: checked={} but SCENARIOS.len()={}",
        checked, SCENARIOS.len());

    println!(
        "CONFORMANCE_CHECK|PASS|N={}|all_per_scenario_checks=ok|domains_ok=true",
        SCENARIOS.len()
    );
}
