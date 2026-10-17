#![cfg(feature = "onnx")]
//! B13 diagnosis scratch: one REAL hard corpus pair through the same learn +
//! fused-retrieve + rerank flow as rerank_probe, full pool score table printed,
//! target marked. Plus MiniLM cosine for comparison. Scratch, delete after.

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::embed::{Embedder, OnnxEmbedder};
use cmhm_core::engine;
use cmhm_core::NodeType;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::tempdir;

const MODEL: &str = "minilm-l6-v2-int8";

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

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|y| y * y).sum::<f32>().sqrt();
    dot / (na * nb).max(1e-9)
}

#[test]
#[ignore]
fn rerank_hard_case() {
    let pairs = load_corpus();
    let emb = OnnxEmbedder::new().unwrap();
    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = [PathBuf::from("../data/projections"), PathBuf::from("data/projections")]
        .into_iter()
        .find(|p| p.exists())
        .expect("projections dir");
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
    let mut text_of: HashMap<u64, String> = HashMap::new();
    for (j, p) in pairs.iter().enumerate() {
        text_of.insert(t_state[j], p.situation.clone());
    }

    // first hard pair whose target sits in the fused pool
    let mut rr = fastembed::TextRerank::try_new(fastembed::RerankInitOptions::new(
        fastembed::RerankerModel::JINARerankerV1TurboEn,
    ))
    .unwrap();
    for (i, p) in pairs.iter().enumerate() {
        if p.kind != "lexical" && p.difficulty == "hard" {
            let f = handle.retrieve(emb.embed(&p.query).unwrap(), MODEL, NodeType::State, &p.query).unwrap();
            let pool = f.post_filter_candidates.clone();
            if !pool.contains(&t_state[i]) {
                continue; // not usable for this diagnosis
            }
            println!("PAIR {i} id={}", p.id);
            println!("QUERY: {}", p.query);
            println!("TARGET-GOLD SITUATION: {}", p.situation);
            let docs: Vec<&str> = pool
                .iter()
                .map(|id| text_of.get(id).map(|s| s.as_str()).unwrap_or(""))
                .collect();
            let qemb = emb.embed(&p.query).unwrap();
            let s = rr.rerank(p.query.as_str(), &docs, false, None).unwrap();
            println!("-- pool ({}) as scored by jina turbo (desc) --", pool.len());
            for (rk, x) in s.iter().enumerate().take(10) {
                let is_target = pool[x.index] == t_state[i];
                let c = cosine(&qemb, &emb.embed(docs[x.index]).unwrap());
                println!(
                    "#{:>2} score={:>8.4} cos={:>6.4} {} {:?}",
                    rk, x.score, c, if is_target { "<== TARGET" } else { "         " }, docs[x.index]
                );
            }
            let tgt_rank = s.iter().position(|x| pool[x.index] == t_state[i]).unwrap();
            println!("TARGET rerank-rank = {} of {}", tgt_rank, pool.len());
            break;
        }
    }
    handle.shutdown().ok();
}
