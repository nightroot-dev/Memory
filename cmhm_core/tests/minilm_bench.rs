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
    // 40 paraphrase: no shared content tokens
    let pairs = [
        ("database connection pool exhausted under peak load", "why are connections timing out"),
        ("authentication token expired before refresh completed", "users logged out unexpectedly"),
        ("cache eviction rate spiked after memory pressure", "responses are sluggish"),
        ("message queue backlog grew past safe threshold", "processing fell behind"),
        ("disk usage reached critical on primary volume", "storage is almost full"),
        ("kubernetes pod entered crash loop after config change", "keeps restarting"),
        ("TLS handshake latency increased on the ingress", "inbound requests slow to connect"),
        ("background scheduler missed two hourly runs", "scheduled jobs did not execute"),
        ("log aggregator dropped events during rotation", "some lines are missing"),
        ("container image pull slowed by registry throttling", "deployments take longer to start"),
        ("redis replica fell behind primary by seconds", "cache reads return stale data"),
        ("payment processor returned intermittent 502 errors", "checkout randomly fails"),
        ("search index rebuilt slower than expected", "results arrive late"),
        ("API gateway rate limit triggered for bulk requests", "clients hit request limits"),
        ("feature flag rollout exposed bug to small user group", "some users see a defect"),
        ("database migration locked the events table", "writes paused during upgrade"),
        ("session store replication lag grew overnight", "users lose sessions after login"),
        ("cron worker memory grew without bound", "background process exhausted its allocation"),
        ("DNS resolver returned stale entries after config push", "hostnames resolve to old addresses"),
        ("load balancer health check flapped during deploy", "traffic shifted unexpectedly"),
        ("storage tier promotion delayed by replication backlog", "hot data lags on cold media"),
        ("build cache corrupted causing spurious test failures", "CI fails without real cause"),
        ("service mesh sidecar restarted under memory pressure", "calls between systems drop briefly"),
        ("TLS certificate approaching expiry without warning", "browsers show insecure warnings"),
        ("queue consumer crashed mid-batch leaving orphans", "some messages never processed"),
        ("backup job missed window due to upstream delay", "backups are out of date"),
        ("metrics exporter dropped series after schema change", "dashboards show gaps"),
        ("authentication service returned 401 for valid tokens", "legitimate users blocked"),
        ("object store returned slow reads under high concurrency", "downloads are sluggish"),
        ("cron scheduler moved timezone causing missed triggers", "jobs fire at wrong times"),
        ("database vacuum ran longer than maintenance window", "cleanup overran its allotted time"),
        ("CDN edge cache poisoned with stale content", "users see outdated assets"),
        ("certificate rotation broke mutual TLS handshake", "some services cannot connect"),
        ("streaming pipeline backpressure stalled consumers", "real-time data falls behind"),
        ("scheduler fairness starved low-priority jobs", "background tasks never run"),
        ("config reload triggered unintended service restart", "nodes reboot whenever settings refresh"),
        ("distributed lock leaked causing worker contention", "workers stall waiting for locks"),
        ("message dedup key collided across tenants", "some events processed twice"),
        ("query planner chose suboptimal index after stats update", "queries became slow overnight"),
        ("resource quota blocked autoscaler from scaling", "capacity could not grow under load"),
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
    let handle = engine::start(cfg).unwrap();

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

    for (i, (_, q, is_lex)) in scenarios.iter().enumerate() {
        let q_emb = embedder.embed(q).unwrap();
        let r = handle.retrieve(q_emb, embedder.model_id(), NodeType::State, q).unwrap();
        let target = target_ids[i];
        let rank = r.post_filter_candidates.iter().position(|&id| id == target);
        let hit = rank == Some(0);
        let rr = rank.map(|rk| 1.0 / (rk as f64 + 1.0)).unwrap_or(0.0);
        if *is_lex { ln += 1; if hit { lh += 1; } lrr += rr; }
        else       { pn += 1; if hit { ph += 1; } prr += rr; }
    }

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
