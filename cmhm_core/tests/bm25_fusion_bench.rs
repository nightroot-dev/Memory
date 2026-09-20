// cmhm_core/tests/bm25_fusion_bench.rs
//
// FEATURE BENCHMARK (not an approval claim): BM25 fusion recall delta.
//
// Measures pool-membership rank of a known target under two conditions:
//   dense-only  (query_text = "")      vs   fused (query_text = real query)
// Groups: 40 LEXICAL queries (rare unique token) + 40 PARAPHRASE queries.
// No PASS/FAIL assertion — prints a markdown table. Run manually:
//   cargo test -p cmhm_core --test bm25_fusion_bench -- --ignored --nocapture
//
// Corpus: deterministic string templates + hardcoded paraphrase pairs.
// No dependency on engine internals. Fresh data, unrelated to corpus v4.

use cmhm_core::{text_to_embedding, Config, ContextRuntime, ContextType, CallerRole};
use cmhm_core::bm25::Bm25Index;

fn make_cfg(test_name: &str) -> (Config, std::path::PathBuf) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cmhm_fusion_{test_name}_{id:06}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = dir.clone();
    cfg.projections_dir = dir.join("projections");
    cfg.max_active_contexts = 4;
    // Freeze maintenance during measurement: fresh nodes can't be culled by
    // age, but pin the triggers anyway so the store is exactly what we learned.
    cfg.consolidation_node_trigger = 1_000_000;
    cfg.consolidation_reinforce_trigger = 1e9;
    cfg.consolidation_interval_hours = 1e9;
    (cfg, dir)
}

const MODEL: &str = "hash-128d";

/// 40 paraphrase (situation, query) pairs — common vocabulary, minimal token overlap.
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

struct Item {
    situation: String,
    query: String,
    kind: &'static str, // "lexical" | "paraphrase"
}

fn build_corpus() -> Vec<Item> {
    let mut items = Vec::with_capacity(80);
    for i in 0..40 {
        let token = format!("rare_token_{i}");
        items.push(Item {
            situation: format!("system experienced {token} failure during phase {}", i % 5),
            query: format!("what causes {token}"),
            kind: "lexical",
        });
    }
    for (s, q) in PARAPHRASES {
        items.push(Item {
            situation: s.to_string(),
            query: q.to_string(),
            kind: "paraphrase",
        });
    }
    items
}

fn rank_of(pool: &[u64], target: u64) -> Option<usize> {
    pool.iter().position(|&id| id == target).map(|p| p + 1)
}

#[derive(Default)]
struct Agg {
    n: usize,
    p1: usize,
    p5: usize,
    p10: usize,
    p20: usize,
    mrr: f64,
}

impl Agg {
    fn add(&mut self, rank: Option<usize>) {
        self.n += 1;
        if let Some(r) = rank {
            if r <= 1 { self.p1 += 1; }
            if r <= 5 { self.p5 += 1; }
            if r <= 10 { self.p10 += 1; }
            if r <= 20 { self.p20 += 1; }
            self.mrr += 1.0 / (r as f64);
        }
    }
    fn row(&self) -> String {
        let n = self.n as f64;
        format!(
            "| {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.3} |",
            100.0 * self.p1 as f64 / n,
            100.0 * self.p5 as f64 / n,
            100.0 * self.p10 as f64 / n,
            100.0 * self.p20 as f64 / n,
            self.mrr / n,
        )
    }
}

