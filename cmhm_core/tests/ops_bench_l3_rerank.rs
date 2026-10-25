#![cfg(feature = "onnx")]
//! L3 — lever 3: bigger embedder + LLM rerank.
//! Pre-registered: tools/eval_b8/l3-llm-rerank.md (committed BEFORE any run).
//!
//! Tests (all #[ignore]; run from repo root, MSVC+onnx):
//!   l3a_bge_embedder_leg        - record harness body, bge-base-en-v1.5.
//!   l3b_dump_record_hard        - fused pools, record learn, 30 hard items.
//!   l3c_dump_d2expanded_para    - easy/medium record pools + hard D2 merged.
//!   l3d_dump_w1bridge_all       - W1a keyword-bridge pools (100 para items).
//!   b12r_dump_agent_texts       - B12-R agent-written memory pools (W/D arms).
//!   l3b_score_record_hard       - score tools/eval_l3/rerank_record_hard.json.
//!   l3c_score_d2expanded        - score tools/eval_l3/rerank_d2expanded.json.
//!   l3d_score_w1bridge          - score tools/eval_l3/rerank_w1bridge.json.
//!
//! Dump files fix the pool frame; score tests NEVER touch the engine.
//! Lexical groups are BYPASSED at the record's 100/100 (pre-registered).

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::embed::{Embedder, OnnxEmbedder};
use cmhm_core::engine::{self, EngineHandle};
use cmhm_core::node_store::cosine;
use cmhm_core::projection::Projection;
use cmhm_core::NodeType;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::tempdir;

const CAP: usize = 20;
const LEX_SEED: usize = 100; // record lexical fused seed (1.00 of 100)
const LEX_POOL: usize = 100; // record lexical fused pool (1.00 of 100)

#[derive(Debug, serde::Deserialize, Clone)]
struct BaseItem {
    id: String,
    kind: String,
    difficulty: String,
    situation: String,
    query: String,
    action: String,
    outcome: String,
    #[serde(default)]
    causes: Vec<String>,
    #[serde(default)]
    kw_transcript: String,
}

fn load_corpus(path: &str) -> Vec<BaseItem> {
    for p in [PathBuf::from(path), PathBuf::from("..").join(path)] {
        if p.exists() {
            return serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        }
    }
    panic!("{path} not found");
}

fn out_path(name: &str) -> PathBuf {
    for base in [PathBuf::from("tools/eval_l3"), PathBuf::from("..").join("tools/eval_l3")] {
        if base.exists() {
            return base.join(name);
        }
    }
    panic!("tools/eval_l3 not found");
}

fn projection_dir() -> PathBuf {
    const PROJ: &str = "data/projections";
    for p in [PathBuf::from("..").join(PROJ), PathBuf::from(PROJ)] {
        if p.exists() {
            return p;
        }
    }
    panic!("data/projections not found");
}

/// (embedder, model_id, projection file stem) — bge = the "bigger embedder".
fn make_embedder(which: &str) -> (OnnxEmbedder, &'static str, &'static str) {
    match which {
        "bge" => (
            OnnxEmbedder::bge().expect("bge init"),
            "bge-base-en-v1.5",
            "bge-base-en-v1.5.bin",
        ),
        _ => (
            OnnxEmbedder::new().expect("onnx init"),
            "minilm-l6-v2-int8",
            "minilm-l6-v2-int8.bin",
        ),
    }
}

fn group_key(p: &BaseItem) -> String {
    if p.kind == "lexical" {
        "lexical".to_string()
    } else {
        format!("para-{}", p.difficulty)
    }
}

#[derive(Default)]
struct Group {
    n: usize,
    seed_p1: usize,
    pool_hit: usize,
    chain_ok: usize,
    mrr: f64,
}

