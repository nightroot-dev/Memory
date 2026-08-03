//! Router integration tests.
//!
//! These tests validate the end-to-end scoring pipeline:
//!   config -> RouterWeights -> score() -> top_k() -> ranked results
//!
//! Every test constructs contexts with known (name, description, tags, state, age, access_count)
//! and asserts exact ranking order. No I/O, no registry, no engine — pure math.

#![cfg(test)]

use crate::config::Config;
use crate::context::{ContextMeta, ContextType, ContextState, ContextId, now_secs};
use crate::context::router::{RouterWeights, top_k, score};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_ctx(
    name: &str,
    description: &str,
    tags: &[&str],
    state: ContextState,
    age_secs: u64,
    access_count: u64,
) -> ContextMeta {
    let now = now_secs();
    ContextMeta {
        id:            ContextId::new(),
        display_name:  name.to_string(),
        context_type:  ContextType::Project,
        state,
        parent_id:     None,
        description:   description.to_string(),
        tags:          tags.iter().map(|s| s.to_string()).collect(),
        created_at:    now.saturating_sub(age_secs as i64),
        last_accessed: now.saturating_sub(age_secs as i64),
        access_count,
        version:       0,
        learn_count:   0,
    }
}

fn default_weights() -> RouterWeights {
    RouterWeights::from_config(&Config::default())
}

fn refs<'a>(ctxs: &'a [ContextMeta]) -> Vec<&'a ContextMeta> {
    ctxs.iter().collect()
}

// ── Test 1: Keyword match dominates when tags are distinct ────────────────────

/// CMHM matches all 3 query words; VideoAI matches 0.
/// Expected rank: CMHM (0.70) > VideoAI (0.20).
/// Derivation (both just-created, no accesses, not Active):
///   alpha=0.50, beta=0.20, gamma=0.20, delta=0.10
///   CMHM:   Sq=1.0 Sh=0 St~1.0 Sa=0 → 0.50+0+0.20+0 = 0.70
///   VideoAI:Sq=0.0 Sh=0 St~1.0 Sa=0 → 0+0+0.20+0    = 0.20
#[test]
fn keyword_match_dominates() {
    let ctxs = vec![
        make_ctx("CMHM",   "causal memory", &["rust","memory"], ContextState::Creating, 1, 0),
        make_ctx("VideoAI","video processing",&["video","ai"],  ContextState::Creating, 1, 0),
    ];
    let weights = default_weights();
    let r = refs(&ctxs);
    let ranked = top_k(&r, "memory causal rust", &weights, 2);
    assert_eq!(ranked.len(), 2);
    let names: Vec<&str> = ranked.iter().map(|(m,_)| m.display_name.as_str()).collect();
    assert_eq!(names[0], "CMHM",    "CMHM should rank first: matched all query words");
    assert_eq!(names[1], "VideoAI", "VideoAI should rank second: matched no query words");
    // Score sanity
    assert!((ranked[0].1 - 0.70).abs() < 0.02, "CMHM score should be ~0.70, got {}", ranked[0].1);
    assert!((ranked[1].1 - 0.20).abs() < 0.02, "VideoAI score should be ~0.20, got {}", ranked[1].1);
}

// ── Test 2: Active-context bonus (Sa) flips ranking when Sq is tied ──────────

/// Two contexts with identical tags and age.
/// One is Active (Sa=1), one is Sleeping (Sa=0).
/// Expected: Active context scores delta=0.10 higher.
#[test]
fn active_bonus_breaks_tie() {
    let ctxs = vec![
        make_ctx("Alpha", "machine learning", &["ml","model"], ContextState::Sleeping, 10, 5),
        make_ctx("Beta",  "machine learning", &["ml","model"], ContextState::Active,   10, 5),
    ];
    let weights = default_weights();
    let r = refs(&ctxs);
    let ranked = top_k(&r, "ml model", &weights, 2);
    let diff = ranked[0].1 - ranked[1].1;
    assert!((diff - 0.10).abs() < 1e-5,
        "Active bonus should be exactly delta=0.10. Got diff={diff}");
    assert_eq!(ranked[0].0.display_name, "Beta",  "Active context should rank first");
    assert_eq!(ranked[1].0.display_name, "Alpha", "Sleeping context should rank second");
}

// ── Test 3: Idle is serviceable (Sa=1), same as Active ───────────────────────

#[test]
fn idle_gets_active_bonus() {
    let idle  = make_ctx("IdleCtx",    "test", &["t"], ContextState::Idle,     5, 3);
    let sleep = make_ctx("SleepCtx",   "test", &["t"], ContextState::Sleeping, 5, 3);
    let weights = default_weights();
    let s_idle  = score(&idle,  "t", &weights, 3);
    let s_sleep = score(&sleep, "t", &weights, 3);
    assert!(s_idle > s_sleep,
        "Idle ({s_idle:.3}) should beat Sleeping ({s_sleep:.3}) by delta=0.10");
    assert!((s_idle - s_sleep - 0.10).abs() < 0.001,
        "Difference should be exactly delta=0.10");
}

// ── Test 4: Recency decay — older context scores lower on St ─────────────────

/// Two identical contexts: one fresh (1s old), one very old (365 days old).
/// With default half_life=86400s: St(365d) = exp(-ln(2)*365) ≈ 0.0 (nearly zero).
/// Fresh: St ≈ 1.0.  Expected gap ≈ gamma*(1.0 - ~0.0) = 0.20.
#[test]
fn recency_decay_penalises_old_context() {
    let fresh = make_ctx("Fresh", "causal", &["rust"], ContextState::Creating, 1,         0);
    let old   = make_ctx("Old",   "causal", &["rust"], ContextState::Creating, 365*86400, 0);
    let weights = default_weights();
    let s_fresh = score(&fresh, "rust causal", &weights, 0);
    let s_old   = score(&old,   "rust causal", &weights, 0);
    assert!(s_fresh > s_old,
        "Fresh ({s_fresh:.3}) should outscore year-old context ({s_old:.3})");
    // St for 1-year-old context ≈ 0 → score difference ≈ gamma = 0.20
    let diff = s_fresh - s_old;
    assert!(diff > 0.15 && diff < 0.22,
        "Recency gap should be ~0.20 (gamma), got {diff:.4}");
}

