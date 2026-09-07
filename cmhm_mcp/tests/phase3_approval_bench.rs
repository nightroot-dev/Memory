//! Phase 3 Approval Benchmark — Role A scaffolding (frozen at bc66340)
//!
//! AUTHORING BOUNDARY (Protocol v1.5 Section 4.1):
//!   Role A authored this file: all types, the runner harness, scoring logic,
//!   distractor generation, pool-stage extraction, gate metric computation,
//!   and the PHASE3_RESULT|...| structured output formatter.
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
//! EXECUTION CONSTRAINT:
//!   This benchmark consumes the single approval attempt when Stage 2 launches.
//!   Do not run `scripts/verify_phase3_approval.ps1` without `-SyntheticOnly`
//!   until Role B has committed and audited the corpus.

use std::sync::atomic::{AtomicU32, Ordering};
use tempfile::TempDir;

use cmhm_core::{Config, ContextRuntime, ContextType, ConflictType, CallerRole};

// ── Scenario types (frozen — Role B must not modify) ─────────────────────────

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
    /// If true, D+ supersedes D− (SUPERSESSION class only).
    pub supersedes_neg: bool,
    /// Expected conflict classification from the engine.
    pub expected_conflict: ExpectedConflict,
    /// If true, the engine must abstain (winner = None).
    pub expected_abstain: bool,
    /// Which candidate should win if not abstained.
    pub expected_winner: ExpectedWinner,
}

// ── ROLE B: POPULATE THIS SLICE ONLY. MINIMUM 40 SCENARIOS. ──────────────────
// Zero structural or harness logic changes are permitted.
// Follow Section 4 of Protocol v1.5 strictly.
pub static SCENARIOS: &[ApprovalScenario] = &[
    // Role B fills scenario instances here
];

// ── Distractor RNG seed (frozen by protocol) ──────────────────────────────────
const DISTRACTOR_RNG_SEED: u64 = 0xA3F7_2B91_C5D4_E608;
const MIN_DISTRACTORS: usize = 60;

// ── Runtime counter for isolated temp dirs ───────────────────────────────────
static COUNTER: AtomicU32 = AtomicU32::new(0);

// ── Harness helpers (frozen — Role B must not modify) ────────────────────────

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

// ── Per-scenario result (frozen) ──────────────────────────────────────────────

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

// ── 5-way outcome categorisation (frozen) ─────────────────────────────────────

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

// ── Bootstrap CI helper (10,000 resamples, frozen) ────────────────────────────

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

// ── Gate evaluation (frozen) ───────────────────────────────────────────────────

struct GateResult {
    name:   &'static str,
    passed: bool,
    value:  f64,
    ci_lo:  f64,
    ci_hi:  f64,
    note:   String,
}

// ── Main benchmark test ────────────────────────────────────────────────────────

#[test]
fn phase3_approval_benchmark() {
    assert!(
        !SCENARIOS.is_empty(),
        "SCENARIOS is empty — Role B must populate the corpus before running the benchmark."
    );
    assert!(
        SCENARIOS.len() >= 40,
        "SCENARIOS has {} entries; protocol requires ≥ 40.",
        SCENARIOS.len()
    );

    let n = SCENARIOS.len();
    let mut results: Vec<ScenarioResult> = Vec::with_capacity(n);

    for scenario in SCENARIOS.iter() {
        // ── Isolated runtime per scenario ──────────────────────────────────
        let (mut rt, _tmp) = make_runtime_pair();
        let ctx_id = rt.create_context(
            &format!("approval-{}", scenario.id),
            ContextType::default(),
            None, "", vec![],
        ).expect("create_context").id.to_string();

        // ── Populate 60+ background distractors ───────────────────────────
        let base_seed = DISTRACTOR_RNG_SEED
            .wrapping_add((scenario.id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        for i in 0..MIN_DISTRACTORS {
            learn_distractor(&mut rt, &ctx_id, base_seed.wrapping_add(i as u64));
        }

        // ── Learn D− (negative / dangerous) ───────────────────────────────
        let neg_id = learn_text(
            &mut rt, &ctx_id,
            scenario.neg_text,
            scenario.neg_auth,
            scenario.neg_prov,
            None,
            scenario.neg_key,
            scenario.neg_polarity,
        );

        // ── Learn D+ (positive / correct) ─────────────────────────────────
        // For SUPERSESSION: D+ supersedes D−.
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

        // ── Retrieve via production unscoped path ──────────────────────────
        use cmhm_core::text_to_embedding;
        let query_emb = text_to_embedding(scenario.query).to_vec();
        let (result, _) = rt
            .retrieve_unscoped(Some(&ctx_id), query_emb, "hash-128d")
            .expect("retrieve_unscoped");

        // ── Pool stage membership ──────────────────────────────────────────
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

        // ── Engine decision ────────────────────────────────────────────────
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

    // ── Compute gate metrics ──────────────────────────────────────────────────

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

    // S1: LC eligibility — D+ and D− both in SCORED top-2, gap < 0.02
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
    let g5_slots: usize = 200;
    let g5_neg_in_top5 = results.iter()
        .filter(|r| r.neg_rank_scored.map_or(false, |r| r <= 5)).count();
    // G6: Mean Margin
    let g6_sum: f64 = results.iter()
        .map(|r| (r.pos_score.unwrap_or(0.0) - r.neg_score.unwrap_or(0.0)) as f64).sum();
    let g6_val = g6_sum / all_n as f64;
    // G7: Supersession Safety — superseded D− must not appear in post_filter/scored/chain/winner
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
    // G10: Pool Contamination — D− absent from RAW_NN for non-supersession scenarios
    let non_supersession: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class != ScenarioClass::Supersession).collect();
    let g10_violations = non_supersession.iter()
        .filter(|r| !r.neg_in_raw_nn).count();

    // ── Bootstrap CIs ─────────────────────────────────────────────────────────
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

    // ── Evaluate gates ─────────────────────────────────────────────────────────
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

    // ── 5-way outcome decomposition ────────────────────────────────────────────
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

    // ── Structured output (parsed by verify_phase3_approval.ps1) ──────────────
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

    // ── Per-scenario detail ────────────────────────────────────────────────────
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

    // ── Final verdict ──────────────────────────────────────────────────────────
    let all_passed = gates.iter().all(|g| g.passed);
    println!("PHASE3_RESULT|VERDICT|{}", if all_passed { "APPROVED" } else { "REJECTED" });

    assert!(
        all_passed,
        "Phase 3 approval benchmark FAILED. Check PHASE3_RESULT|GATE|... lines above for failures."
    );
}
