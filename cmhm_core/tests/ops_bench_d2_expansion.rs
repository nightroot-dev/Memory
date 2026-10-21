#![cfg(feature = "onnx")]
//! D2 — query-expansion bench (pre-registered: results.md reviewer deltas
//! verbatim + tools/eval_b8/d2-query-expansion.md addendum BEFORE any run).
//!
//! Two legs in one instrument, same learn calls as the MiniLM record
//! (NO keywords anywhere):
//!   - control leg: plain record baseline through this clone (drift check).
//!   - expanded leg: non-hard groups identical to control; HARD items run
//!     4 self-contained fused retrievals each (original query + 3 causes),
//!     merged metrics per the addendum:
//!       merged seedP@1 = gold is seed in ANY run (gate: >= 21/30)
//!       merged poolRec = gold in ANY run's post_filter slots (fail w/o rank
//!                        work if < 21/30 — registered optic)
//!       merged MRR     = best 1/(rank+1) across runs
//!       merged attribution: candidate if gold outside union(raw_nn),
//!         safety if in raw-union but outside post-filter-union, rank else.
//!
//! Corpus: tools/eval_d2/corpus_l1_d2.json (mechanical coupling of the 30
//! hard items with the committed subagent expansions).
//!
//! Run:
//!   cargo +stable-x86_64-pc-windows-msvc test -p cmhm_core --target x86_64-pc-windows-msvc --features onnx --test ops_bench_d2_expansion -- --ignored --nocapture

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
    #[serde(default)]
    causes: Vec<String>,
}

