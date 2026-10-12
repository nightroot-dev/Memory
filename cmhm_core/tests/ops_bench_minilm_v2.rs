#![cfg(feature = "onnx")]
//! Session L benchmark, MiniLM leg: same L1 corpus, same four metrics.
//!
//! Corpus: tools/corpus_l1.json. Projection: data/projections/minilm-l6-v2-int8.bin.
//! Run:
//!   cargo +stable-x86_64-pc-windows-msvc test -p cmhm_core --target x86_64-pc-windows-msvc --features onnx --test ops_bench_minilm_v2 -- --ignored --nocapture

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::embed::{Embedder, OnnxEmbedder};
use cmhm_core::engine;
use cmhm_core::node_store::cosine;
use cmhm_core::projection::Projection;
use cmhm_core::NodeType;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::tempdir;

const MODEL: &str = "minilm-l6-v2-int8";
const PROJ_FILE: &str = "data/projections/minilm-l6-v2-int8.bin";

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
    for p in [
        PathBuf::from("tools/corpus_l1.json"),
        PathBuf::from("..").join("tools/corpus_l1.json"),
    ] {
        if p.exists() {
            return serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        }
    }
    panic!("corpus_l1.json not found");
}

fn projection_dir() -> Option<PathBuf> {
    [PathBuf::from("..").join(PROJ_FILE), PathBuf::from(PROJ_FILE)]
        .into_iter()
        .find(|p| p.exists())
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
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

#[test]
#[ignore]
fn ops_bench_minilm_v2() {
    let pairs = load_corpus();
    assert_eq!(pairs.len(), 200);
    let proj_dir = projection_dir().expect("projection missing");
    let emb = OnnxEmbedder::new().expect("onnx init");
    let proj_bytes = std::fs::read(
        proj_dir.join("minilm-l6-v2-int8.bin"),
    )
    .unwrap();
    let proj = Projection::from_bytes(&proj_bytes).unwrap();

    // WITHOUT: native 384-d + projected 128-d brute force (depths 8 / pool n/a).
    let s_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.situation).unwrap()).collect();
    let q_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.query).unwrap()).collect();
    let s_prj: Vec<Vec<f32>> = s_nat.iter().map(|v| proj.project(v).unwrap().to_vec()).collect();
    let q_prj: Vec<Vec<f32>> = q_nat.iter().map(|v| proj.project(v).unwrap().to_vec()).collect();
    let mut no_nat: Vec<Option<usize>> = vec![];
    let mut no_prj: Vec<Option<usize>> = vec![];
    for i in 0..pairs.len() {
        let mut sn: Vec<(usize, f32)> = s_nat.iter().enumerate().map(|(j, s)| (j, cosine(&q_nat[i], s))).collect();
        sn.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        sn.truncate(8);
        no_nat.push(sn.iter().position(|(j, _)| *j == i));
        let mut sp: Vec<(usize, f32)> = s_prj.iter().enumerate().map(|(j, s)| (j, cosine(&q_prj[i], s))).collect();
        sp.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        sp.truncate(8);
        no_prj.push(sp.iter().position(|(j, _)| *j == i));
    }

    // WITH: full engine.
    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = proj_dir;
    let handle = engine::start(cfg).unwrap();
    let mut t_state = vec![];
    let mut t_out = vec![];
    for (idx, p) in pairs.iter().enumerate() {
        let r = handle
            .learn_with_text_context(
                s_nat[idx].clone(),
                emb.embed(&format!("{} {idx}", p.action)).unwrap(),
                emb.embed(&format!("{} {idx}", p.outcome)).unwrap(),
                0.9, 0.5, MODEL, 0.5, String::new(), None,
                CallerRole::Agent, None, None,
                Some((p.situation.clone(), p.action.clone(), p.outcome.clone())),
            false,
            )
            .unwrap();
        t_state.push(r.state_node_id);
        t_out.push(r.outcome_node_id);
    }

    let key = |p: &Pair| {
        if p.kind == "lexical" {
            "lexical".to_string()
        } else {
            format!("para-{}", p.difficulty)
        }
    };
    let mut wd: HashMap<String, Agg> = HashMap::new();
    let mut wf: HashMap<String, Agg> = HashMap::new();
    let mut wn: HashMap<String, Agg> = HashMap::new();
    let mut wp: HashMap<String, Agg> = HashMap::new();
    // Stage attribution (WITH-fused only): where is each gold item lost?
    // candidate = target absent from raw pool; safety = present raw but
    // filtered/abstained; rank = present post-filter but seed wrong.
    let mut attr: HashMap<String, (usize, usize, usize, usize)> = HashMap::new();
    for (i, p) in pairs.iter().enumerate() {
        let k = key(p);
        wn.entry(k.clone()).or_default().add(no_nat[i] == Some(0), no_nat[i], false);
        wp.entry(k.clone()).or_default().add(no_prj[i] == Some(0), no_prj[i], false);
        let d = handle.retrieve(q_nat[i].clone(), MODEL, NodeType::State, "").unwrap();
        let f = handle.retrieve(q_nat[i].clone(), MODEL, NodeType::State, &p.query).unwrap();
        {
            let e = attr.entry(k.clone()).or_default();
            e.3 += 1;
            if f.seed_id == Some(t_state[i]) {
                // found: no loss to attribute (seed could still be right while
                // chain misses; that is a separate chain metric, not staged here)
            } else if !f.raw_nn_candidates.contains(&t_state[i]) {
                e.0 += 1; // lost at candidate generation
            } else if f.abstained || !f.post_filter_candidates.contains(&t_state[i]) {
                e.1 += 1; // lost at safety filtering / abstention
            } else {
                e.2 += 1; // lost at final ranking
            }
        }
        for (r, a) in [(&d, wd.entry(k.clone()).or_default()), (&f, wf.entry(k.clone()).or_default())] {
            // A hop must ARRIVE at the outcome (>= 1 edge); starting at it (seed == outcome) is a seed miss.
            let chain_ok = r.causal_chain.iter().any(|s| s.to_node == t_out[i]);
            a.add(
                r.seed_id == Some(t_state[i]),
                r.post_filter_candidates.iter().position(|&id| id == t_state[i]),
                chain_ok,
            );
        }
    }
    handle.shutdown().ok();

    println!();
    println!("## ops_bench_v2 (L1, MiniLM, n=200)");
    println!("| group          | seedP@1 | poolRec | chainAcc | MRR    | n    |");
    println!("|----------------|---------|---------|----------|--------|------|");
    for k in ["lexical", "para-easy", "para-medium", "para-hard"] {
        for (label, m) in [("WITHOUT-native", &wn), ("WITHOUT-proj", &wp), ("WITH-dense", &wd), ("WITH-fused", &wf)] {
            if let Some(a) = m.get(k) {
                println!("{}  {label}", a.row(k));
            }
        }
    }
    println!();
    println!("## stage attribution (WITH-fused misses only)");
    println!("| group          | lost_candidate | lost_safety | lost_rank | n_miss |");
    println!("|----------------|----------------|-------------|-----------|--------|");
    for k in ["lexical", "para-easy", "para-medium", "para-hard"] {
        if let Some((c, s, r, n)) = attr.get(k) {
            let miss = n - wf.get(k).map(|a| a.seed_p1).unwrap_or(0);
            println!("| {k:<14} | {c:>14} | {s:>11} | {r:>9} | {miss:>6} |");
        }
    }
}
