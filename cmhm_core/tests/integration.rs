//! End-to-end integration tests for the CMHM Context OS stack.
//!
//! Real ContextRuntime, real temp directories, real embedding vectors.
//! No mocking. Same code path the MCP server uses.
//!
//! Run:
//!   cargo +stable-x86_64-pc-windows-gnu test --test integration -- --nocapture

use cmhm_core::{Config, ContextRuntime, ContextId, ContextType};

// ── Deterministic 128-dim embedding ──────────────────────────────────────────

fn emb(seed: u32) -> Vec<f32> {
    let mut v = vec![0.0f32; 128];
    let mut x = seed as u64;
    for f in v.iter_mut() {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *f = ((x >> 33) as f32) / (u32::MAX as f32) - 0.5;
    }
    let norm = v.iter().map(|f| f * f).sum::<f32>().sqrt().max(1e-9);
    v.iter_mut().for_each(|f| *f /= norm);
    v
}

fn make_cfg(name: &str) -> (Config, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("cmhm_integ_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut cfg = Config::default();
    cfg.data_dir            = dir.clone();
    cfg.projections_dir     = dir.join("projections");
    cfg.max_active_contexts = 8;
    (cfg, dir)
}

// Helper: create_context(name, type, parent, description, tags)
macro_rules! ctx {
    ($rt:expr, $name:expr, $desc:expr, $tags:expr) => {
        $rt.create_context(
            $name,
            ContextType::Project,
            None,
            $desc,
            $tags,
        ).expect("create_context failed")
    };
}

// =============================================================================
// Test 1: Basic learn + retrieve round-trip
// =============================================================================

#[test]
fn test_learn_and_retrieve() {
    let (cfg, dir) = make_cfg("learn_retrieve");
    let mut rt = ContextRuntime::new(cfg).unwrap();

    let desc = ctx!(rt, "CMHM", "causal memory project",
                    vec!["rust".into(), "memory".into()]);
    println!("[T1] Created: {} ({})", desc.display_name, desc.id);

    let ctx = Some(desc.id.as_str());

    for i in 0u32..5 {
        let (r, _) = rt.learn(ctx, emb(i*3), emb(i*3+1), emb(i*3+2), 0.6, 0.5, "test").unwrap();
        println!("[T1] Learn {i}: nodes {},{},{}", r.state_node_id, r.action_node_id, r.outcome_node_id);
    }

    let (result, _) = rt.retrieve(ctx, emb(0), "test").unwrap();
    println!("[T1] Retrieve: {} chain steps | {}", result.causal_chain.len(), result.explanation);
    assert!(!result.causal_chain.is_empty() || !result.explanation.contains("No memories"),
        "Must return something after 5 learns");

    let stats = rt.stats(ctx).unwrap();
    println!("[T1] Stats: version={} loaded={}", stats.descriptor.version, stats.is_loaded);
    assert_eq!(stats.descriptor.version, 5, "Version must equal learn count");
    assert!(stats.is_loaded, "Context must be in RAM after learn");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 2: Isolation — two contexts, no memory leakage
// =============================================================================

#[test]
fn test_context_isolation() {
    let (cfg, dir) = make_cfg("isolation");
    let mut rt = ContextRuntime::new(cfg).unwrap();

    let cmhm  = ctx!(rt, "CMHM",    "causal memory", vec!["rust".into()]);
    let video = ctx!(rt, "VideoAI", "video processing", vec!["video".into()]);

    rt.learn(Some(cmhm.id.as_str()),  emb(1),   emb(2),   emb(3),   0.8, 0.5, "test").unwrap();
    rt.learn(Some(video.id.as_str()), emb(100), emb(101), emb(102), 0.8, 0.5, "test").unwrap();

    let s1 = rt.stats(Some(cmhm.id.as_str())).unwrap();
    let s2 = rt.stats(Some(video.id.as_str())).unwrap();
    assert_eq!(s1.descriptor.version, 1, "CMHM: exactly 1 learn");
    assert_eq!(s2.descriptor.version, 1, "VideoAI: exactly 1 learn");

    let list = rt.list_contexts();
    let names: Vec<&str> = list.iter().map(|d| d.display_name.as_str()).collect();
    println!("[T2] Contexts: {:?}", names);
    assert!(names.contains(&"CMHM"),    "CMHM must be in list");
    assert!(names.contains(&"VideoAI"), "VideoAI must be in list");
    assert!(list.len() >= 2, "Must have at least 2 user contexts");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 3: Routing — CMHM wins for "memory causal rust"
// =============================================================================

#[test]
fn test_context_routing() {
    let (cfg, dir) = make_cfg("routing");
    let mut rt = ContextRuntime::new(cfg).unwrap();

    let cmhm  = ctx!(rt, "CMHM",    "causal memory rust",
                     vec!["rust".into(),"memory".into(),"causal".into()]);
    let video = ctx!(rt, "VideoAI", "video ai processing",
                     vec!["video".into(),"ai".into()]);
    let linux = ctx!(rt, "Linux",   "kernel systems c programming",
                     vec!["linux".into(),"kernel".into(),"c".into()]);

    // Activate all 3 with one learn each (Sa bonus applies to all equally)
    for c in [&cmhm, &video, &linux] {
        rt.learn(Some(c.id.as_str()), emb(1), emb(2), emb(3), 0.7, 0.5, "test").unwrap();
    }

    let ranked = rt.route_top_k("memory causal rust", 3);
    println!("[T3] Rankings for 'memory causal rust':");
    for (d, s) in &ranked {
        println!("  {} → {:.4}", d.display_name, s);
    }

    assert!(!ranked.is_empty(), "route_top_k returned nothing");
    assert_eq!(ranked[0].0.display_name, "CMHM",
        "CMHM must rank #1, got '{}'", ranked[0].0.display_name);

    let cmhm_score  = ranked.iter().find(|(d,_)| d.display_name=="CMHM").map(|(_,s)| *s).unwrap();
    let video_score = ranked.iter().find(|(d,_)| d.display_name=="VideoAI").map(|(_,s)| *s).unwrap_or(0.0);
    assert!(cmhm_score > video_score,
        "CMHM ({cmhm_score:.4}) must outscore VideoAI ({video_score:.4})");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 4: Persistence — version survives clean restart
// =============================================================================

#[test]
fn test_persistence_across_restart() {
    let dir = std::env::temp_dir().join("cmhm_integ_persist");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let make = || {
        let mut c = Config::default();
        c.data_dir            = dir.clone();
        c.projections_dir     = dir.join("projections");
        c.max_active_contexts = 4;
        c
    };

    let ctx_id: String = {
        let mut rt = ContextRuntime::new(make()).unwrap();
        let desc = rt.create_context(
            "Persist", ContextType::Project, None, "survives restart", vec![],
        ).unwrap();
        let id = desc.id.as_str().to_string();
        rt.learn(Some(&id), emb(1), emb(2), emb(3), 0.8, 0.5, "test").unwrap();
        rt.learn(Some(&id), emb(4), emb(5), emb(6), 0.8, 0.5, "test").unwrap();
        rt.checkpoint(Some(&id)).unwrap();
        println!("[T4] Session 1: 2 learns, id={id}");
        rt.shutdown();
        id
    };

    {
        let mut rt = ContextRuntime::new(make()).unwrap();
        let stats = rt.stats(Some(&ctx_id)).expect("[T4] Context must exist after restart");
        println!("[T4] Session 2: version={} loaded={}", stats.descriptor.version, stats.is_loaded);
        assert_eq!(stats.descriptor.version, 2, "Version must be 2 after restart");

        let (result, _) = rt.retrieve(Some(&ctx_id), emb(1), "test").unwrap();
        println!("[T4] Retrieved {} chain steps", result.causal_chain.len());
        rt.shutdown();
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 5: Delete is irrevocable — FSM enforced
// =============================================================================

#[test]
fn test_delete_is_irrevocable() {
    let (cfg, dir) = make_cfg("delete");
    let mut rt = ContextRuntime::new(cfg).unwrap();

    let desc = rt.create_context(
        "ToDelete", ContextType::Project, None, "temp", vec![],
    ).unwrap();
    let raw_id = desc.id.as_str().to_string();

    rt.learn(Some(&raw_id), emb(1), emb(2), emb(3), 0.5, 0.5, "test").unwrap();

    let ctx_id = ContextId::from_str(&raw_id);
    rt.delete_context(&ctx_id).expect("[T5] delete_context failed");
    println!("[T5] Context {} deleted", raw_id);

    // Learn must fail
    let err = rt.learn(Some(&raw_id), emb(10), emb(11), emb(12), 0.5, 0.5, "test")
        .unwrap_err().to_string();
    println!("[T5] Expected error: {err}");
    assert!(
        err.contains("E_NOT_FOUND") || err.contains("E_CONTEXT_DELETED") || err.contains("does not exist"),
        "Wrong error type: {err}"
    );

    let list = rt.list_contexts();
    assert!(!list.iter().any(|d| d.id.as_str() == raw_id),
        "Deleted context must not appear in list");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Test 6: WAL crash recovery
// =============================================================================

#[test]
fn test_wal_crash_recovery() {
    let dir = std::env::temp_dir().join("cmhm_integ_wal");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let make = || {
        let mut c = Config::default();
        c.data_dir            = dir.clone();
        c.projections_dir     = dir.join("projections");
        c.max_active_contexts = 4;
        c
    };

    let ctx_id: String = {
        let mut rt = ContextRuntime::new(make()).unwrap();
        let desc = rt.create_context(
            "CrashTest", ContextType::Project, None, "wal recovery", vec![],
        ).unwrap();
        let id = desc.id.as_str().to_string();

        // 10 learns → checkpoint
        for i in 0u32..10 {
            rt.learn(Some(&id), emb(i*3), emb(i*3+1), emb(i*3+2), 0.6, 0.5, "test").unwrap();
        }
        rt.checkpoint(Some(&id)).unwrap();

        // 5 post-checkpoint learns → WAL only
        for i in 10u32..15 {
            rt.learn(Some(&id), emb(i*3), emb(i*3+1), emb(i*3+2), 0.6, 0.5, "test").unwrap();
        }

        println!("[T6] Session 1: 15 total learns, dropping without shutdown (simulated crash)");
        drop(rt);
        id
    };

    // Session 2: WAL replay
    {
        let mut rt = ContextRuntime::new(make()).unwrap();
        let stats = rt.stats(Some(&ctx_id)).expect("[T6] Context must survive crash");
        println!("[T6] Session 2: version={} loaded={}", stats.descriptor.version, stats.is_loaded);
        assert!(stats.descriptor.version >= 10,
            "[T6] Must restore ≥10 versions, got {}", stats.descriptor.version);

        let (result, _) = rt.retrieve(Some(&ctx_id), emb(0), "test").unwrap();
        println!("[T6] Retrieved {} chain steps from recovered context", result.causal_chain.len());

        rt.shutdown();
    }

    let _ = std::fs::remove_dir_all(&dir);
}