#[test]
#[ignore]
fn bm25_fusion_reports_recall_delta() {
    let (cfg, _dir) = make_cfg("bench");
    let mut rt = ContextRuntime::new(cfg, "fusion-bench".into()).unwrap();
    let ctx_id = rt
        .create_context("fusion", ContextType::Project, None, "fusion", vec![])
        .unwrap()
        .id;
    let ctx = Some(ctx_id.as_str());

    let corpus = build_corpus();
    // Mirror index: same (id -> state text) pairs the engine indexes.
    // Used ONLY for the (b) diagnostic — the measured ranks come from the engine.
    let mut mirror = Bm25Index::new();
    let mut targets: Vec<u64> = Vec::with_capacity(corpus.len());

    for (i, item) in corpus.iter().enumerate() {
        let s_emb = text_to_embedding(&item.situation).to_vec();
        let a_emb = text_to_embedding(&format!("investigated incident {i}")).to_vec();
        let o_emb = text_to_embedding(&format!("service restored after incident {i}")).to_vec();
        let (res, _) = rt
            .learn_text_with_context(
                ctx,
                s_emb,
                a_emb,
                o_emb,
                0.9, // force birth: every situation gets its own nodes
                0.5,
                MODEL,
                0.5,
                String::new(),
                None,
                CallerRole::Agent,
                None,
                None,
                Some((
                    item.situation.clone(),
                    format!("investigated incident {i}"),
                    format!("service restored after incident {i}"),
                )),
            )
            .expect("bench learn");
        mirror.insert(res.state_node_id, &item.situation);
        targets.push(res.state_node_id);
    }

    let mut lex_dense = Agg::default();
    let mut lex_fused = Agg::default();
    let mut par_dense = Agg::default();
    let mut par_fused = Agg::default();

    for (item, &target) in corpus.iter().zip(targets.iter()) {
        let q_emb = text_to_embedding(&item.query).to_vec();
        let (dense_res, _) = rt.retrieve_unscoped(ctx, q_emb.clone(), MODEL, "").unwrap();
        let (fused_res, _) = rt
            .retrieve_unscoped(ctx, q_emb, MODEL, &item.query)
            .unwrap();
        let rd = rank_of(&dense_res.post_filter_candidates, target);
        let rf = rank_of(&fused_res.post_filter_candidates, target);
        if item.kind == "lexical" {
            lex_dense.add(rd);
            lex_fused.add(rf);
        } else {
            par_dense.add(rd);
            par_fused.add(rf);
        }
        // Diagnostics for the first 5 queries of each kind.
        if item.kind == "lexical" && lex_dense.n <= 5 || item.kind == "paraphrase" && par_dense.n <= 5 {
            println!(
                "diag kind={} q={:?} raw_nn={} bm25_mirror={} pool_dense={} pool_fused={} rank_dense={:?} rank_fused={:?}",
                item.kind,
                item.query,
                dense_res.raw_nn_candidates.len(),
                mirror.query(&item.query, 50).len(),
                dense_res.post_filter_candidates.len(),
                fused_res.post_filter_candidates.len(),
                rd,
                rf,
            );
        }
    }

    let mut all_dense = Agg::default();
    let mut all_fused = Agg::default();
    for agg in [&lex_dense, &par_dense] {
        all_dense.n += agg.n;
        all_dense.p1 += agg.p1;
        all_dense.p5 += agg.p5;
        all_dense.p10 += agg.p10;
        all_dense.p20 += agg.p20;
        all_dense.mrr += agg.mrr;
    }
    for agg in [&lex_fused, &par_fused] {
        all_fused.n += agg.n;
        all_fused.p1 += agg.p1;
        all_fused.p5 += agg.p5;
        all_fused.p10 += agg.p10;
        all_fused.p20 += agg.p20;
        all_fused.mrr += agg.mrr;
    }

    println!("\n## BM25 Fusion Benchmark (pool-membership rank, n=80)");
    println!("| Group | Condition | P@1 | P@5 | P@10 | P@20 | MRR |");
    println!("|---|---|---|---|---|---|---|");
    println!("| Lexical (40) | Dense-only {} |", lex_dense.row());
    println!("| Lexical (40) | Fused {} |", lex_fused.row());
    println!("| Paraphrase (40) | Dense-only {} |", par_dense.row());
    println!("| Paraphrase (40) | Fused {} |", par_fused.row());
    println!("| Overall (80) | Dense-only {} |", all_dense.row());
    println!("| Overall (80) | Fused {} |", all_fused.row());
}
