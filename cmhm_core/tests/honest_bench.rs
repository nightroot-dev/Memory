// Honest benchmark: WITHOUT CMHM vs WITH CMHM on real-ish ops paraphrases.
//
// WITHOUT CMHM = flat brute-force over the same 40 situations:
//   (a) dense cosine only (no graph, no dedup, no RRF, no Dijkstra, no authority)
//   (b) BM25 lexical only (no dense, no graph)
// WITH CMHM = full engine retrieve:
//   (c) dense-only (query_text = "") — NN top-8 + authority + Dijkstra
//   (d) fused (query_text = real) — dense + BM25 RRF + authority + Dijkstra
//
// Corpus: 40 human-written ops paraphrase pairs from bm25_fusion_bench.
// NOT embedder-filtered (no circular margin validation). Minimal token
// overlap by construction, realistic ops language. Toy rare-ID lexical
// half is deliberately excluded.
//
// Run:
//   cargo +stable-x86_64-pc-windows-gnu test -p cmhm_core --test honest_bench -- --ignored --nocapture

use cmhm_core::authority::CallerRole;
use cmhm_core::bm25::Bm25Index;
use cmhm_core::config::Config;
use cmhm_core::engine;
use cmhm_core::node_store::cosine;
use cmhm_core::text_to_embedding;
use cmhm_core::NodeType;
use tempfile::tempdir;

const MODEL: &str = "hash-128d";

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
fn honest_without_vs_with() {
    // ---- WITHOUT CMHM: flat brute force ----
    let s_embs: Vec<Vec<f32>> = PARAPHRASES
        .iter()
        .map(|(s, _)| text_to_embedding(s).to_vec())
        .collect();

    let mut no_cmhm_dense = Agg::new();
    for (i, (_, q)) in PARAPHRASES.iter().enumerate() {
        let q_emb = text_to_embedding(q).to_vec();
        let mut scored: Vec<(usize, f32)> = s_embs
            .iter()
            .enumerate()
            .map(|(j, s)| (j, cosine(&q_emb, s)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let rank = scored.iter().position(|(j, _)| *j == i);
        no_cmhm_dense.add(rank);
    }

    let mut no_cmhm_bm25_idx = Bm25Index::new();
    for (i, (s, _)) in PARAPHRASES.iter().enumerate() {
        no_cmhm_bm25_idx.insert(i as u64, s);
    }
    let mut no_cmhm_bm25 = Agg::new();
    for (i, (_, q)) in PARAPHRASES.iter().enumerate() {
        let res = no_cmhm_bm25_idx.query(q, 40);
        let rank = res.iter().position(|(id, _)| *id == i as u64);
        no_cmhm_bm25.add(rank);
    }

    // ---- WITH CMHM: full engine ----
    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = tmp.path().join("projections");
    let handle = engine::start(cfg).unwrap();

    let mut target_ids = Vec::with_capacity(PARAPHRASES.len());
    for (idx, (s, _)) in PARAPHRASES.iter().enumerate() {
        let s_emb = text_to_embedding(s).to_vec();
        let a_emb = text_to_embedding(&format!("investigated incident {idx}")).to_vec();
        let o_emb = text_to_embedding(&format!("service restored after incident {idx}")).to_vec();
        let r = handle
            .learn_with_text_context(
                s_emb,
                a_emb,
                o_emb,
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
                    s.to_string(),
                    format!("investigated incident {idx}"),
                    format!("service restored after incident {idx}"),
                )),
            )
            .unwrap();
        target_ids.push(r.state_node_id);
    }

    let mut with_dense = Agg::new();
    let mut with_fused = Agg::new();
    for (i, (_, q)) in PARAPHRASES.iter().enumerate() {
        let q_emb = text_to_embedding(q).to_vec();
        let target = target_ids[i];
        let d = handle
            .retrieve(q_emb.clone(), MODEL, NodeType::State, "")
            .unwrap();
        let rank_d = d.post_filter_candidates.iter().position(|&id| id == target);
        with_dense.add(rank_d);
        let f = handle
            .retrieve(q_emb, MODEL, NodeType::State, *q)
            .unwrap();
        let rank_f = f.post_filter_candidates.iter().position(|&id| id == target);
        with_fused.add(rank_f);
    }
    handle.shutdown().ok();

    println!();
    println!("## Honest bench: WITHOUT vs WITH CMHM (hash-128d, n=40 paraphrase, no margin filter)");
    println!("WITHOUT dense brute-force : {}", no_cmhm_dense.summary());
    println!("WITHOUT BM25 only         : {}", no_cmhm_bm25.summary());
    println!("WITH CMHM dense-only      : {}", with_dense.summary());
    println!("WITH CMHM fused           : {}", with_fused.summary());
    println!();
    println!("| Condition | P@1 | P@5 | MRR |");
    println!("|---|---|---|---|");
    println!(
        "| WITHOUT dense | {:.4} | {:.4} | {:.4} |",
        no_cmhm_dense.p1 as f64 / no_cmhm_dense.n as f64,
        no_cmhm_dense.p5 as f64 / no_cmhm_dense.n as f64,
        no_cmhm_dense.mrr / no_cmhm_dense.n as f64
    );
    println!(
        "| WITHOUT BM25 | {:.4} | {:.4} | {:.4} |",
        no_cmhm_bm25.p1 as f64 / no_cmhm_bm25.n as f64,
        no_cmhm_bm25.p5 as f64 / no_cmhm_bm25.n as f64,
        no_cmhm_bm25.mrr / no_cmhm_bm25.n as f64
    );
    println!(
        "| WITH dense-only | {:.4} | {:.4} | {:.4} |",
        with_dense.p1 as f64 / with_dense.n as f64,
        with_dense.p5 as f64 / with_dense.n as f64,
        with_dense.mrr / with_dense.n as f64
    );
    println!(
        "| WITH fused | {:.4} | {:.4} | {:.4} |",
        with_fused.p1 as f64 / with_fused.n as f64,
        with_fused.p5 as f64 / with_fused.n as f64,
        with_fused.mrr / with_fused.n as f64
    );
}
