#![cfg(feature = "onnx")]
//! Dense-only MiniLM test: prove the store/query dims match.
//!
//! Learns a 384-dim MiniLM embedding, queries with a token-disjoint
//! paraphrase and EMPTY query_text (BM25 disabled), asserts the target
//! is in the retrieval pool.
//!
//! On the buggy code this test fails: the store holds 384-dim nodes,
//! retrieval filters on 128-dim queries, pool is empty.

use cmhm_core::embed::{Embedder, OnnxEmbedder};
use cmhm_core::engine;
use cmhm_core::config::Config;
use cmhm_core::authority::CallerRole;
use cmhm_core::NodeType;
use std::path::PathBuf;
use tempfile::tempdir;

const PROJ_FILE: &str = "data/projections/minilm-l6-v2-int8.bin";
const MODEL: &str = "minilm-l6-v2-int8";

fn projection_dir() -> Option<PathBuf> {
    [PathBuf::from("..").join(PROJ_FILE), PathBuf::from(PROJ_FILE)]
        .into_iter().find(|p| p.exists())
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

#[test]
fn minilm_dense_only_retrieval_finds_paraphrase() {
    let proj_dir = match projection_dir() {
        Some(p) => p,
        None => { eprintln!("SKIP: projection missing"); return; }
    };
    let embedder = OnnxEmbedder::new().expect("onnx init");
    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = proj_dir;
    let handle = engine::start(cfg).expect("engine start");

    // Target: unique situation, no shared tokens with the query.
    let target_sit = "the login service rejected every credential after the certificate authority rotation";
    let target_s_emb = embedder.embed(target_sit).unwrap();
    let target_a_emb = embedder.embed("renewed the CA bundle and restarted the service").unwrap();
    let target_o_emb = embedder.embed("logins resumed within two minutes").unwrap();
    let r = handle.learn_with_text_context(
        target_s_emb, target_a_emb, target_o_emb,
        0.2, 0.5, MODEL, 0.5, String::new(), None,
        CallerRole::User, None, None,
        Some((target_sit.into(), "renewed bundle".into(), "logins resumed".into())),
    ).expect("learn target");

    // Distractors: no shared tokens with the query.
    for d in [
        "the batch job ran later than scheduled",
        "metrics began reporting higher latency",
        "the file system filled to capacity",
        "two replicas fell out of sync overnight",
        "the feature flag toggled for the wrong user group",
    ] {
        let s = embedder.embed(d).unwrap();
        let a = embedder.embed("investigated").unwrap();
        let o = embedder.embed("resolved").unwrap();
        handle.learn(s, a, o, 0.5, 0.5, MODEL, 0.5, String::new(), None).expect("distractor");
    }

    // Query: paraphrase with ZERO shared content tokens.
    // None of: login, service, rejected, credential, certificate,
    // authority, rotation, CA, bundle, restarted, logins, resumed.
    let query_text = "users cannot sign in after we swapped the trusted issuer";
    let q_emb = embedder.embed(query_text).unwrap();

    // CRITICAL: query_text = "" so BM25 is disabled. Only dense runs.
    let result = handle.retrieve(q_emb, MODEL, NodeType::State, "").expect("retrieve");

    eprintln!("dense-only candidates: {:?}", result.post_filter_candidates);
    eprintln!("target state_id: {}", r.state_node_id);

    assert!(
        result.post_filter_candidates.contains(&r.state_node_id),
        "MiniLM dense-only retrieval must find the paraphrase; \
         store/query dim mismatch likely still present. \
         Got pool {:?}, target {}",
        result.post_filter_candidates, r.state_node_id
    );

    handle.shutdown().ok();
}