impl Group {
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

// ---------------------------------------------------------------- L3a ----

/// Record harness body (plain corpus, byte-identical learn treatment) with
/// the embedder swapped. Judged on its own numbers vs the standing gates.
fn l3a_bge_leg() {
    let pairs = load_corpus("tools/corpus_l1.json");
    assert_eq!(pairs.len(), 200);
    assert_eq!(pairs.iter().filter(|p| p.difficulty == "hard").count(), 30);

    let (emb, model, proj_file) = make_embedder("bge");
    let pdir = projection_dir();
    let proj = Projection::from_bytes(&std::fs::read(pdir.join(proj_file)).unwrap()).unwrap();

    let s_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.situation).unwrap()).collect();
    let q_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.query).unwrap()).collect();
    let s_prj: Vec<Vec<f32>> = s_nat.iter().map(|v| proj.project(v).unwrap().to_vec()).collect();
    let q_prj: Vec<Vec<f32>> = q_nat.iter().map(|v| proj.project(v).unwrap().to_vec()).collect();
    let mut without: HashMap<String, (Group, Group)> = HashMap::new(); // (native, proj)
    for i in 0..pairs.len() {
        let k = group_key(&pairs[i]);
        let mut sn: Vec<(usize, f32)> =
            s_nat.iter().enumerate().map(|(j, s)| (j, cosine(&q_nat[i], s))).collect();
        sn.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        sn.truncate(8);
        let e = without.entry(k.clone()).or_default();
        e.0.add(sn.first().map_or(false, |(j, _)| *j == i), sn.iter().position(|(j, _)| *j == i), false);
        let mut sp: Vec<(usize, f32)> =
            s_prj.iter().enumerate().map(|(j, s)| (j, cosine(&q_prj[i], s))).collect();
        sp.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        sp.truncate(8);
        e.1.add(sp.first().map_or(false, |(j, _)| *j == i), sp.iter().position(|(j, _)| *j == i), false);
    }

    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = pdir;
    let handle = engine::start(cfg).unwrap();
    let mut t_state = vec![];
    let mut t_out = vec![];
    for (idx, p) in pairs.iter().enumerate() {
        let r = handle
            .learn_with_text_context(
                s_nat[idx].clone(),
                emb.embed(&format!("{} {idx}", p.action)).unwrap(),
                emb.embed(&format!("{} {idx}", p.outcome)).unwrap(),
                0.9, 0.5, model, 0.5, String::new(), None,
                CallerRole::Agent, None, None,
                Some((p.situation.clone(), p.action.clone(), p.outcome.clone())),
                false,
            )
            .unwrap();
        t_state.push(r.state_node_id);
        t_out.push(r.outcome_node_id);
    }
    let mut wd: HashMap<String, Group> = HashMap::new();
    let mut wf: HashMap<String, Group> = HashMap::new();
    for (i, p) in pairs.iter().enumerate() {
        let k = group_key(p);
        let d = handle.retrieve(q_nat[i].clone(), model, NodeType::State, "").unwrap();
        let f = handle.retrieve(q_nat[i].clone(), model, NodeType::State, &p.query).unwrap();
        for (r, a) in [(&d, wd.entry(k.clone()).or_default()), (&f, wf.entry(k.clone()).or_default())] {
            let chain_ok = r.causal_chain.iter().any(|s| s.to_node == t_out[i]);
            a.add(
                r.seed_id == Some(t_state[i]),
                r.post_filter_candidates.iter().position(|&id| id == t_state[i]),
                chain_ok,
            );
        }
    }
    handle.shutdown().ok();

    let tot_wf: usize = wf.values().map(|a| a.seed_p1).sum();
    let tot_pool: usize = wf.values().map(|a| a.pool_hit).sum();
    let hard_wf = wf.get("para-hard").unwrap();
    let lex_wf = wf.get("lexical").unwrap();
    println!();
    println!("## ops_bench_l3_rerank l3a_bge_embedder_leg (L1, bge-base-en-v1.5, n=200)");
    println!("| group          | seedP@1 | poolRec | chainAcc | MRR    | n    |");
    println!("|----------------|---------|---------|----------|--------|------|");
    for k in ["lexical", "para-easy", "para-medium", "para-hard"] {
        if let Some((a_nat, a_prj)) = without.get(k) {
            println!("{}  WITHOUT-native", a_nat.row(k));
            println!("{}  WITHOUT-proj", a_prj.row(k));
        }
        if let Some(a) = wd.get(k) {
            println!("{}  WITH-dense", a.row(k));
        }
        if let Some(a) = wf.get(k) {
            println!("{}  WITH-fused", a.row(k));
        }
    }
    println!();
    println!(
        "GATES l3a (bge frame, standing bar): A lexical_guard={} (need ==1.00 -> {}/100), \
         B hard_seed={} (need >=15/30), C pool={} (need >=170/200), \
         D overall_R1={} (need >=140/200), E not_a_swap_worse (pool >=164/200)",
        lex_wf.seed_p1, lex_wf.seed_p1, hard_wf.seed_p1, tot_pool, tot_wf
    );
    println!(
        "MiniLM record anchors (informational only, no mixing): hard 2/30, pool 164/200, R@1 114/200"
    );
}

