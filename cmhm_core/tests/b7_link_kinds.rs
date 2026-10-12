// cmhm_core/tests/b7_link_kinds.rs
//
// B7 (S-19, refined direction): hide CROSS-EXPERIENCE links, not "prior-only"
// links. The learn-event ledger says whether a chain step connects nodes of
// the same experience or two different ones:
//
// | link kind                              | human block              | JSON   |
// |----------------------------------------|----------------------------|--------|
// | SameExperience                         | always shown             | kept   |
// | CrossExperienceUnverified (prior-only) | hidden + count marker    | kept   |
// | CrossExperienceLearned (reinforced)    | shown + "learned assoc." | kept   |
// | Unknown (pre-B1b/evicted ledgerless)   | shown + "unverified"     | kept   |
//
// Why not the strict evidence predicate (first B7 attempt, reverted): a fresh
// legitimate edge and a spurious one BOTH have evidence 2.0, so hiding every
// prior-only step emptied fresh chains and broke the B1 contract tests (clean
// 77e6fce passes B1 12/12; the strict-predicate WIP flaked them). The ledger
// distinguishes what evidence cannot.
//
// PRESERVATION RULE: if a test below fails, fix the production code, not the
// assertion. Ranking/scoring are untouched by B7 — these tests pin rendering.

use cmhm_core::authority::CallerRole;
use cmhm_core::engine::EngineHandle;
use cmhm_core::learn_ledger::LearnLedger;
use cmhm_core::retrieval::{classify_link, is_prior_only_evidence, EpisodeStatus, LinkKind};
use cmhm_core::{Config, LearnResult, NodeType, EMBED_DIM};
use cmhm_core::engine;
use std::path::PathBuf;

/// Unit vector e[dim].
fn unit_vec(dim: usize) -> Vec<f32> {
    assert!(dim < EMBED_DIM);
    let mut v = vec![0.0f32; EMBED_DIM];
    v[dim] = 1.0;
    v
}

/// Normalized sparse vector (cosine-safe for birth/dedup thresholds).
fn sparse_unit_vec(components: &[(usize, f32)]) -> Vec<f32> {
    let mut v = vec![0.0f32; EMBED_DIM];
    for &(dim, val) in components {
        v[dim] = val;
    }
    let norm = v.iter().map(|f| f * f).sum::<f32>().sqrt().max(1e-9);
    v.iter_mut().for_each(|f| *f /= norm);
    v
}

fn test_config(data_dir: PathBuf) -> Config {
    Config {
        data_dir: data_dir.clone(),
        projections_dir: data_dir.join("projections"),
        retrieval_authority_weight: 0.25,
        conflict_detection_threshold: 0.02,
        ..Config::default()
    }
}

fn learn_triple(
    handle: &cmhm_core::engine::EngineHandle,
    s: Vec<f32>,
    a: Vec<f32>,
    o: Vec<f32>,
    st: String,
    at: String,
    ot: String,
) -> cmhm_core::LearnResult {
    handle
        .learn_with_text_context(
            s,
            a,
            o,
            0.5,
            0.5,
            "hash-128d",
            0.9,
            "user_explicit".to_string(),
            None,
            CallerRole::User,
            None,
            None,
            Some((st, at, ot)),
        )
        .expect("learn failed")
}

// ── T-same: a fresh same-experience chain stays fully visible ────────────
// This is the B1-flake regression pin: under the reverted strict predicate,
// fresh Beta(1,1) edges (evidence exactly 2.0) were hidden and emptied fresh
// chains. Same-experience steps must ALWAYS render, whatever their evidence.

#[test]
fn t_same_experience_chain_visible() {
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = engine::start(test_config(dir.path().to_path_buf())).expect("start");
    learn_triple(
        &handle,
        unit_vec(10),
        sparse_unit_vec(&[(10, 0.7), (11, 0.7)]),
        sparse_unit_vec(&[(10, 0.7), (12, 0.7)]),
        "t-same state port audit".to_string(),
        "t-same action restarted proxy".to_string(),
        "t-same outcome healthy 200".to_string(),
    );
    let r = handle
        .retrieve(unit_vec(10), "hash-128d", NodeType::State, "")
        .expect("retrieve failed");
    assert!(
        !r.causal_chain.is_empty(),
        "T-same: single related learn must form a chain"
    );
    for (i, step) in r.causal_chain.iter().enumerate() {
        assert_eq!(
            step.link_kind,
            LinkKind::SameExperience,
            "T-same: every step of a one-experience chain is same-experience (step {i})"
        );
        assert!(
            r.explanation.contains(step.from_text.as_ref().map(|t| t.text.as_str()).unwrap_or("")),
            "T-same: same-experience step text must render (step {i}): {}",
            r.explanation
        );
    }
    assert!(
        !r.explanation.contains("unverified links hidden"),
        "T-same: no hidden-marker on an all-same chain: {}",
        r.explanation
    );
}

