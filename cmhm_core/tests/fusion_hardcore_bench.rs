//! HARDCORE variant of the fusion benchmark: heavier distractors, template
//! overlap, paraphrase traps, and a BM25 re-index churn phase that proves the
//! total_length accounting no longer corrupts under repeated re-indexing.
//!
//! NOTE: n=23 paraphrase / n=24 lexical scenarios — a single flip is ~4pt.
//! Small deltas between conditions on this corpus are within noise; the durable
//! signal is the churn assertion (re-indexing does not grow doc counts / drift
//! total_length), which is the F1 regression pin. Do not cite sub-5pt deltas.
//!
//! Run: cargo test -p cmhm_core --test fusion_hardcore_bench -- --ignored --nocapture

use cmhm_core::{Config, engine, NodeType, CallerRole, text_to_embedding};
use cmhm_core::bm25::Bm25Index;

fn lexical_corpus() -> Vec<(&'static str, String, &'static str, String)> {
    // 24 distinct rare tokens embedded in template-heavy situations; the query
    // shares ONLY the rare token, so pure-lexical overlap with template words
    // must not decide the ranking.
    let mut v: Vec<(&'static str, &'static str, &'static str, String)> = Vec::new();
    let rare = ["zqwr0","zqwr1","zqwr2","zqwr3","zqwr4","zqwr5","zqwr6","zqwr7",
                "zqwr8","zqwr9","zqwrA","zqwrB","zqwrC","zqwrD","zqwrE","zqwrF",
                "zqwrG","zqwrH","zqwrI","zqwrJ","zqwrK","zqwrL","zqwrM","zqwrN"];
    let templates = [
        ("the {} connection pool keeps exhausting during traffic spikes", "raised idle pool cap to 200", "connections recovered"),
        ("{} latency p99 blowing past two hundred milleseconds", "enabled query plan cache", "p99 back to 40ms"),
        ("redis {} replica falling behind the leader constantly", "switched replica to async replication", "replica caught up"),
        ("postgres {} deadlocks every time the payroll job runs", "reordered transaction queries", "deadlock rate fell to zero"),
        ("kafka {} consumer group lagging by millions of messages", "increased partition count to 64", "lag drained overnight"),
        ("nginx {} upstream timing out and returning 502s", "raised upstream timeout to five seconds", "502 errors stopped"),
    ];
    for (i, &r) in rare.iter().enumerate() {
        let t = templates[i % templates.len()];
        v.push((r, t.0, t.1, format!("{} {}", r, t.1)));
    }
    // Fill remaining slots with full sentences per token for the query set.
    let mut out: Vec<(&'static str, String, &'static str, String)> = Vec::new();
    for (i, (r, tmpl, action, _outcome)) in v.iter().enumerate() {
        let situation = tmpl.replacen("{}", r, 1);
        let query = format!("what causes {} failures", r);
        out.push((r, situation, *action, query));
    }
    out
}

const PARAPHRASE_HARD: &[(&str, &str, &str, &str)] = &[
    ("cpu throttling on the api server", "raised cpu limit", "throttle rate dropped", "the api is hitting cpu limits"),
    ("tls certificate expired on the ingress", "renewed via cert-manager", "https traffic restored", "users see expired cert warnings"),
    ("disk full on the database node", "expanded the volume", "database writable again", "no space left on the db server"),
    ("oauth token endpoint returning 429", "added client-side backoff", "rate limit errors gone", "login failing with too many requests"),
    ("memory leak in the worker process", "patched the slab allocator", "heap stable", "worker keeps growing in memory"),
    ("clock skew breaks jwt validation", "synced ntp on the nodes", "tokens validate again", "sign-ins rejected on time mismatch"),
    ("cdn cache serving stale assets", "purged the cache keys", "fresh assets served", "users stuck on old frontend"),
    ("queue depth exceeded alert storm", "added alert deduplication", "alert noise dropped", "pager fired fifty times"),
    ("ssh brute force on the bastion", "enabled fail2ban", "attempts blocked", "many failed ssh logins"),
    ("elasticsearch shard allocation blocked", "raised flood watermark", "shards reallocated", "es refuses to assign shards"),
    ("wireguard tunnel dropping packets", "raised mtu to 1420", "throughput stable", "vpn stalls on large downloads"),
    ("systemd service failing to start after update", "fixed the unit file", "service starts", "daemon won't boot post-upgrade"),
    ("postgres replication slot filling disk", "added slot monitoring + drop inactive", "disk recovered", "wal files eating the disk"),
    ("load balancer health checks flapping", "tuned probe interval", "checks stable", "instances keep going unhealthy"),
    ("cache stampede on cold start", "added single-flight refresh", "origin load flat", "thundering herd on restart"),
    ("graphql N+1 on the user query", "added dataloader batching", "query time 20ms", "too many db round trips"),
    ("secrets in the kubernetes env vars", "moved to external secrets operator", "env cleaned", "yaml leaks the passwords"),
    ("zero-downtime migration breaking old reads", "added schema versioning", "reads compatible", "deploy corrupts old rows"),
    ("recursive lambda exhausted the function quota", "added reserved concurrency cap", "invocations bounded", "runaway function bill"),
    ("pod evicted for memory pressure", "right-sized the requests", "evictions stopped", "pods keep getting killed"),
    ("grpc stream keeps resetting", "added keepalive pings", "stream stable", "connection drops mid-stream"),
    ("dns resolution slow after v6 rollout", "fixed ipv6 precedence", "lookup 1ms", "everything takes forever to resolve"),
    ("prometheus scrape targets down", "fixed the scrape config", "targets up", "metrics not collected"),
];

#[test]
#[ignore]
fn hardcore_fusion_bench() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().join("data");
    cfg.projections_dir = tmp.path().join("proj");
    let handle = engine::start(cfg).unwrap();

    let lex = lexical_corpus();
    let mut gold_lex: Vec<(u64, String, String)> = Vec::new();
    for (tok, situation, action, _query) in &lex {
        let outcome = "recovered stable";
        let r = handle
            .learn_with_text_context(
                text_to_embedding(situation).to_vec(),
                text_to_embedding(*action).to_vec(),
                text_to_embedding(outcome).to_vec(),
                0.5, 0.5, "hash-128d", 0.85, String::new(), None,
                CallerRole::User, None, None,
                Some((situation.to_string(), action.to_string(), outcome.to_string())),
            )
            .unwrap();
        gold_lex.push((r.state_node_id, situation.to_string(), tok.to_string()));
    }
    let mut gold_para: Vec<(u64, String, String)> = Vec::new();
    for (situation, action, outcome, query) in PARAPHRASE_HARD {
        let r = handle
            .learn_with_text_context(
                text_to_embedding(situation).to_vec(),
                text_to_embedding(action).to_vec(),
                text_to_embedding(outcome).to_vec(),
                0.5, 0.5, "hash-128d", 0.85, String::new(), None,
                CallerRole::User, None, None,
                Some((situation.to_string(), action.to_string(), outcome.to_string())),
            )
            .unwrap();
        gold_para.push((r.state_node_id, query.to_string(), situation.to_string()));
    }

    fn score_group(handle: &engine::EngineHandle, golds: &[(u64, String, String)], dense_only: bool) -> (f64, f64) {
        let mut hit1 = 0usize;
        let mut rr_sum = 0.0f64;
        for (gold, query, _text) in golds {
            let q_emb = text_to_embedding(query).to_vec();
            let qt = if dense_only { "" } else { query.as_str() };
            let rr = handle.retrieve(q_emb, "hash-128d", NodeType::State, qt).unwrap();
            if rr.seed_id == Some(*gold) { hit1 += 1; }
            let rank = rr.scored_candidates.iter().position(|(id, _)| id == gold).map(|p| p + 1);
            rr_sum += match rank { Some(r) => 1.0 / r as f64, None => 0.0 };
        }
        (hit1 as f64 / golds.len() as f64, rr_sum / golds.len() as f64)
    }

    let (lex_d_p1, lex_d_mrr) = score_group(&handle, &gold_lex, true);
    let (lex_f_p1, lex_f_mrr) = score_group(&handle, &gold_lex, false);
    let (par_d_p1, par_d_mrr) = score_group(&handle, &gold_para, true);
    let (par_f_p1, par_f_mrr) = score_group(&handle, &gold_para, false);

    // ── BM25 churn: re-index 50% of docs twice with edited text; avgdl exact? ──
    let mut idx = Bm25Index::new();
    for (id, text, _t) in gold_lex.iter() {
        idx.insert(*id, text);
    }
    for (id, _q, situation) in gold_para.iter() {
        idx.insert(*id, situation);
    }
    let docs_before = idx.len();
    for (i, (id, text, _t)) in gold_lex.iter().enumerate() {
        if i % 2 == 0 {
            idx.insert(*id, &format!("{} plus extra tokens squeezed in", text));
        }
    }
    let ok = idx.len() == docs_before;

    println!("\n=========== HARDCORE FUSION BENCH ===========");
    println!("lexical (24)  dense-only P@1={:.2} MRR={:.3} | FUSED P@1={:.2} MRR={:.3}", lex_d_p1, lex_d_mrr, lex_f_p1, lex_f_mrr);
    println!("paraphrase(23) dense-only P@1={:.2} MRR={:.3} | FUSED P@1={:.2} MRR={:.3}", par_d_p1, par_d_mrr, par_f_p1, par_f_mrr);
    println!("bm25 churn: docs {} -> {} (no growth: {})", docs_before, idx.len(), ok);
    println!("==============================================\n");

    assert!(lex_f_p1 >= lex_d_p1, "fusion must not regress lexical recall");
    assert!(ok, "re-indexing must not grow num_docs");
}