// ------------------------------------------------------------- dumps ----

#[derive(serde::Serialize, serde::Deserialize)]
struct DumpCand {
    rank: usize,
    node_id: u64,
    text: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DumpItem {
    id: String,
    group: String,
    query: String,
    gold_node_id: u64,
    gold_pool_rank: Option<usize>,
    pool_len: usize,
    presented: Vec<DumpCand>,
    gold_presented_rank: Option<usize>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DumpFile {
    leg: String,
    model: String,
    cap: usize,
    items: Vec<DumpItem>,
}

fn start_engine(pdir: &PathBuf) -> (tempfile::TempDir, EngineHandle) {
    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = pdir.to_path_buf();
    (tmp, engine::start(cfg).unwrap())
}

/// Byte-identical record learn treatment; returns state node ids.
fn learn_record(handle: &EngineHandle, pairs: &[BaseItem], s_nat: &[Vec<f32>], emb: &OnnxEmbedder, model: &str) -> Vec<u64> {
    let mut t = vec![];
    for (idx, p) in pairs.iter().enumerate() {
        let r = handle
            .learn_with_text_context(
                s_nat[idx].clone(),
                emb.embed(&format!("{} {idx}", p.action)).unwrap(),
                emb.embed(&format!("{} {idx}", p.outcome)).unwrap(),
                0.9, 0.5, model, 0.5, String::new(), None,
                CallerRole::Agent, None, None,
                Some((p.situation.clone(), p.action.clone(), p.outcome.clone())),
                false,
            )
            .unwrap();
        t.push(r.state_node_id);
    }
    t
}

/// W1a learn treatment (store text carries the keyword suffix).
fn learn_w1a(handle: &EngineHandle, pairs: &[BaseItem], s_nat: &[Vec<f32>], emb: &OnnxEmbedder, model: &str) -> Vec<u64> {
    let mut t = vec![];
    for (idx, p) in pairs.iter().enumerate() {
        let sit_raw = format!("{} keywords: {}", p.situation, p.kw_transcript);
        assert!(!p.kw_transcript.is_empty(), "{} empty kw", p.id);
        let r = handle
            .learn_with_text_context(
                s_nat[idx].clone(),
                emb.embed(&format!("{} {idx}", p.action)).unwrap(),
                emb.embed(&format!("{} {idx}", p.outcome)).unwrap(),
                0.9, 0.5, model, 0.5, String::new(), None,
                CallerRole::Agent, None, None,
                Some((sit_raw, p.action.clone(), p.outcome.clone())),
                false,
            )
            .unwrap();
        t.push(r.state_node_id);
    }
    t
}

fn present(pool: &[u64], texts: &HashMap<u64, String>, gold: u64) -> (Vec<DumpCand>, Option<usize>) {
    let mut out = vec![];
    let mut gold_rank = None;
    for (pos, &id) in pool.iter().take(CAP).enumerate() {
        out.push(DumpCand { rank: pos + 1, node_id: id, text: texts[&id].clone() });
        if id == gold {
            gold_rank = Some(pos + 1);
        }
    }
    (out, gold_rank)
}

fn dump_write(leg: &str, items: Vec<DumpItem>, file: &str) {
    let dump = DumpFile {
        leg: leg.to_string(),
        model: "minilm-l6-v2-int8".to_string(),
        cap: CAP,
        items,
    };
    std::fs::write(out_path(file), serde_json::to_string_pretty(&dump).unwrap()).unwrap();
    println!("written: {} ({} items)", file, dump.items.len());
}

// ------------------------------------------------------ B12-R instrument ----

fn b12r_path(name: &str) -> PathBuf {
    for base in [
        PathBuf::from("tools/eval_b12r"),
        PathBuf::from("..").join("tools/eval_b12r"),
    ] {
        if base.exists() {
            return base.join(name);
        }
    }
    panic!("tools/eval_b12r not found");
}

fn load_saved_texts(file: &str) -> HashMap<String, String> {
    serde_json::from_str(&std::fs::read_to_string(b12r_path(file)).unwrap()).unwrap()
}

/// Learn each agent-written text as the State node for its item. Missing
/// items get the registered placeholder (a guaranteed miss in scoring).
fn learn_agent_texts(
    handle: &EngineHandle,
    pairs: &[BaseItem],
    saved: &HashMap<String, String>,
    emb: &OnnxEmbedder,
    model: &str,
) -> (Vec<u64>, HashMap<u64, String>) {
    let mut state_ids = vec![];
    let mut texts = HashMap::new();
    for (idx, p) in pairs.iter().enumerate() {
        let text = saved
            .get(&p.id)
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "(no memory saved for this incident)".to_string());
        let r = handle
            .learn_with_text_context(
                emb.embed(&text).unwrap(),
                emb.embed(&format!("{} {idx}", p.action)).unwrap(),
                emb.embed(&format!("{} {idx}", p.outcome)).unwrap(),
                0.9, 0.5, model, 0.5, String::new(), None,
                CallerRole::Agent, None, None,
                Some((text.clone(), p.action.clone(), p.outcome.clone())),
                false,
            )
            .unwrap();
        state_ids.push(r.state_node_id);
        texts.insert(r.state_node_id, text);
    }
    (state_ids, texts)
}

fn b12r_dump_arm(arm: &str) {
    let pairs: Vec<BaseItem> = load_corpus("tools/corpus_l1.json")
        .into_iter()
        .filter(|p| p.difficulty == "hard")
        .collect();
    assert_eq!(pairs.len(), 30, "b12r: hard item count");
    let saved = load_saved_texts(&format!("saved_{arm}.json"));
    let (emb, model, proj_file) = make_embedder("minilm");
    let pdir = projection_dir();
    let _proj = Projection::from_bytes(&std::fs::read(pdir.join(proj_file)).unwrap()).unwrap();
    let q_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.query).unwrap()).collect();
    let (_tmp, handle) = start_engine(&pdir);
    let (t_state, texts) = learn_agent_texts(&handle, &pairs, &saved, &emb, model);
    let mut items = vec![];
    for (i, p) in pairs.iter().enumerate() {
        let f = handle.retrieve(q_nat[i].clone(), model, NodeType::State, &p.query).unwrap();
        assert!(f.post_filter_candidates.len() <= CAP, "engine cap violated on {}", p.id);
        let gold = t_state[i];
        let pool_rank = f.post_filter_candidates.iter().position(|&id| id == gold);
        let (presented, p_rank) = present(&f.post_filter_candidates, &texts, gold);
        items.push(DumpItem {
            id: p.id.clone(),
            group: format!("b12r-{arm}"),
            query: p.query.clone(),
            gold_node_id: gold,
            gold_pool_rank: pool_rank,
            pool_len: f.post_filter_candidates.len(),
            presented,
            gold_presented_rank: p_rank,
        });
    }
    assert_eq!(items.len(), 30, "b12r: dump item count");
    handle.shutdown().ok();
    dump_write("b12r_dump_agent_texts", items, &format!("pools_b12r_{arm}.json"));
}

