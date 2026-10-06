// Probe: BM25 top-1 score distribution for paraphrase vs lexical queries.
// Purpose: pick a fusion gate threshold that keeps lexical gain, drops paraphrase noise.
use cmhm_core::bm25::Bm25Index;

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

#[test]
#[ignore]
fn bm25_score_distributions() {
    let mut idx = Bm25Index::new();
    for (i, (s, _)) in PARAPHRASES.iter().enumerate() {
        idx.insert(i as u64, s);
    }
    println!();
    println!("paraphrase BM25 top1 scores (target rank in parens):");
    let mut correct = 0;
    for (i, (_, q)) in PARAPHRASES.iter().enumerate() {
        let res = idx.query(q, 5);
        let top = res.first().map(|(_, s)| *s).unwrap_or(0.0);
        let rank = res.iter().position(|(id, _)| *id == i as u64);
        if rank == Some(0) {
            correct += 1;
        }
        println!("  q{i:02} top={top:.3} rank={rank:?} q={q:?}");
    }
    println!("paraphrase BM25 P@1: {correct}/40");

    // Lexical rare-ID corpus (v3 design): unique token per situation
    let mut lidx = Bm25Index::new();
    for i in 0..40u64 {
        let rare = format!("zqxrare{:04}", i);
        lidx.insert(i, &format!("{rare} firmware bootloader halted seized"));
    }
    println!();
    println!("lexical BM25 top1 scores:");
    let mut lcorrect = 0;
    for i in 0..40u64 {
        let rare = format!("zqxrare{:04}", i);
        let q = format!("what caused {rare}");
        let res = lidx.query(&q, 5);
        let top = res.first().map(|(_, s)| *s).unwrap_or(0.0);
        let rank = res.iter().position(|(id, _)| *id == i);
        if rank == Some(0) {
            lcorrect += 1;
        }
        if i < 5 {
            println!("  lex{i:02} top={top:.3} rank={rank:?}");
        }
    }
    println!("lexical BM25 P@1: {lcorrect}/40");
}
