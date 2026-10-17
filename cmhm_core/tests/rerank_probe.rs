#![cfg(feature = "onnx")]
//! B13 rerank PRE-REGISTERED probe (committed gates in tools/eval_b8/results.md):
//! rerank the EXISTING WITH-fused top-20 pool with a small cross-encoder.
//! No engine change, no locked files - test-scoped only.
//! PASS = net hard gain >= +10 of 30 pairs (net = recovered_from_pool - stolen).
//! Model pin: jina-reranker-v1-turbo-en. If net in (6..10): ONE follow-up with
//! BGERerankerBase (pre-registered follow-up, no third run without decision).
//!
//! Run:
//!   cargo +stable-x86_64-pc-windows-msvc test -p cmhm_core --target x86_64-pc-windows-msvc --features onnx --test rerank_probe -- --ignored --nocapture

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::embed::{Embedder, OnnxEmbedder};
use cmhm_core::engine;
use cmhm_core::NodeType;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::tempdir;

const MODEL: &str = "minilm-l6-v2-int8"; // the CURRENT serving embedder makes the pools
const PROJ_DIR_ROOT: &str = "data/projections";

#[derive(Debug, serde::Deserialize, Clone)]
struct Pair {
    id: String,
    kind: String,
    difficulty: String,
    situation: String,
    query: String,
    action: String,
    outcome: String,
}

fn load_corpus() -> Vec<Pair> {
    for p in [PathBuf::from("tools/corpus_l1.json"), PathBuf::from("..").join("tools/corpus_l1.json")] {
        if p.exists() {
            return serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        }
    }
    panic!("corpus_l1.json not found");
}

fn projection_dir() -> Option<PathBuf> {
    [PathBuf::from("..").join(PROJ_DIR_ROOT), PathBuf::from(PROJ_DIR_ROOT)]
        .into_iter()
        .find(|p| p.exists())
}

#[derive(Default)]
struct Bucket {
    n: usize,
    base_hit: usize,
    pool_miss: usize,  // target NOT in pool (rerank cannot help)
    recovered: usize,  // base miss + target in pool + rerank puts target first
    stolen: usize,     // base hit but rerank puts another pool candidate first
    rerank_hit: usize, // rerank top == target (net policy outcome in pool)
    ms: f64,
}

#[test]
#[ignore]
fn rerank_probe() {
    let pairs = load_corpus();
    assert_eq!(pairs.len(), 200);
    let proj_dir = projection_dir().expect("data/projections missing");
    let emb = OnnxEmbedder::new().expect("onnx init");

    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = proj_dir;
    let handle = engine::start(cfg).unwrap();
    let mut t_state = vec![];
    for p in pairs.iter() {
        let r = handle
            .learn_with_text_context(
                emb.embed(&p.situation).unwrap(),
                emb.embed(&format!("{} idx", p.action)).unwrap(),
                emb.embed(&format!("{} idx", p.outcome)).unwrap(),
                0.9, 0.5, MODEL, 0.5, String::new(), None,
                CallerRole::Agent, None, None,
                Some((p.situation.clone(), p.action.clone(), p.outcome.clone())),
                false,
            )
            .unwrap();
        t_state.push(r.state_node_id);
    }
    let mut text_of: HashMap<u64, &str> = HashMap::new();
    for (j, p) in pairs.iter().enumerate() {
        text_of.insert(t_state[j], p.situation.as_str());
    }

    let mut rr = fastembed::TextRerank::try_new(fastembed::RerankInitOptions::new(
        fastembed::RerankerModel::JINARerankerV1TurboEn,
    ))
    .expect("reranker init");

    let mut b: HashMap<&str, Bucket> = HashMap::new();
    for p in pairs.iter().filter(|p| p.kind != "lexical") {
        let bucket = b.entry(p.difficulty.as_str()).or_default();
        bucket.n += 1;
        let f = handle
            .retrieve(emb.embed(&p.query).unwrap(), MODEL, NodeType::State, &p.query)
            .unwrap();
        let pool = &f.post_filter_candidates;
        let target = t_state[pairs.iter().position(|x| x.id == p.id).unwrap()];
        let in_pool = pool.contains(&target);
        let docs: Vec<&str> = pool
            .iter()
            .map(|id| text_of.get(id).copied().unwrap_or(""))
            .collect();
        let n_unknown = docs.iter().filter(|d| d.is_empty()).count();

        let t0 = std::time::Instant::now();
        let scores = rr
            .rerank(p.query.as_str(), &docs, false, None)
            .expect("rerank");
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        bucket.ms += dt;
        let best_idx = scores
            .iter()
            .max_by(|a, x| a.score.partial_cmp(&x.score).unwrap())
            .map(|r| r.index);
        let rerank_pick = best_idx.and_then(|i| pool.get(i)).copied();

        let was_first = f.seed_id == Some(target);
        if was_first {
            bucket.base_hit += 1;
            if rerank_pick == Some(target) {
                bucket.rerank_hit += 1;
            } else {
                bucket.stolen += 1;
            }
        } else if in_pool {
            if rerank_pick == Some(target) {
                bucket.recovered += 1;
                bucket.rerank_hit += 1;
            }
        } else {
            bucket.pool_miss += 1;
        }
        let _ = n_unknown; // counted implicitly: unmapped pool ids rerank as empty text
    }
    handle.shutdown().ok();

    println!();
    println!("## B13 rerank probe (pool = WITH-fused top-{}, jina-reranker-v1-turbo-en)", 20);
    println!("| group  | n | base_hit | stolen | recovered | pool_miss(target not in pool) | net_hard | rerank ms/q |");
    println!("|--------|---|----------|--------|-----------|-------------------------------|----------|-------------|");
    let mut net_hard = 0;
    for k in ["easy", "medium", "hard"] {
        if let Some(bucket) = b.get(k) {
            let net = bucket.base_hit - bucket.stolen + bucket.recovered;
            if k == "hard" {
                net_hard = net;
            }
            println!(
                "| {:<6} | {:>1} | {:>8} | {:>6} | {:>9} | {:>29} | {:>8} | {:>11.1} |",
                k, bucket.n, bucket.base_hit, bucket.stolen, bucket.recovered, bucket.pool_miss, net, bucket.ms / bucket.n as f64
            );
        }
    }
    let pass = net_hard >= 12;
    println!();
    println!(
        "GATE (pre-registered): hard net {} >= 12 of 30 (+10 over baseline 2) -> {}",
        net_hard,
        if pass { "PASS" } else { "FAIL" }
    );
}