/// B12-R: agent-written memories (two save-wording arms) -> fused pools.
/// Pre-registered in tools/eval_b8/b12r-save-discipline.md; eval only.
#[test]
#[ignore]
fn b12r_dump_agent_texts() {
    b12r_dump_arm("w");
    b12r_dump_arm("d");
}

fn sit_texts(pairs: &[BaseItem], t_state: &[u64], suffix: bool) -> HashMap<u64, String> {
    let mut m = HashMap::new();
    for (j, p) in pairs.iter().enumerate() {
        let text = if suffix {
            format!("{} keywords: {}", p.situation, p.kw_transcript)
        } else {
            p.situation.clone()
        };
        m.insert(t_state[j], text);
    }
    m
}

/// l3b: record learn, hard items only, 1 fused retrieve each.
#[test]
#[ignore]
fn l3b_dump_record_hard() {
    let pairs = load_corpus("tools/corpus_l1.json");
    let (emb, model, proj_file) = make_embedder("minilm");
    let pdir = projection_dir();
    let _proj = Projection::from_bytes(&std::fs::read(pdir.join(proj_file)).unwrap()).unwrap();
    let s_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.situation).unwrap()).collect();
    let q_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.query).unwrap()).collect();
    let (_tmp, handle) = start_engine(&pdir);
    let t_state = learn_record(&handle, &pairs, &s_nat, &emb, model);
    let texts = sit_texts(&pairs, &t_state, false);
    let mut items = vec![];
    for (i, p) in pairs.iter().enumerate() {
        if p.difficulty != "hard" {
            continue;
        }
        let f = handle.retrieve(q_nat[i].clone(), model, NodeType::State, &p.query).unwrap();
        assert!(f.post_filter_candidates.len() <= CAP, "engine cap violated on {}", p.id);
        let gold = t_state[i];
        let pool_rank = f.post_filter_candidates.iter().position(|&id| id == gold);
        let (presented, p_rank) = present(&f.post_filter_candidates, &texts, gold);
        items.push(DumpItem {
            id: p.id.clone(),
            group: group_key(p),
            query: p.query.clone(),
            gold_node_id: gold,
            gold_pool_rank: pool_rank,
            pool_len: f.post_filter_candidates.len(),
            presented,
            gold_presented_rank: p_rank,
        });
    }
    assert_eq!(items.len(), 30);
    handle.shutdown().ok();
    dump_write("l3b_dump_record_hard", items, "pools_record_hard.json");
}

