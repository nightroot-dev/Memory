//! Seed-type contract: a typed retrieve() must never return a seed of another node type,
//! whichever arm (dense or BM25) put the candidate into the pool. The dense arm is already
//! type-scoped; the BM25 arm used to be unfiltered, so a lexically matching Outcome or Action
//! node could enter the pool, win scoring, and be returned as the seed of a State request.
//!
//! Hash embedder, default features, fast: runs in the normal suite.

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::engine::{self, EngineHandle};
use cmhm_core::text_to_embedding;
use cmhm_core::NodeType;
use tempfile::tempdir;

const MODEL: &str = "hash-128d";

fn learn(h: &EngineHandle, state: &str, action: &str, outcome: &str) {
    h.learn_with_text_context(
        text_to_embedding(state).to_vec(),
        text_to_embedding(action).to_vec(),
        text_to_embedding(outcome).to_vec(),
        0.9, // force-birth so every node exists and is indexed
        0.5,
        MODEL,
        0.5,
        String::new(),
        None,
        CallerRole::Agent,
        None,
        None,
        Some((state.to_string(), action.to_string(), outcome.to_string())),
    )
    .unwrap();
}

fn engine_with_corpus() -> (EngineHandle, tempfile::TempDir) {
    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = tmp.path().join("projections");
    let h = engine::start(cfg).unwrap();
    learn(&h, "alpha beta gamma", "delta epsilon", "zebra quagga crossing river");
    learn(&h, "one two three", "four five", "six seven eight nine");
    learn(&h, "red green blue", "cyan magenta", "yellow orange purple pink");
    (h, tmp)
}

/// The query repeats an OUTCOME's text exactly, so the Outcome node is both the best BM25 hit
/// and the best cosine match. A State request must still return a State seed.
#[test]
fn state_request_never_returns_an_outcome_seed_via_bm25() {
    let (h, _tmp) = engine_with_corpus();
    let q = "zebra quagga crossing river";
    let r = h
        .retrieve(text_to_embedding(q).to_vec(), MODEL, NodeType::State, q)
        .unwrap();
    assert_eq!(
        r.seed_node_type,
        Some(NodeType::State),
        "a State-typed retrieve must not return a {:?} seed (seed_id={:?})",
        r.seed_node_type,
        r.seed_id
    );
    h.shutdown().ok();
}

/// Same contract for an ACTION text.
#[test]
fn state_request_never_returns_an_action_seed_via_bm25() {
    let (h, _tmp) = engine_with_corpus();
    let q = "delta epsilon";
    let r = h
        .retrieve(text_to_embedding(q).to_vec(), MODEL, NodeType::State, q)
        .unwrap();
    assert_eq!(r.seed_node_type, Some(NodeType::State), "seed_id={:?}", r.seed_id);
    h.shutdown().ok();
}

/// The filter must not over-restrict: asking for Outcome seeds still returns an Outcome.
#[test]
fn outcome_request_still_returns_an_outcome_seed() {
    let (h, _tmp) = engine_with_corpus();
    let q = "zebra quagga crossing river";
    let r = h
        .retrieve(text_to_embedding(q).to_vec(), MODEL, NodeType::Outcome, q)
        .unwrap();
    assert_eq!(r.seed_node_type, Some(NodeType::Outcome), "seed_id={:?}", r.seed_id);
    h.shutdown().ok();
}
