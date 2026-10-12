#![cfg(feature = "onnx")]
// Honest BGE bench (Lever 1): WITHOUT CMHM vs WITH CMHM, same vectors, same corpus.
//
// WITHOUT = brute-force cosine over NATIVE 768-dim vectors and over
//   PROJECTED 128-dim vectors (same PCA matrix the engine uses).
// WITH = full engine retrieve (dense-only + fused).
//
// Corpus: same 40 human ops paraphrases as honest_bench.rs (no margin filter).
// Run:
//   cargo +stable-x86_64-pc-windows-msvc test -p cmhm_core --target x86_64-pc-windows-msvc --features onnx --test honest_bge_bench -- --ignored --nocapture

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::embed::{Embedder, OnnxEmbedder};
use cmhm_core::engine;
use cmhm_core::node_store::cosine;
use cmhm_core::projection::Projection;
use cmhm_core::NodeType;
use std::path::PathBuf;
use tempfile::tempdir;

const MODEL: &str = "bge-base-en-v1.5";
const PROJ_FILE: &str = "data/projections/bge-base-en-v1.5.bin";

const PARAPHRASES: [(&str, &str); 40] = [
    ("database connection pool exhausted under peak load", "why are connections timing out at peak hours"),
    ("authentication token expired before refresh completed", "users getting logged out unexpectedly"),
    ("disk partition reached full capacity overnight", "server ran out of storage space"),
    ("message queue backlog growing faster than workers drain", "background jobs piling up unprocessed"),
    ("tls certificate renewal failed on edge nodes", "browsers warn the site is insecure"),
    ("cache hit ratio collapsed after deploy", "responses suddenly much slower since release"),
    ("dns resolver returning stale records", "clients reaching the wrong server address"),
    ("memory leak in worker process over several hours", "service restarts fix rising ram usage temporarily"),
    ("leader election flapping between replicas", "cluster keeps switching primary repeatedly"),
    ("rate limiter rejecting legitimate bursts", "customers report blocked requests during sales"),
    ("null pointer crash in payment handler", "checkout fails with internal error"),
    ("stale lock held after node failure", "jobs stuck waiting on a dead owner"),
    ("index rebuild blocked writes for twenty minutes", "database froze during maintenance window"),
    ("clock skew broke token validation", "fresh logins rejected as expired"),
    ("connection churn from misconfigured keepalive", "sockets opening and closing constantly"),
    ("retry storm amplified downstream outage", "one failing service took down the rest"),
    ("unbounded log growth consumed all disk", "alerts fired for filesystem at one hundred percent"),
    ("schema migration locked the users table", "signups timing out during upgrade"),
    ("hot partition throttled by key skew", "one shard overloaded while others idle"),
    ("feature flag evaluation slow on cold start", "first request after boot takes forever"),
    ("webhook delivery delayed by backoff", "integrations receiving events late"),
    ("session store evicted active carts", "shoppers losing basket contents"),
    ("search index lagging behind writes", "new documents not appearing in results"),
    ("backup snapshot corrupted and unrestorable", "disaster recovery drill failed to bring data back"),
    ("container image pull throttled by registry", "deploys stuck waiting on downloads"),
    ("thread pool starvation under fan-out", "parallel requests queueing with idle cpu"),
    ("misrouted traffic after config push", "half the regions serving errors post deploy"),
    ("password rotation broke service account", "nightly sync failing authentication"),
    ("fragmented packets dropped by firewall", "large uploads failing on corporate network"),
    ("runaway cron spawning duplicate reports", "customers emailed the same invoice twice"),
    ("serializer mismatch between client and server", "mobile app crashing on new api response"),
    ("file descriptor leak in proxy", "gateway refusing connections after days uptime"),
    ("priority inversion in scheduler", "urgent tasks waiting behind batch work"),
    ("expired cache poisoned responses", "users seeing yesterday prices"),
    ("split brain after network partition", "two primaries accepting writes simultaneously"),
    ("slow query missing index on join", "report page loads in minutes not seconds"),
    ("health check too strict caused flapping", "instances cycling in and out of rotation"),
    ("secret volume failed to mount", "pods crashlooping on missing config"),
    ("overflow counter wrapped request ids", "duplicate identifiers appearing in traces"),
    ("charset mismatch garbled export file", "csv download shows broken characters"),
];

