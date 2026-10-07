//! Session L benchmark: WITHOUT vs WITH CMHM on the 200-pair L1 corpus.
//!
//! Corpus: tools/corpus_l1.json (single source of truth, embedder-free).
//! Metrics per protocol (docs/CORPUS_PROTOCOL.md): seed P@1, pool recall,
//! chain accuracy (chain reaches the target OUTCOME node), MRR — split by
//! lexical / paraphrase-easy / paraphrase-medium / paraphrase-hard.
//!
//! This file uses the hash embedder (runs everywhere). MiniLM/BGE runs use
//! the same corpus via their own harnesses.
//!
//! Run:
//!   cargo +stable-x86_64-pc-windows-gnu test -p cmhm_core --test ops_bench_v2 -- --ignored --nocapture

use cmhm_core::authority::CallerRole;
use cmhm_core::bm25::Bm25Index;
use cmhm_core::config::Config;
use cmhm_core::engine;
use cmhm_core::node_store::cosine;
use cmhm_core::text_to_embedding;
use cmhm_core::NodeType;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::tempdir;

const MODEL: &str = "hash-128d";

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
    let candidates = [
        PathBuf::from("tools/corpus_l1.json"),
        PathBuf::from("..").join("tools/corpus_l1.json"),
    ];
    for p in candidates {
        if p.exists() {
            let text = std::fs::read_to_string(&p).unwrap();
            return serde_json::from_str(&text).unwrap();
        }
    }
    panic!("corpus_l1.json not found (run from repo root or cmhm_core/)");
}

#[derive(Default)]
struct Agg {
    n: usize,
    seed_p1: usize,
    pool_hit: usize,
    chain_ok: usize,
    mrr: f64,
}

impl Agg {
    fn add(&mut self, seed_hit: bool, rank: Option<usize>, chain_ok: bool) {
        self.n += 1;
        if seed_hit {
            self.seed_p1 += 1;
        }
        if let Some(r) = rank {
            self.pool_hit += 1;
            self.mrr += 1.0 / (r as f64 + 1.0);
        }
        if chain_ok {
            self.chain_ok += 1;
        }
    }
    fn row(&self, name: &str) -> String {
        let n = self.n as f64;
        format!(
            "| {:<14} | {:>6.4} | {:>6.4} | {:>6.4} | {:>6.4} | {:>4} |",
            name,
            self.seed_p1 as f64 / n,
            self.pool_hit as f64 / n,
            self.chain_ok as f64 / n,
            self.mrr / n,
            self.n
        )
    }
}

fn chain_reaches_outcome(chain: &[cmhm_core::retrieval::ChainStep], outcome_id: u64) -> bool {
    chain.iter().any(|s| s.from_node == outcome_id || s.to_node == outcome_id)
}

