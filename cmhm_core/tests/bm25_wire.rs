// cmhm_core/tests/bm25_wire.rs
//
// BM25 lexical wire — recall contract tests.
//
// Contract under test:
//   1. `learn_text_with_context` stores scrubbed text on the node (Node.text).
//   2. `retrieve_*` with non-empty `query_text` fuses BM25 candidates via RRF,
//      so a lexically-matching node enters the candidate pool even when its
//      dense cosine ranks outside the top-8.
//   3. Dense-only retrieval (`query_text == ""`) is unchanged: the same node
//      is absent from the pool.
//   4. Text survives a restart: snapshot persists Node.text, WAL V4 (0x04)
//      carries it for un-checkpointed learns, and `start()` rebuilds the
//      in-memory index from stored text.
//
// What this does NOT claim: BM25 does not change final seed scoring
// (Step 2c remains cosine + dense + authority). These tests assert pool
// membership (recall), never the winner. Ranking is dense-led by design.

use cmhm_core::{Config, ContextRuntime, ContextType, CallerRole};

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

fn noisy(base: &[f32], seed: u32) -> Vec<f32> {
    let n = emb(seed);
    let v: Vec<f32> = base.iter().zip(n.iter()).map(|(a, b)| 0.99 * a + 0.01 * b).collect();
    let norm = v.iter().map(|f| f * f).sum::<f32>().sqrt().max(1e-9);
    v.into_iter().map(|f| f / norm).collect()
}

fn make_cfg(test_name: &str) -> (Config, std::path::PathBuf) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cmhm_bm25_{test_name}_{id:06}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = dir.clone();
    cfg.projections_dir = dir.join("projections");
    cfg.max_active_contexts = 4;
    (cfg, dir)
}

const MODEL: &str = "hash-128d";
const RARE_STATE: &str = "xylophone zebra quantum database pool size";
const RARE_QUERY: &str = "xylophone quantum pool";

/// Learn 1 text target + 9 dense-close distractors. Returns (ctx_id, target_state_id).
fn seed_store(rt: &mut ContextRuntime, ctx: Option<&str>) -> u64 {
    // Target: embedding ANTI-aligned to the query (cosine = -1, deterministically
    // last in dense order) but lexically matching via rare keywords.
    let query_emb = emb(7);
    let target_emb: Vec<f32> = query_emb.iter().map(|f| -f).collect();
    let (res, _) = rt
        .learn_text_with_context(
            ctx,
            target_emb,
            emb(1001),
            emb(1002),
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
                RARE_STATE.to_string(),
                "set pool size to 32".to_string(),
                "connection errors stopped".to_string(),
            )),
        )
        .expect("target learn_text_with_context");
    let target_state = res.state_node_id;

    // 9 distractors: dense-near the query (cosine ~0.99), no text (dense-only).
    for i in 0..9u32 {
        let d = noisy(&query_emb, 500 + i);
        rt.learn(
            ctx,
            d,
            emb(2000 + i * 3),
            emb(2001 + i * 3),
            0.9,
            0.5,
            MODEL,
            0.5,
            String::new(),
            None,
        )
        .expect("distractor learn");
    }
    target_state
}

#[test]
fn bm25_fusion_restores_recall_for_lexical_match() {
    let (cfg, _dir) = make_cfg("recall");
    let mut rt = ContextRuntime::new(cfg, "bm25-recall".into()).unwrap();
    let ctx_id = rt
        .create_context("bm25", ContextType::Project, None, "bm25", vec![])
        .unwrap()
        .id;
    let ctx = Some(ctx_id.as_str());
    let target_state = seed_store(&mut rt, ctx);

    let query_emb = emb(7);

    // Dense-only: target is cosine-last → outside the top-8 pool.
    let (dense_res, _) = rt
        .retrieve_unscoped(ctx, query_emb.clone(), MODEL, "")
        .unwrap();
    assert!(
        !dense_res.post_filter_candidates.contains(&target_state),
        "dense-only pool must NOT contain the anti-aligned target"
    );

    // Fused: rare-keyword BM25 hit pulls the target into the pool.
    let (fused_res, _) = rt
        .retrieve_unscoped(ctx, query_emb, MODEL, RARE_QUERY)
        .unwrap();
    assert!(
        fused_res.post_filter_candidates.contains(&target_state),
        "BM25-fused pool MUST contain the lexically-matching target"
    );
}

#[test]
fn bm25_index_survives_restart_via_snapshot_and_rebuild() {
    let (cfg, dir) = make_cfg("restart");
    let target_state: u64;
    let ctx_name = "bm25restart".to_string();
    let ctx_id_str: String;
    {
        let mut rt = ContextRuntime::new(cfg.clone(), "bm25-s1".into()).unwrap();
        let ctx_id = rt
            .create_context(&ctx_name, ContextType::Project, None, "bm25restart", vec![])
            .unwrap()
            .id;
        ctx_id_str = ctx_id.to_string();
        target_state = seed_store(&mut rt, Some(ctx_id.as_str()));
        // Durably persist (snapshot carries Node.text; WAL V4 carries un-checkpointed text).
        rt.checkpoint(Some(ctx_id.as_str())).unwrap();
    }
    // Reopen with a fresh session — forces snapshot load + WAL replay + index rebuild.
    let mut cfg2 = cfg;
    cfg2.data_dir = dir.clone();
    cfg2.projections_dir = dir.join("projections");
    let mut rt2 = ContextRuntime::new(cfg2, "bm25-s2".into()).unwrap();
    let ctx = Some(ctx_id_str.as_str());
    let query_emb = emb(7);
    let (fused_res, _) = rt2
        .retrieve_unscoped(ctx, query_emb, MODEL, RARE_QUERY)
        .unwrap();
    assert!(
        fused_res.post_filter_candidates.contains(&target_state),
        "after restart, BM25-fused pool MUST still contain the target"
    );
}
