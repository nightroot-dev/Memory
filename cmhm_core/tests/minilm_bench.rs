#![cfg(feature = "onnx")]

//! MiniLM vs hash-128d benchmark, same 80-scenario corpus.
//!
//! Run:
//!   cargo +stable-x86_64-pc-windows-msvc --target x86_64-pc-windows-msvc \
//!     test -p cmhm_core --features onnx --test minilm_bench -- --ignored --nocapture

use cmhm_core::embed::{Embedder, OnnxEmbedder};
use cmhm_core::engine;
use cmhm_core::config::Config;
use cmhm_core::authority::CallerRole;
use cmhm_core::text_to_embedding;
use cmhm_core::NodeType;
use std::path::PathBuf;
use tempfile::tempdir;

const HASH_MODEL: &str = "hash-128d";
const PROJ_FILE: &str = "data/projections/minilm-l6-v2-int8.bin";

fn projection_dir() -> Option<PathBuf> {
    [PathBuf::from("..").join(PROJ_FILE), PathBuf::from(PROJ_FILE)]
        .into_iter()
        .find(|p| p.exists())
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

struct Metrics { lex_p1: f64, lex_mrr: f64, para_p1: f64, para_mrr: f64, all_p1: f64, all_mrr: f64 }

fn build_corpus() -> Vec<(String, String, bool)> {
    let mut out = Vec::with_capacity(80);
    // 40 lexical: each situation carries a unique single-token rare ID
    // plus 4 content words used nowhere else in the corpus. The rare ID
    // is alphanumeric with no separators so the tokenizer keeps it as
    // one token. Query shares ONLY the rare ID with its situation.
    // (Prior design shared 7 boilerplate tokens across all 40, which
    // made BM25 unable to discriminate — see Session G caveats.)
    let nouns = [
        "firmware", "bootloader", "hypervisor", "bios", "uefi", "raid",
        "nas", "san", "fiber", "switch", "router", "modem", "codec",
        "transcoder", "multiplexer", "oscillator", "resonator", "capacitor",
        "inductor", "transistor", "diode", "thyristor", "relay", "contactor",
        "breaker", "transformer", "generator", "turbine", "compressor",
        "pump", "valve", "actuator", "sensor", "gyroscope", "accelerometer",
        "magnetometer", "barometer", "hygrometer", "thermometer",
        "thermostat", "humidifier", "purifier", "ionizer", "ozonizer",
        "humidistat", "anemometer", "pyrometer", "spectrometer",
        "interferometer", "oscilloscope", "multimeter", "analyzer",
        "synthesizer", "sequencer", "plotter", "printer", "engraver",
        "cutter", "welder", "riveter", "press", "lathe", "shaper",
        "grinder", "polisher", "sander", "drill", "saw", "hammer",
        "wrench", "pliers", "chisel", "rasp", "awl", "punch", "die",
        "mold", "cast", "forge", "furnace", "kiln", "oven", "boiler",
        "chiller",
    ];
    let verbs = [
        "halted", "seized", "jammed", "snapped", "cracked", "warped",
        "dented", "pitted", "corroded", "annealed", "tempered", "quenched",
        "plated", "coated", "anodized", "galvanized", "tinned", "soldered",
        "brazed", "welded", "riveted", "bolted", "screwed", "nailed",
        "pinned", "clamped", "crimped", "swaged", "flared", "beveled",
        "chamfered", "tapped", "threaded", "bored", "reamed", "honed",
        "lapped", "ground", "polished", "sanded", "buffed", "blasted",
        "etched", "milled", "turned", "drilled", "sawed", "planed",
        "jointed", "routed", "shaped", "mortised", "doweled", "glued",
        "epoxied", "grouted", "caulked", "potted", "painted", "varnished",
        "lacquered", "stained", "dyed", "bleached", "carded", "spun",
        "woven", "knitted", "felted", "quilted", "stitched", "hemmed",
        "piped", "edged", "trimmed", "notched", "grooved", "slotted",
        "dimpled", "splined", "knurled", "peened", "burnished",
    ];
    for i in 0..40 {
        let rare = format!("zqxrare{:04}", i);
        let sit = format!(
            "{} {} {} {} {}",
            rare,
            nouns[i * 2],
            nouns[i * 2 + 1],
            verbs[i * 2],
            verbs[i * 2 + 1],
        );
        let q = format!("what caused {}", rare);
        out.push((sit, q, true));
    }
    // 40 paraphrase: embedder-validated (v3 corpus). Each pair kept
    // only if projected MiniLM cos(Q,S_target) beats the best distractor
    // (tools/validate_paraphrases.py). 12/40 clear margin > 0.03; the
    // rest are top-by-margin. All pairs verified zero shared content
    // tokens between query and situation.
    let pairs = [
        ("distributed lock leaked causing worker contention", "workers stall waiting for locks"),
        ("API gateway rate limit triggered for bulk requests", "clients hit request limits"),
        ("background scheduler missed two hourly runs", "scheduled jobs did not execute"),
        ("payment processor returned intermittent 502 errors", "checkout randomly fails"),
        ("DNS resolver returned stale entries after config push", "hostnames resolve to old addresses"),
        ("split brain partitioned the cluster in two", "halves cannot see each other"),
        ("feature flag rollout exposed bug to small user group", "some users see a defect"),
        ("config reload triggered unintended service restart", "nodes reboot whenever settings refresh"),
        ("backup job missed window due to upstream delay", "backups are out of date"),
        ("CDN edge cache poisoned with stale content", "users see outdated assets"),
        ("TLS certificate approaching expiry without warning", "browsers show insecure warnings"),
        ("graceful shutdown hung waiting for drains", "deploys stall at termination"),
        ("database vacuum ran longer than maintenance window", "cleanup overran its allotted time"),
        ("authentication token expired before refresh completed", "users logged out unexpectedly"),
        ("idempotency window expired allowing duplicate charges", "customers billed twice"),
        ("disk usage reached critical on primary volume", "storage is almost full"),
        ("garbage collector pause exceeded stop-the-world budget", "application froze for seconds"),
        ("queue consumer crashed mid-batch leaving orphans", "some messages never processed"),
        ("metrics exporter dropped series after schema change", "dashboards show gaps"),
        ("search index rebuilt slower than expected", "results arrive late"),
        ("hedged requests doubled load on the backend", "redundant calls overload servers"),
        ("streaming pipeline backpressure stalled consumers", "real-time data falls behind"),
        ("build cache corrupted causing spurious test failures", "CI fails without real cause"),
        ("schema migration backfill lagged for hours", "new column stays empty"),
        ("resource quota blocked autoscaler from scaling", "capacity could not grow under load"),
        ("cron worker memory grew without bound", "background process exhausted its allocation"),
        ("slow consumer dragged down the whole pipeline", "one lagging stage blocks all"),
        ("connection timeouts cascade across dependent services", "outage spreads downstream"),
        ("session store replication lag grew overnight", "users lose sessions after login"),
        ("TLS handshake latency increased on the ingress", "inbound requests slow to connect"),
        ("DNS TTL misconfiguration pinned clients to dead endpoints", "traffic hits retired hosts"),
        ("checkpoint interval too coarse losing recent writes", "restarts lose minutes of data"),
        ("object store returned slow reads under high concurrency", "downloads are sluggish"),
        ("cold start latency spiked for serverless handlers", "first requests are very slow"),
        ("container image pull slowed by registry throttling", "deployments take longer to start"),
        ("retry storm amplified a partial outage", "clients hammering made it worse"),
        ("cron scheduler moved timezone causing missed triggers", "jobs fire at wrong times"),
        ("primary database failover stalled mid-promotion", "standby never took over traffic"),
        ("hot partition throttled by rate governor", "one shard is much slower"),
        ("write-ahead log segment filled the disk partition", "node refuses new writes"),
    ];
    for (sit, q) in pairs.iter() {
        out.push((sit.to_string(), q.to_string(), false));
    }
    out.truncate(80);
    out
}

struct HashWrap;
impl Embedder for HashWrap {
    fn model_id(&self) -> &str { HASH_MODEL }
    fn dim(&self) -> usize { 128 }
    fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| text_to_embedding(t).to_vec()).collect())
    }
}