#[test]
#[ignore]
fn ops_bench_v2_honest() {
    let pairs = load_corpus();
    assert_eq!(pairs.len(), 200, "L1 corpus must have 200 pairs");

    // ---- WITHOUT CMHM: flat brute force (dense) + pure BM25 ----
    let s_embs: Vec<Vec<f32>> = pairs
        .iter()
        .map(|p| text_to_embedding(&p.situation).to_vec())
        .collect();
    let mut no_dense_ranks: Vec<Option<usize>> = Vec::with_capacity(pairs.len());
    for (i, p) in pairs.iter().enumerate() {
        let q = text_to_embedding(&p.query).to_vec();
        let mut scored: Vec<(usize, f32)> = s_embs
            .iter()
            .enumerate()
            .map(|(j, s)| (j, cosine(&q, s)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scored.truncate(8); // same depth as the engine's dense pool
        no_dense_ranks.push(scored.iter().position(|(j, _)| *j == i));
    }
    let mut bidx = Bm25Index::new();
    for (i, p) in pairs.iter().enumerate() {
        bidx.insert(i as u64, &p.situation);
    }
    let mut no_bm25_ranks: Vec<Option<usize>> = Vec::with_capacity(pairs.len());
    for (i, p) in pairs.iter().enumerate() {
        let res = bidx.query(&p.query, 50); // same depth as the engine's BM25 arm
        no_bm25_ranks.push(res.iter().position(|(id, _)| *id == i as u64));
    }

    // ---- WITH CMHM: full engine ----
    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = tmp.path().join("projections");
    let handle = engine::start(cfg).unwrap();

    let mut target_state = Vec::with_capacity(pairs.len());
    let mut target_outcome = Vec::with_capacity(pairs.len());
    for (idx, p) in pairs.iter().enumerate() {
        let r = handle
            .learn_with_text_context(
                text_to_embedding(&p.situation).to_vec(),
                text_to_embedding(&format!("{} {idx}", p.action)).to_vec(),
                text_to_embedding(&format!("{} {idx}", p.outcome)).to_vec(),
                0.9,
                0.5,
                MODEL,
                0.5,
                String::new(),
                None,
                CallerRole::Agent,
                None,
                None,
                Some((p.situation.clone(), p.action.clone(), p.outcome.clone())),
            )
            .unwrap();
        target_state.push(r.state_node_id);
        target_outcome.push(r.outcome_node_id);
    }

    // group -> indices
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, p) in pairs.iter().enumerate() {
        let key = if p.kind == "lexical" {
            "lexical".to_string()
        } else {
            format!("para-{}", p.difficulty)
        };
        groups.entry(key).or_default().push(i);
    }

    let mut with_dense: HashMap<String, Agg> = HashMap::new();
    let mut with_fused: HashMap<String, Agg> = HashMap::new();
    for (i, p) in pairs.iter().enumerate() {
        let key = if p.kind == "lexical" {
            "lexical".to_string()
        } else {
            format!("para-{}", p.difficulty)
        };
        let q_emb = text_to_embedding(&p.query).to_vec();
        let d = handle.retrieve(q_emb.clone(), MODEL, NodeType::State, "").unwrap();
        let f = handle.retrieve(q_emb, MODEL, NodeType::State, &p.query).unwrap();
        for (res, agg) in [
            (&d, with_dense.entry(key.clone()).or_default()),
            (&f, with_fused.entry(key.clone()).or_default()),
        ] {
            let seed_hit = res.seed_id == Some(target_state[i]);
            let rank = res
                .post_filter_candidates
                .iter()
                .position(|&id| id == target_state[i]);
            let chain_ok = chain_reaches_outcome(&res.causal_chain, target_outcome[i]);
            agg.add(seed_hit, rank, chain_ok);
        }
    }
    handle.shutdown().ok();

    // WITHOUT aggregates from the same groups.
    let mut no_dense: HashMap<String, Agg> = HashMap::new();
    let mut no_bm25: HashMap<String, Agg> = HashMap::new();
    for (key, idxs) in &groups {
        for &i in idxs {
            // WITHOUT has no seed/chain (flat list): seed P@1 == pool P@1, chain n/a.
            let r = no_dense_ranks[i];
            no_dense.entry(key.clone()).or_default().add(
                r == Some(0),
                r,
                false,
            );
            let r = no_bm25_ranks[i];
            no_bm25.entry(key.clone()).or_default().add(
                r == Some(0),
                r,
                false,
            );
        }
    }

    let order = ["lexical", "para-easy", "para-medium", "para-hard"];
    println!();
    println!("## ops_bench_v2 (L1, hash-128d, n=200; chain n/a for WITHOUT)");
    println!("| group          | seedP@1 | poolRec | chainAcc | MRR    | n    |");
    println!("|----------------|---------|---------|----------|--------|------|");
    for key in order {
        if let Some(a) = no_dense.get(key) {
            println!("{}  WITHOUT-dense", a.row(key));
        }
        if let Some(a) = no_bm25.get(key) {
            println!("{}  WITHOUT-bm25", a.row(key));
        }
        if let Some(a) = with_dense.get(key) {
            println!("{}  WITH-dense", a.row(key));
        }
        if let Some(a) = with_fused.get(key) {
            println!("{}  WITH-fused", a.row(key));
        }
    }
}