/// l3c: easy/medium record pools + hard D2 4-run merged union (cap 20).
#[test]
#[ignore]
fn l3c_dump_d2expanded_para() {
    let pairs = load_corpus("tools/eval_d2/corpus_l1_d2.json");
    let (emb, model, proj_file) = make_embedder("minilm");
    let pdir = projection_dir();
    let _proj = Projection::from_bytes(&std::fs::read(pdir.join(proj_file)).unwrap()).unwrap();
    let s_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.situation).unwrap()).collect();
    let q_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.query).unwrap()).collect();
    let (_tmp, handle) = start_engine(&pdir);
    let t_state = learn_record(&handle, &pairs, &s_nat, &emb, model);
    let texts = sit_texts(&pairs, &t_state, false);
    let mut items = vec![];
    for (i, p) in pairs.iter().enumerate() {
        if p.kind != "paraphrase" {
            continue;
        }
        let gold = t_state[i];
        let (pool, pool_rank) = if p.difficulty == "hard" {
            assert_eq!(p.causes.len(), 3, "{} missing causes", p.id);
            // 4 self-contained runs (registered: orig + 3 causes), union first-seen.
            let mut runs: Vec<(Vec<f32>, String)> = vec![(q_nat[i].clone(), p.query.clone())];
            for c in &p.causes {
                runs.push((emb.embed(c).unwrap(), c.clone()));
            }
            let mut union: Vec<u64> = vec![];
            let mut best = None;
            for (emb_v, text) in &runs {
                let r = handle.retrieve(emb_v.clone(), model, NodeType::State, text).unwrap();
                if let Some(pos) = r.post_filter_candidates.iter().position(|&id| id == gold) {
                    best = Some(best.map_or(pos, |b: usize| b.min(pos)));
                }
                for &id in &r.post_filter_candidates {
                    if !union.contains(&id) {
                        union.push(id);
                    }
                }
            }
            (union, best)
        } else {
            let f = handle.retrieve(q_nat[i].clone(), model, NodeType::State, &p.query).unwrap();
            assert!(f.post_filter_candidates.len() <= CAP, "engine cap violated on {}", p.id);
            let pr = f.post_filter_candidates.iter().position(|&id| id == gold);
            (f.post_filter_candidates.clone(), pr)
        };
        let (presented, p_rank) = present(&pool, &texts, gold);
        items.push(DumpItem {
            id: p.id.clone(),
            group: group_key(p),
            query: p.query.clone(),
            gold_node_id: gold,
            gold_pool_rank: pool_rank,
            pool_len: pool.len(),
            presented,
            gold_presented_rank: p_rank,
        });
    }
    assert_eq!(items.len(), 100);
    assert_eq!(items.iter().filter(|it| it.group == "para-easy").count(), 29);
    assert_eq!(items.iter().filter(|it| it.group == "para-medium").count(), 41);
    assert_eq!(items.iter().filter(|it| it.group == "para-hard").count(), 30);
    handle.shutdown().ok();
    dump_write("l3c_dump_d2expanded_para", items, "pools_d2expanded_para.json");
}