// ── Shared topology for the cross-experience tests ──────────────────────
// A_state = e0. A_action = e1 (dissimilar to state: no state→action edge).
// A_outcome ~ e1 (links from action only). B_* nodes ~ e0 (link FROM A_state
// only — every A_state outgoing edge is cross-experience by construction, so
// the first chain step from an A_state seed is ALWAYS cross-experience,
// whatever the Thompson-sampled Dijkstra picks downstream).

struct AbStore {
    _tmp: tempfile::TempDir,
    handle: cmhm_core::engine::EngineHandle,
    a_state: u64,
    b_state: u64,
    b_action: u64,
    b_outcome: u64,
}

fn build_ab_store(tag: &str) -> AbStore {
    let tmp = tempfile::tempdir().expect("tempdir");
    let handle = engine::start(test_config(tmp.path().to_path_buf())).expect("start");
    let a = learn_triple(
        &handle,
        unit_vec(0),
        unit_vec(1),
        sparse_unit_vec(&[(1, 0.7), (4, 0.7)]),
        format!("{tag} A state port audit"),
        format!("{tag} A action rotated keys"),
        format!("{tag} A outcome keys rotated"),
    );
    let b = learn_triple(
        &handle,
        sparse_unit_vec(&[(0, 0.7), (5, 0.7)]),
        sparse_unit_vec(&[(0, 0.7), (6, 0.7)]),
        sparse_unit_vec(&[(0, 0.7), (7, 0.7)]),
        format!("{tag} B state cache audit"),
        format!("{tag} B action flushed cache"),
        format!("{tag} B outcome cache clear"),
    );
    AbStore {
        _tmp: tmp,
        handle,
        a_state: a.state_node_id,
        b_state: b.state_node_id,
        b_action: b.action_node_id,
        b_outcome: b.outcome_node_id,
    }
}

fn retrieve_a_state(store: &AbStore) -> cmhm_core::RetrievalResult {
    store
        .handle
        .retrieve(unit_vec(0), "hash-128d", NodeType::State, "")
        .expect("retrieve failed")
}

// ── T-cross-prior: spurious link hidden, marker present, JSON kept ───────

#[test]
fn t_cross_prior_hidden_with_marker() {
    let store = build_ab_store("t-cross-prior");
    let r = retrieve_a_state(&store);
    assert_eq!(
        r.seed_id,
        Some(store.a_state),
        "T-cross-prior: A_state (cos 1.0) must seed"
    );
    assert!(
        !r.causal_chain.is_empty(),
        "T-cross-prior: A_state has cross outgoing edges, chain must form"
    );
    // The first step leaves the seed, whose only outgoing edges are cross.
    assert_eq!(
        r.causal_chain[0].link_kind,
        LinkKind::CrossExperienceUnverified,
        "T-cross-prior: first step from A_state is always a prior-only cross link"
    );
    // Every step renders exactly per its kind; hidden count matches the marker.
    let hidden = r
        .causal_chain
        .iter()
        .filter(|s| s.link_kind == LinkKind::CrossExperienceUnverified)
        .count();
    assert!(hidden >= 1, "T-cross-prior: at least the first step hides");
    assert!(
        r.explanation.contains("unverified link"),
        "T-cross-prior: hidden-marker must render (singular or plural): {}",
        r.explanation
    );
    // The seed triple still resolves (rendered first by the MCP triple
    // block — core explanation only renders the chain, so assert the view).
    let sep = r.seed_episode.as_ref().expect("T-cross-prior: seed triple resolves");
    assert!(
        sep.state_text.as_deref().unwrap_or("").contains("A state port audit"),
        "T-cross-prior: seed episode carries the seed text"
    );
    assert_eq!(
        r.seed_episode_status,
        cmhm_core::retrieval::EpisodeStatus::Found
    );
}