fn load_corpus() -> Vec<Pair> {
    for p in [
        PathBuf::from("tools/eval_d2/corpus_l1_d2.json"),
        PathBuf::from("..").join("tools/eval_d2/corpus_l1_d2.json"),
    ] {
        if p.exists() {
            return serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        }
    }
    panic!("corpus_l1_d2.json not found");
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

fn run_leg(expanded: bool) {
    let pairs = load_corpus();
    assert_eq!(pairs.len(), 200);
    let hard_count = pairs.iter().filter(|p| p.difficulty == "hard").count();
    assert_eq!(hard_count, 30);
    if expanded {
        for (i, p) in pairs.iter().enumerate() {
            if p.difficulty == "hard" {
                assert_eq!(p.causes.len(), 3, "hard item {} without 3 causes", p.id);
            } else {
                assert!(p.causes.is_empty(), "unexpected causes on {}", p.id);
            }
            let _ = i;
        }
    }

    let proj_dir = projection_dir().expect("projection missing");
    let emb = OnnxEmbedder::new().expect("onnx init");
    let proj_bytes = std::fs::read(proj_dir.join("minilm-l6-v2-int8.bin")).unwrap();
    let proj = Projection::from_bytes(&proj_bytes).unwrap();

    let s_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.situation).unwrap()).collect();
    let q_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.query).unwrap()).collect();
    let s_prj: Vec<Vec<f32>> = s_nat.iter().map(|v| proj.project(v).unwrap().to_vec()).collect();
    let q_prj: Vec<Vec<f32>> = q_nat.iter().map(|v| proj.project(v).unwrap().to_vec()).collect();
    let mut no_nat: Vec<Option<usize>> = vec![];
    let mut no_prj: Vec<Option<usize>> = vec![];
    for i in 0..pairs.len() {
        // WITHOUT legs are dense-only and see no expansion either way.
        let mut sn: Vec<(usize, f32)> = s_nat.iter().enumerate().map(|(j, s)| (j, cosine(&q_nat[i], s))).collect();
        sn.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        sn.truncate(8);
        no_nat.push(sn.iter().position(|(j, _)| *j == i));
        let mut sp: Vec<(usize, f32)> = s_prj.iter().enumerate().map(|(j, s)| (j, cosine(&q_prj[i], s))).collect();
        sp.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        sp.truncate(8);
        no_prj.push(sp.iter().position(|(j, _)| *j == i));
    }

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
            } else if !f.raw_nn_candidates.contains(&t_state[i]) {
                e.0 += 1;
            } else if f.abstained || !f.post_filter_candidates.contains(&t_state[i]) {
                e.1 += 1;
            } else {
                e.2 += 1;
            }
        }
        for (r, a) in [(&d, wd.entry(k.clone()).or_default()), (&f, wf.entry(k.clone()).or_default())] {
            let chain_ok = r.causal_chain.iter().any(|s| s.to_node == t_out[i]);
            a.add(
                r.seed_id == Some(t_state[i]),
                r.post_filter_candidates.iter().position(|&id| id == t_state[i]),
                chain_ok,
            );
        }
    }

    // merged metrics for the expanded leg (hard items only)
    let mut merged: HashMap<String, Agg> = HashMap::new();
    let mut merged_attr: HashMap<String, (usize, usize, usize, usize)> = HashMap::new();
    if expanded {
        for (i, p) in pairs.iter().enumerate() {
            if p.difficulty != "hard" {
                continue;
            }
            let k = "para-hard".to_string();
            let mut texts: Vec<(Vec<f32>, &str)> = vec![(q_nat[i].clone(), p.query.as_str())];
            for c in &p.causes {
                texts.push((emb.embed(c).unwrap(), c.as_str()));
            }
            let mut any_seed = false;
            let mut raw_union_hit = false;
            let mut pf_union_rank: Option<usize> = None;
            let mut chain_any = false;
            for (emb_v, text) in &texts {
                let r = handle.retrieve(emb_v.clone(), MODEL, NodeType::State, *text).unwrap();
                if r.seed_id == Some(t_state[i]) {
                    any_seed = true;
                }
                if r.causal_chain.iter().any(|s| s.to_node == t_out[i]) {
                    chain_any = true;
                }
                if r.raw_nn_candidates.contains(&t_state[i]) {
                    raw_union_hit = true;
                }
                let rr = r.post_filter_candidates.iter().position(|&id| id == t_state[i]);
                if let Some(rank) = rr {
                    pf_union_rank = Some(pf_union_rank.map_or(rank, |b: usize| b.min(rank)));
                }
            }
            let m = merged.entry(k.clone()).or_default();
            m.add(any_seed, pf_union_rank, chain_any);
            {
                let e = merged_attr.entry(k.clone()).or_default();
                e.3 += 1;
                if any_seed {
                } else if !raw_union_hit {
                    e.0 += 1;
                } else if pf_union_rank.is_none() {
                    e.1 += 1;
                } else {
                    e.2 += 1;
                }
            }
        }
    }

    handle.shutdown().ok();

    let leg_name = if expanded { "d2_expanded" } else { "d2_control" };
    println!();
    println!("## ops_bench_d2_expansion leg={} (L1, MiniLM, n=200)", leg_name);
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
    println!("## stage attribution (WITH-fused misses only, leg={leg_name})");
    println!("| group          | lost_candidate | lost_safety | lost_rank | n_miss |");
    println!("|----------------|----------------|-------------|-----------|--------|");
    for k in ["lexical", "para-easy", "para-medium", "para-hard"] {
        if let Some((c, s, r, n)) = attr.get(k) {
            let miss = n - wf.get(k).map(|a| a.seed_p1).unwrap_or(0);
            println!("| {k:<14} | {c:>14} | {s:>11} | {r:>9} | {miss:>6} |");
        }
    }
    let tot_wf: usize = wf.values().map(|a| a.seed_p1).sum();
    let tot_pool: usize = wf.values().map(|a| a.pool_hit).sum();
    let hard_wf = wf.get("para-hard").unwrap();
    let lex_wf = wf.get("lexical").unwrap();
    println!(
        "STANDING (informational only for D2; the verdict gate is the merged line) leg={leg_name}: \
         lexical_guard={}/100 hard_seed={}/30 pool={}/200 overall_R1={}/200 \
         (record drift check: 100, 2, 164, 114)",
        lex_wf.seed_p1, hard_wf.seed_p1, tot_pool, tot_wf
    );
    if expanded {
        println!();
        println!("## MERGED (4 self-contained runs per hard item) leg={leg_name}");
        println!("| group          | seedP@1 | poolRec | chainAcc | MRR    | n    |");
        println!("|----------------|---------|---------|----------|--------|------|");
        for k in ["para-hard"] {
            if let Some(a) = merged.get(k) {
                println!("{}  merged-fused", a.row(k));
            }
        }
        println!();
        println!("## MERGED stage attribution (misses after best-of-runs)");
        println!("| group          | lost_candidate | lost_safety | lost_rank | n_miss |");
        println!("|----------------|----------------|-------------|-----------|--------|");
        for k in ["para-hard"] {
            if let Some((c, s, r, _n)) = merged_attr.get(k) {
                let miss = c + s + r;
                println!("| {k:<14} | {c:>14} | {s:>11} | {r:>9} | {miss:>6} |");
            }
        }
        if let Some(m) = merged.get("para-hard") {
            println!();
            println!(
                "D2 GATES: hard_merged_seedP@1={}/30 (register: >=21/30 is the ONLY pass); \
                 hard_merged_poolRec={}/30 (register: <21/30 fails without rank work)",
                m.seed_p1, m.pool_hit
            );
        }
    }
}

#[test]
#[ignore]
fn d2_control_leg() {
    run_leg(false);
}

#[test]
#[ignore]
fn d2_expanded_leg() {
    run_leg(true);
}