/// l3d: W1a pools for the 100 paraphrase items (lexical bypassed at 100/100).
#[test]
#[ignore]
fn l3d_dump_w1bridge_all() {
    let pairs = load_corpus("tools/eval_w1/corpus_l1_kw.json");
    let (emb, model, proj_file) = make_embedder("minilm");
    let pdir = projection_dir();
    let _proj = Projection::from_bytes(&std::fs::read(pdir.join(proj_file)).unwrap()).unwrap();
    let s_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.situation).unwrap()).collect();
    let q_nat: Vec<Vec<f32>> = pairs.iter().map(|p| emb.embed(&p.query).unwrap()).collect();
    let (_tmp, handle) = start_engine(&pdir);
    let t_state = learn_w1a(&handle, &pairs, &s_nat, &emb, model);
    let texts = sit_texts(&pairs, &t_state, true);
    let mut items = vec![];
    for (i, p) in pairs.iter().enumerate() {
        if p.kind != "paraphrase" {
            continue;
        }
        let f = handle.retrieve(q_nat[i].clone(), model, NodeType::State, &p.query).unwrap();
        assert!(f.post_filter_candidates.len() <= CAP, "engine cap violated on {}", p.id);
        let gold = t_state[i];
        let pool_rank = f.post_filter_candidates.iter().position(|&id| id == gold);
        let (presented, p_rank) = present(&f.post_filter_candidates, &texts, gold);
        items.push(DumpItem {
            id: p.id.clone(),
            group: group_key(p),
            query: p.query.clone(),
            gold_node_id: gold,
            gold_pool_rank: pool_rank,
            pool_len: f.post_filter_candidates.len(),
            presented,
            gold_presented_rank: p_rank,
        });
    }
    assert_eq!(items.len(), 100);
    handle.shutdown().ok();
    dump_write("l3d_dump_w1bridge_all", items, "pools_w1bridge_all.json");
}

// ------------------------------------------------------------ scoring ----

fn load_dump(file: &str) -> DumpFile {
    for p in [PathBuf::from(file), PathBuf::from("..").join(file)] {
        if p.exists() {
            return serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        }
    }
    panic!("{file} not found");
}

fn load_rerank(file: &str) -> HashMap<String, usize> {
    for p in [PathBuf::from(file), PathBuf::from("..").join(file)] {
        if p.exists() {
            let raw: HashMap<String, serde_json::Value> =
                serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
            let mut out = HashMap::new();
            for (k, v) in raw {
                let n = v
                    .as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
                    .expect("rerank pick must be an integer") as usize;
                out.insert(k, n);
            }
            return out;
        }
    }
    panic!("{file} not found");
}

