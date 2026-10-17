#![cfg(feature = "onnx")]
//! B13 rerank PRE-REGISTERED probe (gates committed in tools/eval_b8/results.md):
//! rerank the EXISTING WITH-fused top-20 pool with a cross-encoder.
//! No engine change, no locked files - test-scoped only.
//! AUTHORIZING GATE (jina-reranker-v1-turbo-en arm):
//!   PASS = net hard gain >= +10 of 30 pairs (net = recovered - stolen).
//! Documented deviation: turbo arm returned net 0 (below the pre-registered
//! follow-up band 6..10 on the first commit of this probe); a single
//! BGERerankerBase arm runs to record the strong-reranker ceiling both as
//! instrument validation and embedder-spend decision evidence. No third arm
//! without a decision note.
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
    base_hit: usize,   // fused seed == target (rerank never sees better info)
    pool_miss: usize,  // target NOT in pool (rerank cannot help)
    recovered: usize,  // base miss + target in pool + rerank puts target first
    stolen: usize,     // base hit but rerank puts another pool candidate first
    rerank_hit: usize, // rerank top == target
    ms: f64,
}

struct Arm {
    name: &'static str,
    model: fastembed::RerankerModel,
    gate: bool,
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
    for (idx, p) in pairs.iter().enumerate() {
        let r = handle
            .learn_with_text_context(
                emb.embed(&p.situation).unwrap(),
                emb.embed(&format!("{} {idx}", p.action)).unwrap(),
                emb.embed(&format!("{} {idx}", p.outcome)).unwrap(),
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

    // Pools + seed status computed ONCE from the real fused retrieves (the pools
    // both arms rerank are the identical serving pipeline output).
    struct PoolInfo {
        i: usize,
        target: u64,
        pool: Vec<u64>,
        seed_hit: bool,
    }
    let mut pool_infos: Vec<PoolInfo> = vec![];
    for (i, p) in pairs.iter().enumerate() {
        if p.kind == "lexical" {
            continue;
        }
        let f = handle
            .retrieve(emb.embed(&p.query).unwrap(), MODEL, NodeType::State, &p.query)
            .unwrap();
        let seed_hit = f.seed_id == Some(t_state[i]);
        pool_infos.push(PoolInfo { i, target: t_state[i], pool: f.post_filter_candidates.clone(), seed_hit });
    }
    handle.shutdown().ok();

    let arms = [
        Arm { name: "jina-reranker-v1-turbo-en", model: fastembed::RerankerModel::JINARerankerV1TurboEn, gate: true },
        Arm { name: "BAAI/bge-reranker-base (deviation run: ceiling doc)", model: fastembed::RerankerModel::BGERerankerBase, gate: false },
    ];
    for a in &arms {
        let mut rr =
            fastembed::TextRerank::try_new(fastembed::RerankInitOptions::new(a.model.clone()))
                .expect("reranker init");
        let mut b: HashMap<&str, Bucket> = HashMap::new();
        for pi in &pool_infos {
            let p = &pairs[pi.i];
            let bucket = b.entry(p.difficulty.as_str()).or_default();
            bucket.n += 1;
            if pi.seed_hit {
                bucket.base_hit += 1;
            }
            let docs: Vec<&str> = pi.pool.iter().map(|id| text_of.get(id).copied().unwrap_or("")).collect();
            let t0 = std::time::Instant::now();
            let scores = rr.rerank(p.query.as_str(), &docs, false, None).expect("rerank");
            bucket.ms += t0.elapsed().as_secs_f64() * 1000.0;
            let best = scores.iter().max_by(|x, y| x.score.total_cmp(&y.score)).map(|r| r.index);
            let rerank_pick = best.and_then(|k| pi.pool.get(k)).copied();
            let in_pool = pi.pool.contains(&pi.target);
            if pi.seed_hit {
                if rerank_pick == Some(pi.target) {
                    bucket.rerank_hit += 1;
                } else {
                    bucket.stolen += 1;
                }
            } else if in_pool {
                if rerank_pick == Some(pi.target) {
                    bucket.recovered += 1;
                    bucket.rerank_hit += 1;
                }
            } else {
                bucket.pool_miss += 1;
            }
        }
        println!();
        println!("## B13 rerank probe (pool = WITH-fused top-20, reranker = {})", a.name);
        println!("| group  | n | base_hit | stolen | recovered | pool_miss | net | rerank ms/q |");
        println!("|--------|---|----------|--------|-----------|-----------|-----|-------------|");
        for k in ["easy", "medium", "hard"] {
            if let Some(bucket) = b.get(k) {
                let net = bucket.base_hit - bucket.stolen + bucket.recovered;
                println!(
                    "| {:<6} | {:>1} | {:>8} | {:>6} | {:>9} | {:>9} | {:>3} | {:>11.1} |",
                    k,
                    bucket.n,
                    bucket.base_hit,
                    bucket.stolen,
                    bucket.recovered,
                    bucket.pool_miss,
                    net,
                    bucket.ms / bucket.n as f64
                );
            }
        }
        if a.gate {
            let hard = b.get("hard").unwrap();
            let net = hard.base_hit - hard.stolen + hard.recovered;
            let pass = net >= 12;
            println!();
            println!(
                "GATE (pre-registered, {}): hard net {} >= 12 of 30 (+10 over baseline 2) -> {}",
                a.name,
                net,
                if pass { "PASS" } else { "FAIL" }
            );
        } else {
            let hard = b.get("hard").unwrap();
            let net = hard.base_hit - hard.stolen + hard.recovered;
            println!();
            println!("CEILING DOC (deviation run, not the gate): hard net {} of 30", net);
        }
    }
}
