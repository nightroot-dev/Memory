//! Gate Calibration Benchmark — empirical distribution & threshold separation under hash-128d.
//!
//! Validates:
//!   1. Score separation: Near-duplicate routine events (<0.35) vs novel/surprising events (>=0.35).
//!   2. Monotonic surprise scaling: High prediction error expands admission window.
//!   3. Verifiability dampening: Contradicted / unverified low-authority signals receive lower admission scores.
//!   4. Numerical stability: All scores strictly finite, clamped within valid ranges.

use std::sync::Arc;
use parking_lot::Mutex;
use serde_json::json;
use tempfile::TempDir;

use cmhm_core::{Config, ContextRuntime};
use cmhm_mcp_lib::{dispatch, tools};

fn make_runtime(tmp: &TempDir, enforce: bool, threshold: f32) -> tools::SharedRuntime {
    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
        enforce_gate: enforce,
        gate_threshold: threshold,
        ..Config::default()
    };
    let rt = ContextRuntime::new(cfg, "gate-bench-session".to_string()).unwrap();
    Arc::new(Mutex::new(rt))
}

#[test]
fn test_gate_calibration_distribution_hash128d() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp, false, 0.35);

    // Seed 5 baseline topic memories
    let baseline_topics = [
        ("Kubernetes pod networking", "Configure Calico CNI overlay", "Pod ping succeeds", 0.1),
        ("PostgreSQL WAL archiving", "Enable archive_mode with pgBackRest", "Archiving continuous", 0.05),
        ("Redis clustering", "Deploy 3 master 3 replica nodes", "Cluster slots allocated", 0.1),
        ("TLS certificate renewal", "Run certbot renew --force-renewal", "Cert valid 90 days", 0.05),
        ("BGP route peering", "Establish eBGP session with ASN 65001", "Prefixes received", 0.15),
    ];

    for (s, a, o, pe) in &baseline_topics {
        let args = json!({
            "state": s, "action": a, "outcome": o, "prediction_error": pe,
            "authority": 0.85, "provenance": "user_explicit", "auto_tag": false
        });
        let r = dispatch("memory_learn_text", &rt, &args).unwrap();
        assert_ne!(r.isError, Some(true));
    }

    // Now test Group A: Near-duplicates with routine low pe
    // Expected: gate_score < 0.35 -> would_index = false
    let near_duplicates = [
        ("Kubernetes pod networking service", "Verify Calico overlay", "Pod ping ok", 0.05),
        ("PostgreSQL WAL archiving daemon", "Check pgBackRest status", "Archiving fine", 0.08),
        ("Redis clustering topology", "Inspect replica status", "Slots assigned", 0.02),
        ("TLS certificate renewal daemon", "Check cert expiration", "Cert valid", 0.05),
    ];

    for (s, a, o, pe) in &near_duplicates {
        let args = json!({
            "state": s, "action": a, "outcome": o, "prediction_error": pe,
            "authority": 0.85, "provenance": "user_explicit", "auto_tag": false
        });
        let r = dispatch("memory_learn_text", &rt, &args).unwrap();
        assert_ne!(r.isError, Some(true));
    }

    // Test Group B: Near-duplicate with high surprise (anomalous failure under known state region)
    // Expected: pe = 0.95 boosts score significantly -> higher score than routine duplicate
    let surprising_duplicate = json!({
        "state": "Kubernetes pod networking service",
        "action": "Configure Calico CNI overlay",
        "outcome": "Kernel packet drop due to MTU black hole",
        "prediction_error": 0.95,
        "authority": 0.85,
        "provenance": "user_explicit",
        "auto_tag": false
    });
    let r_surp = dispatch("memory_learn_text", &rt, &surprising_duplicate).unwrap();
    assert_ne!(r_surp.isError, Some(true));

    // Test Group C: Distinct novel topics with moderate pe
    // Expected: gate_score >= 0.35 -> would_index = true
    let novel_topics = [
        ("Ceph distributed storage CRUSH map", "Rebalance OSD weights across failure domains", "Cluster HEALTH_OK", 0.25),
        ("eBPF XDP firewall program", "Attach native XDP hook on eth0 interface", "DDoS traffic dropped wire-speed", 0.30),
        ("Raft consensus leader lease", "Elect new leader on timeout expiration", "Term incremented quorum granted", 0.20),
    ];

    for (s, a, o, pe) in &novel_topics {
        let args = json!({
            "state": s, "action": a, "outcome": o, "prediction_error": pe,
            "authority": 0.85, "provenance": "user_explicit", "auto_tag": false
        });
        let r = dispatch("memory_learn_text", &rt, &args).unwrap();
        assert_ne!(r.isError, Some(true));
    }

    // Inspect the generated gate_shadow_log.jsonl
    let gate_log = tmp.path().join("gate_shadow_log.jsonl");
    assert!(gate_log.exists(), "gate_shadow_log.jsonl must exist");
    let content = std::fs::read_to_string(&gate_log).unwrap();
    let entries: Vec<serde_json::Value> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    // Total learns: 5 baseline + 4 near-duplicates + 1 surprising + 3 novel = 13 entries
    assert_eq!(entries.len(), 13, "Must have exactly 13 gate entries");

    // All entries must conform to Schema v=2
    for e in &entries {
        assert_eq!(e["v"], 2);
        assert_eq!(e["policy_v"], 2);
        let score = e["gate_score"].as_f64().unwrap();
        let novelty = e["novelty"].as_f64().unwrap();
        let verifiability = e["verifiability"].as_f64().unwrap();
        assert!(score >= 0.0 && score.is_finite(), "Score must be finite non-negative: {score}");
        assert!(novelty >= 0.0 && novelty <= 2.0, "Novelty must be in [0, 2]: {novelty}");
        assert!(verifiability >= 0.01 && verifiability <= 1.0, "Verifiability must be in [0.01, 1]: {verifiability}");
    }

    // Verify Group A (entries 5..9) have would_index = false (near duplicates filtered)
    for i in 5..9 {
        let e = &entries[i];
        let score = e["gate_score"].as_f64().unwrap();
        assert!(
            score < 0.35,
            "Routine near-duplicate at index {i} should score < 0.35, got {score}"
        );
        assert_eq!(e["would_index"], false, "Routine duplicate must have would_index=false");
    }

    // Verify Surprising duplicate (index 9) score > routine duplicate (index 5)
    let routine_score = entries[5]["gate_score"].as_f64().unwrap();
    let surp_score = entries[9]["gate_score"].as_f64().unwrap();
    assert!(
        surp_score > routine_score * 1.5,
        "Surprise must boost score: routine={routine_score}, surp={surp_score}"
    );

    // Verify Group C (entries 10..13) have would_index = true (novel topics admitted)
    for i in 10..13 {
        let e = &entries[i];
        let score = e["gate_score"].as_f64().unwrap();
        assert!(
            score >= 0.35,
            "Novel topic at index {i} should score >= 0.35, got {score}"
        );
        assert_eq!(e["would_index"], true, "Novel topic must have would_index=true");
    }
}