fn score_leg(leg: &str, dump_file: &str, rerank_file: &str, gated: bool) {
    let dump: DumpFile = load_dump(dump_file);
    let picks = load_rerank(rerank_file);
    let mut groups: HashMap<String, (usize, usize, usize, usize, usize, usize, usize)> = HashMap::new();
    // (n, gold_in_pool, gold_presented, hits, lost_candidate, lost_visibility, lost_rank)
    let mut hist: HashMap<usize, usize> = HashMap::new();
    let mut invalid = 0usize;
    for it in &dump.items {
        assert!(
            picks.contains_key(&it.id),
            "missing rerank pick for {}",
            it.id
        );
        let g = groups.entry(it.group.clone()).or_default();
        g.0 += 1;
        if it.gold_pool_rank.is_some() {
            g.1 += 1;
        } else {
            g.4 += 1;
        }
        if it.gold_presented_rank.is_some() {
            g.2 += 1;
        } else if it.gold_pool_rank.is_some() {
            g.5 += 1;
        }
        let pick = picks[&it.id];
        if pick == 0 || pick > it.presented.len() {
            invalid += 1;
            g.6 += 1;
        } else {
            *hist.entry(pick).or_default() += 1;
            if it.presented[pick - 1].node_id == it.gold_node_id {
                g.3 += 1;
            } else {
                g.6 += 1;
            }
        }
    }
    let mut keys: Vec<&String> = groups.keys().collect();
    keys.sort();
    println!();
    println!("## {} score (rerank seed = LLM pick; lexical bypassed at 100/100)", leg);
    println!("| group          |  n | gold_in_pool | gold_presented | rerank_hits | acc_on_presented |");
    println!("|----------------|----|--------------|----------------|-------------|------------------|");
    let mut pool_tot = LEX_POOL;
    let mut r1_tot = LEX_SEED;
    let mut hard_hits = None;
    for k in &keys {
        let (n, gp, gpr, hits, _lc, _lv, _lr) = groups[*k];
        let acc = if gpr > 0 { hits as f64 / gpr as f64 } else { 0.0 };
        println!("| {k:<14} | {n:>2} | {gp:>12} | {gpr:>14} | {hits:>11} | {acc:>16.4} |");
        if *k != "lexical" {
            pool_tot += gp;
            r1_tot += hits;
        }
        if *k == "para-hard" {
            hard_hits = Some(hits);
        }
    }
    println!();
    println!("## attribution");
    println!("| group          | lost_candidate | lost_rerank_visibility | lost_rank_rerank |");
    println!("|----------------|----------------|------------------------|------------------|");
    for k in &keys {
        let (_, _gp, _gpr, _h, lc, lv, lr) = groups[*k];
        println!("| {k:<14} | {lc:>14} | {lv:>22} | {lr:>16} |");
    }
    println!();
    let mut h: Vec<(usize, usize)> = hist.into_iter().collect();
    h.sort();
    println!("pick distribution (index: count): {:?}", h);
    println!("invalid/missing picks (counted as lost_rank_rerank): {invalid}");
    if gated {
        let hard = hard_hits.expect("hard group present");
        let gate_c = pool_tot >= 170;
        let gate_d = r1_tot >= 140;
        let gate_b = hard >= 15;
        println!(
            "GATES {leg}: A lexical_bypass=100/100 (==1.00 by construction), \
             B hard_seed={}/30 (need >=15): {}, C pool={}/200 (need >=170): {}, \
             D overall_R1={}/200 (need >=140): {}, E pool>=164/200: {}",
            hard,
            if gate_b { "OK" } else { "FAIL" },
            pool_tot,
            if gate_c { "OK" } else { "FAIL" },
            r1_tot,
            if gate_d { "OK" } else { "FAIL" },
            if pool_tot >= 164 { "OK" } else { "FAIL" }
        );
    }
}

#[test]
#[ignore]
fn l3b_score_record_hard() {
    // DIAGNOSTIC leg (pre-registered: no standing gates — pool math fails C
    // by construction on the record pool).
    score_leg("l3b_record_hard", "tools/eval_l3/pools_record_hard.json", "tools/eval_l3/rerank_record_hard.json", false);
}

#[test]
#[ignore]
fn l3c_score_d2expanded() {
    score_leg("l3c_d2expanded", "tools/eval_l3/pools_d2expanded_para.json", "tools/eval_l3/rerank_d2expanded.json", true);
}

#[test]
#[ignore]
fn l3d_score_w1bridge() {
    score_leg("l3d_w1bridge", "tools/eval_l3/pools_w1bridge_all.json", "tools/eval_l3/rerank_w1bridge.json", true);
}

#[test]
#[ignore]
fn l3a_bge_embedder_leg() {
    l3a_bge_leg();
}