// ── T-cross-reinforced: learned association stays visible + labelled ─────

#[test]
fn t_cross_reinforced_shown_labeled() {
    let store = build_ab_store("t-cross-rein");
    // Deterministic reinforcement of EVERY cross edge out of A_state (the only
    // outgoing edges the seed has, by construction): explicit success edges
    // set (alpha, beta) = (5, 1), evidence 6.0. Whatever the Thompson-sampled
    // Dijkstra picks as its first hop, it is a reinforced cross edge.
    // (Same-experience B→B edges overwritten as a side effect stay
    // SameExperience — still visible, assertions unaffected.)
    store
        .handle
        .explicit_edge(store.a_state, store.b_state, store.b_action, true)
        .expect("explicit_edge 1 failed");
    store
        .handle
        .explicit_edge(store.a_state, store.b_action, store.b_outcome, true)
        .expect("explicit_edge 2 failed");
    store
        .handle
        .explicit_edge(store.a_state, store.b_outcome, store.b_action, true)
        .expect("explicit_edge 3 failed");
    let r = retrieve_a_state(&store);
    assert_eq!(
        r.seed_id,
        Some(store.a_state),
        "T-cross-reinforced: A_state (cos 1.0) must seed even after dense-core updates"
    );
    assert!(
        !r.causal_chain.is_empty(),
        "T-cross-reinforced: chain must form"
    );
    assert_eq!(
        r.causal_chain[0].link_kind,
        LinkKind::CrossExperienceLearned,
        "T-cross-reinforced: reinforced cross step is a learned association"
    );
    assert!(
        r.explanation.contains("learned association"),
        "T-cross-reinforced: learned label must render: {}",
        r.explanation
    );
    // Reinforced evidence must clear the eps band by construction, not luck.
    assert!(
        r.causal_chain[0].evidence > 2.0 + 1e-4,
        "T-cross-reinforced: evidence must clear the predicate band"
    );
}

// ── T-unknown: ledgerless steps render with the unverified label ─────────

#[test]
fn t_unknown_shown_labeled() {
    // Pure classifier checks (deterministic, no Thompson involved).
    let ledger = LearnLedger::new();
    assert_eq!(
        classify_link(&ledger, 1, 2, 2.0),
        LinkKind::Unknown,
        "T-unknown: no ledger entries → Unknown, never hidden"
    );
    assert_eq!(
        classify_link(&ledger, 1, 2, 9.5),
        LinkKind::Unknown,
        "T-unknown: evidence is irrelevant without membership"
    );
    // Same-experience and split behavior of the classifier itself.
    let mut full = LearnLedger::new();
    full.append(11, 12, 13, 100);
    full.append(11, 14, 15, 101);
    assert_eq!(
        classify_link(&full, 11, 12, 2.0),
        LinkKind::SameExperience,
        "T-unknown: shared episode → same even at prior evidence"
    );
    assert_eq!(
        classify_link(&full, 12, 14, 2.0),
        LinkKind::CrossExperienceUnverified,
        "T-unknown: different episodes at prior evidence → hidden class"
    );
    assert_eq!(
        classify_link(&full, 12, 14, 6.0),
        LinkKind::CrossExperienceLearned,
        "T-unknown: different episodes reinforced → learned class"
    );
}

// ── T-predicate: eps band units + aged-decay case ────────────────────────
// The rider: decay floors α/β at 0.01 (engine.rs update_causal_edge), NOT the
// 1.0 prior — an aged never-reinforced edge drifts 2.0 → 0.02 and must still
// count prior-only. Float equality would miss it; hence <= 2.0 + eps.

#[test]
fn t_predicate_units_and_aged_decay() {
    assert!(is_prior_only_evidence(2.0), "fresh prior counts");
    assert!(is_prior_only_evidence(1.5), "decayed-aging counts");
    assert!(is_prior_only_evidence(0.02), "decay floor counts");
    assert!(!is_prior_only_evidence(2.0002), "above the band is learned");
    assert!(!is_prior_only_evidence(6.0), "reinforced is learned");
}
