//! End-to-end MiniLM test: real 384-dim embeddings through the trained
//! 384 -> 128 PCA projection into 128-dim storage, and back out through
//! retrieval.
//!
//! Gated on `onnx` feature — default builds skip this file.
//! Requires data/projections/minilm-l6-v2-int8.bin to exist.
//!
//! Run (MSVC override; the project config pins gnu target globally):
//!   cargo +stable-x86_64-pc-windows-msvc --target x86_64-pc-windows-msvc \
//!     test -p cmhm_core --features onnx --test minilm_e2e -- --nocapture

#![cfg(feature = "onnx")]

use cmhm_core::embed::{Embedder, OnnxEmbedder};
use cmhm_core::engine;
use cmhm_core::config::Config;
use cmhm_core::authority::CallerRole;
use cmhm_core::NodeType;
use tempfile::tempdir;

const MODEL_ID: &str = "minilm-l6-v2-int8";
const PROJ_FILE: &str = "data/projections/minilm-l6-v2-int8.bin";

/// Locate the projection file relative to the crate root. Cargo runs
/// tests with CWD = cmhm_core/, so we try both "../" and "".
fn projection_dir() -> Option<std::path::PathBuf> {
    let candidates = [
        std::path::PathBuf::from("..").join(PROJ_FILE),
        std::path::PathBuf::from(PROJ_FILE),
    ];
    for c in candidates {
        if c.exists() {
            return c.parent().map(|p| p.to_path_buf());
        }
    }
    None
}

#[test]
fn minilm_paraphrase_retrieval_end_to_end() {
    let proj_dir = match projection_dir() {
        Some(p) => p,
        None => {
            eprintln!(
                "SKIP: {} not found. Train it first via tools/train_minilm_projection.py",
                PROJ_FILE
            );
            return;
        }
    };
    eprintln!("[F.2] projection dir: {}", proj_dir.display());

    let embedder = match OnnxEmbedder::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("SKIP: OnnxEmbedder init failed: {e}");
            return;
        }
    };
    assert_eq!(embedder.dim(), 384, "MiniLM must produce 384-dim vectors");

    let tmp = tempdir().expect("tempdir");
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = proj_dir;

    let handle = engine::start(cfg).expect("engine start");
    eprintln!("[F.2] engine started");

    let target_state_text = "database connection pool exhausted under peak load";
    let target_action_text = "increased pool size from 20 to 50";
    let target_outcome_text = "connection wait times dropped to under 5ms";

    let s_emb = embedder.embed(target_state_text).expect("embed target state");
    let a_emb = embedder.embed(target_action_text).expect("embed target action");
    let o_emb = embedder.embed(target_outcome_text).expect("embed target outcome");
    assert_eq!(s_emb.len(), 384);

    let learn_result = handle
        .learn_with_text_context(
            s_emb.clone(),
            a_emb,
            o_emb,
            0.2,
            0.5,
            MODEL_ID,
            0.5,
            String::new(),
            None,
            CallerRole::User,
            None,
            None,
            Some((
                target_state_text.to_string(),
                target_action_text.to_string(),
                target_outcome_text.to_string(),
            )),
        )
        .expect("learn target");
    eprintln!(
        "[F.2] target learned: state_id={} action_id={} outcome_id={}",
        learn_result.state_node_id, learn_result.action_node_id, learn_result.outcome_node_id
    );
    let target_state_id = learn_result.state_node_id;

    let distractors = [
        "kubernetes pod crash loop after config change",
        "user authentication fails on expired JWT token",
        "disk usage on primary volume reached 95 percent",
        "redis cache eviction rate spiked overnight",
        "message queue backlog grew past 100k messages",
        "TLS handshake latency increased on the ingress",
        "background job scheduler missed two hourly runs",
        "log aggregator dropped events during rotation",
        "container image pull slowed due to registry throttling",
    ];
    for (i, d) in distractors.iter().enumerate() {
        let s = embedder.embed(d).expect("embed distractor");
        let a = embedder.embed("investigated the anomaly").expect("embed");
        let o = embedder.embed("issue resolved or escalated").expect("embed");
        handle
            .learn(s, a, o, 0.5, 0.5, MODEL_ID, 0.5, String::new(), None)
            .expect("learn distractor");
        eprintln!("[F.2] distractor {} learned: {}", i + 1, d);
    }

    assert!(handle.node_count() >= 10, "at least 10 nodes expected");

    let query_text = "why are database connections timing out";
    let q_emb = embedder.embed(query_text).expect("embed query");
    eprintln!("[F.2] query: '{}'", query_text);

    let result = handle
        .retrieve(q_emb, MODEL_ID, NodeType::State, query_text)
        .expect("retrieve");

    eprintln!(
        "[F.2] retrieve: seed_id={:?} seed_prov={} candidates={:?}",
        result.seed_id, result.seed_provenance, result.post_filter_candidates
    );

    assert!(
        result.post_filter_candidates.contains(&target_state_id),
        "target state {} must appear in candidate pool {:?}; \
         MiniLM + PCA failed to surface the paraphrase match",
        target_state_id, result.post_filter_candidates
    );

    handle.shutdown().ok();
    eprintln!("[F.2] PASS");
}

#[test]
fn minilm_projection_roundtrip_shape() {
    let proj_dir = match projection_dir() {
        Some(p) => p,
        None => {
            eprintln!("SKIP: projection file not present");
            return;
        }
    };
    let path = proj_dir.join("minilm-l6-v2-int8.bin");
    let bytes = std::fs::read(&path).expect("read proj");
    let proj = cmhm_core::projection::Projection::from_bytes(&bytes)
        .expect("parse proj");
    assert_eq!(proj.input_dim, 384);

    let mut v = vec![0.0f32; 384];
    for (i, x) in v.iter_mut().enumerate() {
        *x = ((i as f32) * 0.13).sin();
    }
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter_mut().for_each(|x| *x /= norm);

    let out = proj.project(&v).expect("project");
    let out_norm: f32 = out.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((out_norm - 1.0).abs() < 1e-4, "output must be unit-norm");
    eprintln!("[F.2] projection round-trip OK");
}