fn projection_dir() -> Option<PathBuf> {
    [PathBuf::from("..").join(PROJ_FILE), PathBuf::from(PROJ_FILE)]
        .into_iter()
        .find(|p| p.exists())
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn load_proj() -> Projection {
    let bytes = std::fs::read(PROJ_FILE)
        .or_else(|_| std::fs::read(PathBuf::from("..").join(PROJ_FILE)))
        .expect("projection file missing");
    Projection::from_bytes(&bytes).expect("projection parse")
}

struct Agg {
    n: usize,
    p1: usize,
    p5: usize,
    mrr: f64,
}
impl Agg {
    fn new() -> Self {
        Agg { n: 0, p1: 0, p5: 0, mrr: 0.0 }
    }
    fn add(&mut self, rank: Option<usize>) {
        self.n += 1;
        if let Some(r) = rank {
            if r == 0 {
                self.p1 += 1;
            }
            if r < 5 {
                self.p5 += 1;
            }
            self.mrr += 1.0 / (r as f64 + 1.0);
        }
    }
    fn summary(&self) -> String {
        format!(
            "P@1={:.4} P@5={:.4} MRR={:.4} ({}/{})",
            self.p1 as f64 / self.n as f64,
            self.p5 as f64 / self.n as f64,
            self.mrr / self.n as f64,
            self.p1,
            self.n
        )
    }
}

#[test]
#[ignore]
fn honest_bge_without_vs_with() {
    let proj_dir = match projection_dir() {
        Some(p) => p,
        None => {
            eprintln!("SKIP: projection missing");
            return;
        }
    };
    let emb = OnnxEmbedder::bge().expect("bge init");
    assert_eq!(emb.dim(), 768);
    let proj = load_proj();

    let s_native: Vec<Vec<f32>> = PARAPHRASES
        .iter()
        .map(|(s, _)| emb.embed(s).unwrap())
        .collect();
    let q_native: Vec<Vec<f32>> = PARAPHRASES
        .iter()
        .map(|(_, q)| emb.embed(q).unwrap())
        .collect();

    let s_proj: Vec<Vec<f32>> = s_native
        .iter()
        .map(|v| proj.project(v).unwrap().to_vec())
        .collect();
    let q_proj: Vec<Vec<f32>> = q_native
        .iter()
        .map(|v| proj.project(v).unwrap().to_vec())
        .collect();

    let mut no_native = Agg::new();
    let mut no_proj = Agg::new();
    for i in 0..PARAPHRASES.len() {
        let mut sn: Vec<(usize, f32)> = s_native
            .iter()
            .enumerate()
            .map(|(j, s)| (j, cosine(&q_native[i], s)))
            .collect();
        sn.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        no_native.add(sn.iter().position(|(j, _)| *j == i));

        let mut sp: Vec<(usize, f32)> = s_proj
            .iter()
            .enumerate()
            .map(|(j, s)| (j, cosine(&q_proj[i], s)))
            .collect();
        sp.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        no_proj.add(sp.iter().position(|(j, _)| *j == i));
    }

    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = proj_dir;
    let handle = engine::start(cfg).unwrap();

    let mut target_ids = Vec::with_capacity(PARAPHRASES.len());
    for (idx, _) in PARAPHRASES.iter().enumerate() {
        let r = handle
            .learn_with_text_context(
                s_native[idx].clone(),
                emb.embed(&format!("investigated incident {idx}")).unwrap(),
                emb.embed(&format!("service restored after incident {idx}")).unwrap(),
                0.9,
                0.5,
                MODEL,
                0.5,
                String::new(),
                None,
                CallerRole::Agent,
                None,
                None,
                Some((
                    PARAPHRASES[idx].0.to_string(),
                    format!("investigated incident {idx}"),
                    format!("service restored after incident {idx}"),
                )),
            false,
            )
            .unwrap();
        target_ids.push(r.state_node_id);
    }

    let mut with_dense = Agg::new();
    let mut with_fused = Agg::new();
    for i in 0..PARAPHRASES.len() {
        let target = target_ids[i];
        let d = handle
            .retrieve(q_native[i].clone(), MODEL, NodeType::State, "")
            .unwrap();
        with_dense.add(d.post_filter_candidates.iter().position(|&id| id == target));
        let f = handle
            .retrieve(q_native[i].clone(), MODEL, NodeType::State, PARAPHRASES[i].1)
            .unwrap();
        with_fused.add(f.post_filter_candidates.iter().position(|&id| id == target));
    }
    handle.shutdown().ok();

    println!();
    println!("## Honest BGE (Lever 1): WITHOUT vs WITH CMHM (n=40 paraphrase, human pairs)");
    println!("WITHOUT native 768-d brute-force : {}", no_native.summary());
    println!("WITHOUT projected 128-d brute    : {}", no_proj.summary());
    println!("WITH CMHM dense-only             : {}", with_dense.summary());
    println!("WITH CMHM fused                  : {}", with_fused.summary());
}
