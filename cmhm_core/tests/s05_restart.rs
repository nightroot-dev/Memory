//! S-05 WAL replay idempotency (R1, ledger S-05).
//!
//! TDD gate: written BEFORE the lifecycle fix — must FAIL on pre-R1 code.
//! Fix is at LIFECYCLE (shutdown compaction + crash-safe replay watermark),
//! NOT at dedup/birth. `birth_node` gate stays untouched.
//!
//! - S05-1 high-surprise: N=10 distinct learns at pe=0.9 → 30 nodes,
//!   ledger 10, next_seq 11 pre-restart; restart must preserve counts.
//!   Pre-fix: up to 60 nodes / 20 episodes (WAL replayed on top of snapshot).
//! - S05-2 low-surprise control: pe=0.1/eta=0.1 identical embeddings →
//!   node count flat before AND after restart (live dedup untouched).
//! - S05-3 crash path: mid-life snapshot → drop WITHOUT shutdown →
//!   restart same dir → counts unchanged (pins watermark; fails shutdown-only).

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::engine;

// ── deterministic helpers (reuse b1b_ledger.rs:45-66 shape) ──────────────

fn emb(seed: u32) -> Vec<f32> {
    let mut v = vec![0.0f32; 128];
    let mut x = seed as u64;
    for f in v.iter_mut() {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *f = ((x >> 33) as f32) / (u32::MAX as f32) - 0.5;
    }
    let norm = v.iter().map(|f| f * f).sum::<f32>().sqrt().max(1e-9);
    v.iter_mut().for_each(|f| *f /= norm);
    v
}

fn test_cfg(dir: &std::path::Path) -> Config {
    let mut cfg = Config::default();
    cfg.data_dir = dir.to_path_buf();
    cfg.projections_dir = dir.join("projections");
    cfg.consolidation_node_trigger = 10_000_000;
    cfg.consolidation_reinforce_trigger = 1e18;
    cfg.consolidation_interval_hours = 1e18;
    cfg
}

fn tmpdir(tag: &str) -> tempfile::TempDir {
    static CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = CTR.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "cmhm_s05_{tag}_{}_{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    tempfile::tempdir_in(std::env::temp_dir()).unwrap()
}

// ── S05-1: high-surprise restart idempotency ─────────────────────────────

#[test]
fn s05_high_surprise_restart_idempotent() {
    let tmp = tmpdir("high");
    let dir = tmp.path().to_path_buf();

    // Session 1: 10 distinct learns at pe=0.9 (escape hatch forces live birth).
    {
        let handle = engine::start(test_cfg(&dir)).unwrap();
        for i in 0..10u32 {
            handle
                .learn_with_context(
                    emb(i * 3 + 1),
                    emb(i * 3 + 2),
                    emb(i * 3 + 3),
                    0.9,
                    0.5,
                    "hash-128d",
                    0.5,
                    String::new(),
                    None,
                    CallerRole::User,
                    None,
                    None,
                )
                .unwrap();
        }
        let n_pre = handle.node_count();
        let l_pre = handle.ledger_len();
        let q_pre = handle.ledger_next_seq();
        assert_eq!(n_pre, 30, "S05-1 pre: 10 distinct pe=0.9 learns must birth 30 nodes");
        assert_eq!(l_pre, 10, "S05-1 pre: ledger must hold 10 episodes");
        assert_eq!(q_pre, 11, "S05-1 pre: next_seq must be 11");
        handle.shutdown().unwrap();
    }

    // Session 2: same dir — snapshot + WAL replay must not duplicate.
    {
        let handle = engine::start(test_cfg(&dir)).unwrap();
        let n_post = handle.node_count();
        let l_post = handle.ledger_len();
        let q_post = handle.ledger_next_seq();
        assert_eq!(n_post, 30, "S05-1 post: node count must survive restart without duplication");
        assert_eq!(l_post, 10, "S05-1 post: ledger len must survive restart without duplication");
        assert_eq!(q_post, 11, "S05-1 post: next_seq must survive restart without duplication");
        handle.shutdown().unwrap();
    }
}

// ── S05-2: low-surprise control (live dedup untouched) ───────────────────

#[test]
fn s05_low_surprise_control_flat() {
    let tmp = tmpdir("low");
    let dir = tmp.path().to_path_buf();
    let s = emb(9001);
    let a = emb(9002);
    let o = emb(9003);

    {
        let handle = engine::start(test_cfg(&dir)).unwrap();
        for _ in 0..10 {
            handle
                .learn_with_context(
                    s.clone(),
                    a.clone(),
                    o.clone(),
                    0.1,
                    0.1,
                    "hash-128d",
                    0.5,
                    String::new(),
                    None,
                    CallerRole::User,
                    None,
                    None,
                )
                .unwrap();
        }
        let n_pre = handle.node_count();
        assert_eq!(n_pre, 3, "S05-2 pre: 10 identical pe=0.1 learns must dedup to 3 nodes");
        let l_pre = handle.ledger_len();
        assert_eq!(l_pre, 10, "S05-2 pre: live ledger still records 10 episodes (T3 semantics)");
        handle.shutdown().unwrap();
    }

    {
        let handle = engine::start(test_cfg(&dir)).unwrap();
        let n_post = handle.node_count();
        let l_post = handle.ledger_len();
        let q_post = handle.ledger_next_seq();
        assert_eq!(n_post, 3, "S05-2 post: node count must stay flat across restart");
        assert_eq!(l_post, 10, "S05-2 post: replayed dedups must not append duplicate episodes");
        assert_eq!(q_post, 11, "S05-2 post: next_seq must stay 11 across restart");
        handle.shutdown().unwrap();
    }
}

// ── S05-3: crash path (mid-life snapshot, no shutdown) ───────────────────

#[test]
fn s05_crash_path_watermark() {
    let tmp = tmpdir("crash");
    let dir = tmp.path().to_path_buf();
    let snap = dir.join("snapshot.bin");

    {
        let handle = engine::start(test_cfg(&dir)).unwrap();
        for i in 0..10u32 {
            handle
                .learn_with_context(
                    emb(i * 3 + 1),
                    emb(i * 3 + 2),
                    emb(i * 3 + 3),
                    0.9,
                    0.5,
                    "hash-128d",
                    0.5,
                    String::new(),
                    None,
                    CallerRole::User,
                    None,
                    None,
                )
                .unwrap();
        }
        assert_eq!(handle.node_count(), 30, "S05-3 pre: 30 nodes before mid-life snapshot");
        assert_eq!(handle.ledger_len(), 10, "S05-3 pre: 10 episodes before mid-life snapshot");
        // Mid-life checkpoint (WAL retained — NOT a terminal shutdown).
        handle.snapshot(snap.clone()).unwrap();
        // Simulate crash: drop WITHOUT shutdown (WAL retained on disk).
        drop(handle);
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    {
        let handle = engine::start(test_cfg(&dir)).unwrap();
        assert_eq!(handle.node_count(), 30, "S05-3 post: crash restart must not duplicate nodes");
        assert_eq!(handle.ledger_len(), 10, "S05-3 post: crash restart must not duplicate episodes");
        assert_eq!(handle.ledger_next_seq(), 11, "S05-3 post: crash restart must preserve next_seq");
        handle.shutdown().unwrap();
    }
}