// ── Test 5: Frequency (Sh) acts as a tiebreaker between equally tagged contexts

/// Three contexts, same tags, same age. Differ only in access_count.
/// Expected ranking mirrors access_count descending.
#[test]
fn frequency_breaks_tag_tie() {
    let ctxs = vec![
        make_ctx("Low",  "ml model", &["ml"], ContextState::Creating, 10, 2),
        make_ctx("High", "ml model", &["ml"], ContextState::Creating, 10, 10),
        make_ctx("Mid",  "ml model", &["ml"], ContextState::Creating, 10, 5),
    ];
    let weights = default_weights();
    let r = refs(&ctxs);
    let ranked = top_k(&r, "ml model", &weights, 3);
    let names: Vec<&str> = ranked.iter().map(|(m,_)| m.display_name.as_str()).collect();
    assert_eq!(names, vec!["High","Mid","Low"],
        "Should rank by access frequency when tags are equal");
}

// ── Test 6: Deleted context can never appear in top_k results ────────────────

/// Registry excludes Deleted contexts before calling top_k. But even if one
/// slips through (defensive), it should score identically to Sleeping (Sa=0).
/// This validates the FSM-to-router contract: Deleted is not serviceable.
#[test]
fn deleted_gets_no_active_bonus() {
    let deleted  = make_ctx("Gone",  "ml", &["ml"], ContextState::Deleted,  1, 100);
    let sleeping = make_ctx("Sleep", "ml", &["ml"], ContextState::Sleeping, 1, 100);
    let weights  = default_weights();
    let s_del  = score(&deleted,  "ml", &weights, 100);
    let s_slp  = score(&sleeping, "ml", &weights, 100);
    assert_eq!(s_del, s_slp,
        "Deleted and Sleeping should score identically (both Sa=0). del={s_del:.3} slp={s_slp:.3}");
}

// ── Test 7: Recovering context gets no active bonus ──────────────────────────

#[test]
fn recovering_context_is_not_serviceable() {
    let recovering = make_ctx("Crashed", "ml", &["ml"], ContextState::Recovering, 1, 5);
    let active     = make_ctx("Healthy", "ml", &["ml"], ContextState::Active,      1, 5);
    let weights    = default_weights();
    let s_rec = score(&recovering, "ml", &weights, 5);
    let s_act = score(&active,     "ml", &weights, 5);
    assert!(s_act > s_rec, "Active ({s_act:.3}) should outscore Recovering ({s_rec:.3})");
    assert!((s_act - s_rec - 0.10).abs() < 0.001,
        "Difference should be exactly delta=0.10");
}

// ── Test 8: Empty query — all contexts score on Sh+St+Sa only (Sq=0) ─────────

#[test]
fn empty_query_uses_history_and_recency_only() {
    let ctxs = vec![
        make_ctx("ActiveFreq",  "x", &["x"], ContextState::Active,   1,  50),
        make_ctx("SleepInfreq", "x", &["x"], ContextState::Sleeping, 30, 1),
    ];
    let weights = default_weights();
    let r = refs(&ctxs);
    let ranked = top_k(&r, "", &weights, 2);
    assert_eq!(ranked[0].0.display_name, "ActiveFreq",
        "With empty query, active+frequent should beat sleeping+infrequent");
}

// ── Test 9: Config weight override changes ranking ────────────────────────────

/// Override alpha=0.0 (ignore keywords completely).
/// Now two contexts that differ only in keywords should score identically.
#[test]
fn weight_override_disables_keyword_scoring() {
    let mut cfg = Config::default();
    cfg.router_alpha = 0.0;
    cfg.router_beta  = 0.0;
    cfg.router_gamma = 0.50;
    cfg.router_delta = 0.50;
    let weights = RouterWeights::from_config(&cfg);

    let c1 = make_ctx("RustCtx",  "rust memory", &["rust","memory"], ContextState::Active, 1, 10);
    let c2 = make_ctx("PythonCtx","python ml",   &["python","ml"],   ContextState::Active, 1, 10);

    let s1 = score(&c1, "rust memory causal", &weights, 10);
    let s2 = score(&c2, "rust memory causal", &weights, 10);

    // With alpha=0 both contexts have Sq contribution=0; all other components identical
    assert!((s1 - s2).abs() < 0.001,
        "With alpha=0, keyword-matched and unmatched should score identically. s1={s1:.4} s2={s2:.4}");
}

// ── Test 10: Partial keyword match scores proportionally ─────────────────────

/// Query has 4 words; context matches 2 of 4 → Sq = 0.50.
#[test]
fn partial_keyword_match_is_proportional() {
    let ctx = make_ctx("PartialMatch", "video processing", &["video","ai"],
                       ContextState::Creating, 1, 0);
    let weights = default_weights();
    // Query: "video ai memory rust" — 4 words, 2 match ("video","ai")
    let s = score(&ctx, "video ai memory rust", &weights, 0);
    // Expected: alpha*0.50 + 0 + gamma*~1.0 + 0 ≈ 0.25 + 0.20 = 0.45
    assert!(s > 0.40 && s < 0.50,
        "Partial match (2/4 words) should score ~0.45, got {s:.4}");
}