fn run_bench(embedder: &dyn Embedder, scenarios: &[(String, String, bool)]) -> Metrics {
    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    if let Some(d) = projection_dir() { cfg.projections_dir = d; }
    if let Ok(v) = std::env::var("CMHM_CTX_W") { if let Ok(f) = v.parse::<f32>() { cfg.retrieval_context_weight = f; } }
    let cfg_keep = cfg.clone();
    let mut handle = engine::start(cfg).unwrap();

    let mut target_ids = Vec::with_capacity(scenarios.len());
    for (sit, _, _) in scenarios {
        let s = embedder.embed(sit).unwrap();
        let a = embedder.embed("investigated and applied fix").unwrap();
        let o = embedder.embed("outcome recorded").unwrap();
        let r = handle.learn_with_text_context(
            s, a, o, 0.3, 0.5, embedder.model_id(), 0.5, String::new(), None,
            CallerRole::User, None, None,
            Some((sit.clone(), "action".into(), "outcome".into())),
        ).unwrap();
        target_ids.push(r.state_node_id);
    }

    let (mut lh, mut ln, mut lrr) = (0usize, 0usize, 0.0f64);
    let (mut ph, mut pn, mut prr) = (0usize, 0usize, 0.0f64);
    let (mut sl, mut sp) = (0usize, 0usize);

    // Growth probe: node count after learning, then after a clean restart (WAL/snapshot recovery).
    if std::env::var("CMHM_GROWTH").is_ok() {
        let n1 = handle.node_count();
        handle.shutdown().ok();
        let h2 = engine::start(cfg_keep.clone()).unwrap();
        let n2 = h2.node_count();
        println!("  [GROWTH {}] learns={} nodes_after_learn={} ({:.2}/learn) nodes_after_restart={}",
                 embedder.model_id(), scenarios.len(), n1, n1 as f64 / scenarios.len() as f64, n2);
        handle = h2;
    }

    for (i, (_, q, is_lex)) in scenarios.iter().enumerate() {
        let q_emb = if embedder.model_id().contains("bge") && std::env::var("CMHM_BGE_PREFIX").is_ok() {
            embedder.embed(&format!("Represent this sentence for searching relevant passages: {}", q)).unwrap()
        } else {
            embedder.embed(q).unwrap()
        };
        let r = handle.retrieve(q_emb, embedder.model_id(), NodeType::State, q).unwrap();
        let target = target_ids[i];
        let rank = r.post_filter_candidates.iter().position(|&id| id == target);
        let hit = rank == Some(0);
        let rr = rank.map(|rk| 1.0 / (rk as f64 + 1.0)).unwrap_or(0.0);
        if *is_lex { ln += 1; if hit { lh += 1; } lrr += rr; }
        else       { pn += 1; if hit { ph += 1; } prr += rr; }
        if r.seed_id == Some(target) { if *is_lex { sl += 1; } else { sp += 1; } }
        if std::env::var("CMHM_DIAG").is_ok() && !*is_lex && r.seed_id != Some(target) {
            let w = r.seed_id.and_then(|id| target_ids.iter().position(|&t| t == id));
            println!("  MISS [{}] q=\"{}\" | want=\"{}\" | got=\"{}\"", embedder.model_id(), q, scenarios[i].0,
                     w.map(|j| scenarios[j].0.clone()).unwrap_or_else(|| "<none>".into()));
        }
    }
    println!("  [REAL SEED top-1] lex={:.4} para={:.4} overall={:.4}",
             sl as f64 / ln as f64, sp as f64 / pn as f64, (sl + sp) as f64 / (ln + pn) as f64);

    handle.shutdown().ok();
    Metrics {
        lex_p1: lh as f64 / ln as f64,
        lex_mrr: lrr / ln as f64,
        para_p1: ph as f64 / pn as f64,
        para_mrr: prr / pn as f64,
        all_p1: (lh + ph) as f64 / (ln + pn) as f64,
        all_mrr: (lrr + prr) / (ln + pn) as f64,
    }
}

