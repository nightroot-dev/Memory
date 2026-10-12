//! Load + scale harness (M7): replaces estimates with measurements.
//!
//! Default run is CI-small (1k nodes, seconds). Set `CMHM_LOAD_N` for bigger
//! sweeps (e.g. 10000, 100000). Reports (not asserts, except sanity):
//! - bulk learn throughput (learns/s)
//! - retrieve throughput + avg/p99-ish latency at scale
//! - bytes per node on disk (snapshot + WAL)
//! - hash embed batch throughput (sidecar path uses the same fn)
//!
//! Run: cargo test -p cmhm_core --test loadbench -- --ignored --nocapture
//! Big:  CMHM_LOAD_N=100000 <same>

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::engine;
use cmhm_core::node::NodeType;
use cmhm_core::text_to_embedding;
use tempfile::tempdir;

fn load_n() -> usize {
    std::env::var("CMHM_LOAD_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000)
}

/// B1b latency gate: `CMHM_LOAD_LONGTEXT=1` pads state text to ~400 chars so
/// the per-candidate text-copy cost is measured with realistic entries, not
/// just ~40-char L1-length strings. Default off (unchanged behavior).
fn long_text() -> bool {
    matches!(
        std::env::var("CMHM_LOAD_LONGTEXT").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

#[test]
#[ignore]
fn load_sweep() {
    let n = load_n();
    let tmp = tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = tmp.path().join("projections");
    // Keep consolidation out of the measurement.
    cfg.consolidation_node_trigger = 10_000_000;
    cfg.consolidation_reinforce_trigger = 1e18;
    cfg.consolidation_interval_hours = 1e18;

    let t0 = std::time::Instant::now();
    let handle = engine::start(cfg).unwrap();
    for i in 0..n {
        let state_s = format!("loadbench state {i} unique token zb{i}");
        // B1b gate: with CMHM_LOAD_LONGTEXT=1 the state text is padded to
        // ~400 chars with per-learn-unique filler (same shared prefix, so
        // lexical behavior stays comparable). Identical padding would
        // near-dup (cosine >= 0.85) and collapse the bench to ~3 nodes.
        let state_text = if long_text() {
            let filler: String = (0..38).map(|j| format!(" pad{i}_{j}abcdefghij")).collect();
            format!("{state_s}{filler}")
        } else {
            state_s
        };
        handle
            .learn_with_text_context(
                text_to_embedding(&state_text).to_vec(),
                text_to_embedding("probed").to_vec(),
                text_to_embedding("worked").to_vec(),
                0.2,
                0.5,
                "hash-128d",
                0.5,
                String::new(),
                None,
                CallerRole::Agent,
                None,
                None,
                Some((
                    state_text,
                    "probed".into(),
                    "worked".into(),
                )),
            )
            .unwrap();
    }
    let learn_secs = t0.elapsed().as_secs_f64();
    let nodes = handle.node_count();
    assert!(nodes > 0);

    // Retrieve sweep: half lexical hits, half paraphrase-ish misses.
    let k = n.min(200);
    let mut lat: Vec<f64> = Vec::with_capacity(k);
    let mut hits = 0;
    for i in 0..k {
        let q = if i % 2 == 0 {
            format!("what caused zb{}", (i * 7) % n)
        } else {
            format!("mystery symptom number {i}")
        };
        let t = std::time::Instant::now();
        let r = handle
            .retrieve(
                text_to_embedding(&q).to_vec(),
                "hash-128d",
                NodeType::State,
                &q,
            )
            .unwrap();
        lat.push(t.elapsed().as_secs_f64() * 1000.0);
        if !r.post_filter_candidates.is_empty() {
            hits += 1;
        }
    }
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = lat[lat.len() / 2];
    let p99 = lat[(lat.len() * 99 / 100).min(lat.len() - 1)];
    let mean = lat.iter().sum::<f64>() / lat.len() as f64;

    // Embed batch throughput (sidecar uses the same primitive).
    let texts: Vec<String> = (0..1000).map(|i| format!("benchmark text {i}")).collect();
    let t = std::time::Instant::now();
    let mut vecs = 0;
    for chunk in texts.chunks(64) {
        for s in chunk {
            vecs += text_to_embedding(s).len();
        }
    }
    let embed_ms = t.elapsed().as_secs_f64() * 1000.0;

    handle.shutdown().unwrap();
    let bytes = dir_size(tmp.path());

    println!();
    println!("## load sweep (n={n} learns, hash-128d)");
    println!("learn throughput : {:.0} learns/s ({:.1}s total)", n as f64 / learn_secs, learn_secs);
    println!("nodes stored     : {nodes} ({:.1} nodes/learn)", nodes as f64 / n as f64);
    println!("retrieve latency : mean={mean:.2}ms p50={p50:.2}ms p99={p99:.2}ms (k={k})");
    println!("retrieve non-empty pool: {hits}/{k}");
    println!("disk per node    : {:.0} bytes (total {} bytes)", bytes as f64 / nodes.max(1) as f64, bytes);
    println!("hash embed       : {:.0} texts/s", 1000.0 / (embed_ms / 1000.0));
    let _ = vecs;
}