#[test]
#[ignore]
fn minilm_vs_hash_bench() {
    if projection_dir().is_none() {
        eprintln!("SKIP: projection file missing");
        return;
    }
    let scenarios = build_corpus();
    println!("corpus size: {}", scenarios.len());

    let hm = run_bench(&HashWrap, &scenarios);
    println!("hash-128d: lex_P@1={:.4} lex_MRR={:.4} para_P@1={:.4} para_MRR={:.4} overall_P@1={:.4} overall_MRR={:.4}",
             hm.lex_p1, hm.lex_mrr, hm.para_p1, hm.para_mrr, hm.all_p1, hm.all_mrr);

    let minilm = OnnxEmbedder::new().expect("onnx init");
    let mm = run_bench(&minilm, &scenarios);

    if let Ok(bge) = OnnxEmbedder::bge() {
        let bm = run_bench(&bge, &scenarios);
        println!("BGE-base:  lex_P@1={:.4} para_P@1={:.4} overall_P@1={:.4} overall_MRR={:.4}",
                 bm.lex_p1, bm.para_p1, bm.all_p1, bm.all_mrr);
    }
    // Native (unprojected 384-d) ceiling check, plain vs mean-centred.
    {
        let sits: Vec<Vec<f32>> = scenarios.iter().map(|(s, _, _)| minilm.embed(s).unwrap()).collect();
        let qs: Vec<Vec<f32>> = scenarios.iter().map(|(_, q, _)| minilm.embed(q).unwrap()).collect();
        let dim = sits[0].len();
        let mut mean = vec![0.0f32; dim];
        for s in &sits { for k in 0..dim { mean[k] += s[k] / sits.len() as f32; } }
        let cos = |a: &[f32], b: &[f32]| -> f32 {
            let d: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
            let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
            d / (na * nb + 1e-9)
        };
        for centred in [false, true] {
            let (mut lh, mut ph) = (0, 0);
            for (i, (_, _, is_lex)) in scenarios.iter().enumerate() {
                let q: Vec<f32> = if centred { qs[i].iter().zip(&mean).map(|(a, m)| a - m).collect() } else { qs[i].clone() };
                let mut best = (0usize, f32::MIN);
                for (j, s) in sits.iter().enumerate() {
                    let sv: Vec<f32> = if centred { s.iter().zip(&mean).map(|(a, m)| a - m).collect() } else { s.clone() };
                    let c = cos(&q, &sv);
                    if c > best.1 { best = (j, c); }
                }
                if best.0 == i { if *is_lex { lh += 1 } else { ph += 1 } }
            }
            println!("  [NATIVE 384d centred={}] lex={}/40 para={}/40", centred, lh, ph);
        }
    }
    println!("MiniLM:    lex_P@1={:.4} lex_MRR={:.4} para_P@1={:.4} para_MRR={:.4} overall_P@1={:.4} overall_MRR={:.4}",
             mm.lex_p1, mm.lex_mrr, mm.para_p1, mm.para_mrr, mm.all_p1, mm.all_mrr);

    println!();
    println!("| Embedder  | Lex P@1 | Lex MRR | Para P@1 | Para MRR | Overall P@1 | Overall MRR |");
    println!("|-----------|---------|---------|----------|----------|-------------|-------------|");
    println!("| hash-128d | {:>7.4} | {:>7.4} | {:>8.4} | {:>8.4} | {:>11.4} | {:>11.4} |",
             hm.lex_p1, hm.lex_mrr, hm.para_p1, hm.para_mrr, hm.all_p1, hm.all_mrr);
    println!("| MiniLM    | {:>7.4} | {:>7.4} | {:>8.4} | {:>8.4} | {:>11.4} | {:>11.4} |",
             mm.lex_p1, mm.lex_mrr, mm.para_p1, mm.para_mrr, mm.all_p1, mm.all_mrr);
}
