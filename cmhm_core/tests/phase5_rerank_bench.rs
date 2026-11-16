// cmhm_core/tests/phase5_rerank_bench.rs
//
// Phase 5 Slice 3 gate: held-out git-triplet reranking.
//
// Requires:
//   - tools/phase5_data/test.jsonl (mined from repos NOT in the train split)
//   - tools/phase5_data/model/model_quantized.onnx + tokenizer files
//
// Run explicitly:
//   cargo test -p cmhm_core --features onnx --test phase5_rerank_bench -- --ignored --nocapture
//
// This is deliberately ignored by default: CI does not ship the trained model.
// It measures Stage 1 (BM25+dense RRF) and Stage 2 (cross-encoder) Recall@1/3/5
// on the same candidate pool, so the reranker's contribution is isolated.

#![cfg(feature = "onnx")]

use std::path::{Path, PathBuf};
use std::time::Instant;

use cmhm_core::embed::{Embedder, HashEmbedder};
use cmhm_core::engine::{start, EngineHandle};
use cmhm_core::reranker::{rerank_top_k, ExperienceReranker};
use cmhm_core::{CallerRole, Config, NodeType};

#[derive(Debug, serde::Deserialize)]
struct Row {
    query: String,
    positive: String,
    negative: String,
    #[serde(default)]
    repo: String,
}

fn split_trigger(payload: &str) -> (&str, &str) {
    match payload.split_once(" :: ") {
        Some((trigger, decision)) => (trigger, decision),
        None => (payload, ""),
    }
}

fn load_rows(path: &Path) -> Vec<Row> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Row>(line).expect("invalid test.jsonl row"))
        .collect()
}

fn test_config(data_dir: PathBuf) -> Config {
    Config {
        data_dir: data_dir.clone(),
        projections_dir: data_dir.join("projections"),
        ..Config::default()
    }
}

fn learn_state(
    handle: &EngineHandle,
    embedder: &HashEmbedder,
    trigger: &str,
    decision: &str,
    authority: f32,
) -> u64 {
    let emb = embedder.embed(trigger).expect("embed trigger");
    handle
        .learn_with_text_context(
            emb.clone(),
            embedder.embed("action").expect("embed action"),
            embedder.embed("outcome").expect("embed outcome"),
            0.5,
            0.5,
            "hash-128d",
            authority,
            String::new(),
            None,
            CallerRole::User,
            None,
            None,
            Some((
                trigger.to_string(),
                decision.to_string(),
                "mined git fix".to_string(),
            )),
            false,
        )
        .expect("learn state")
        .state_node_id
}

fn recall_at(ranks: &[Option<usize>], k: usize) -> f32 {
    if ranks.is_empty() {
        return 0.0;
    }
    let hits = ranks
        .iter()
        .filter(|rank| rank.map(|r| r <= k).unwrap_or(false))
        .count();
    hits as f32 / ranks.len() as f32
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

#[test]
#[ignore = "requires mined held-out triplets and the trained ONNX reranker"]
fn phase5_held_out_rerank_recall() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let data_dir = root.join("../tools/phase5_data");
    let test_path = data_dir.join("test.jsonl");
    let model_dir = std::env::var("CMHM_RERANKER_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| data_dir.join("model"));

    let rows = load_rows(&test_path);
    assert!(
        rows.len() >= 20,
        "held-out split too small: {} rows at {}",
        rows.len(),
        test_path.display()
    );
    assert!(
        model_dir.join("model_quantized.onnx").is_file(),
        "missing trained model at {}",
        model_dir.display()
    );

    println!(
        "held-out rows: {} (repos: {:?})",
        rows.len(),
        rows.iter()
            .map(|r| r.repo.as_str())
            .collect::<std::collections::BTreeSet<_>>()
    );

    let mut reranker =
        ExperienceReranker::from_dir(&model_dir).expect("load experience reranker");
    let embedder = HashEmbedder;
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = start(test_config(dir.path().to_path_buf())).expect("engine start");

    // One State node per positive and per hard negative. Positives get a small
    // authority edge so Stage 1 is not artificially handicapped; the negative
    // stays eligible (not contradicted) so it remains a real competitor.
    let mut positive_ids = Vec::with_capacity(rows.len());
    let mut negative_ids = Vec::with_capacity(rows.len());
    for row in &rows {
        let (pos_trigger, pos_decision) = split_trigger(&row.positive);
        let (neg_trigger, neg_decision) = split_trigger(&row.negative);
        positive_ids.push(learn_state(
            &handle,
            &embedder,
            pos_trigger,
            pos_decision,
            0.70,
        ));
        negative_ids.push(learn_state(
            &handle,
            &embedder,
            neg_trigger,
            neg_decision,
            0.55,
        ));
    }

    let mut stage1_ranks = Vec::with_capacity(rows.len());
    let mut stage2_ranks = Vec::with_capacity(rows.len());
    let mut stage1_top25_hits = 0usize;
    let mut latencies_ms = Vec::with_capacity(rows.len());

    for (i, row) in rows.iter().enumerate() {
        let query_emb = embedder.embed(&row.query).expect("embed query");
        let result = handle
            .retrieve(query_emb, "hash-128d", NodeType::State, &row.query)
            .expect("retrieve");

        let positive = positive_ids[i];
        let stage1_ids: Vec<u64> = result
            .scored_candidates
            .iter()
            .take(25)
            .map(|(id, _)| *id)
            .collect();
        let stage1_rank = stage1_ids
            .iter()
            .position(|id| *id == positive)
            .map(|p| p + 1);
        stage1_ranks.push(stage1_rank);
        if stage1_rank.is_some() {
            stage1_top25_hits += 1;
        }

        let candidates: Vec<(u64, String)> = stage1_ids
            .iter()
            .filter_map(|id| {
                handle.get_node_record(*id).map(|record| {
                    let text = if record.trigger_signature.is_empty() {
                        record.text
                    } else {
                        record.trigger_signature
                    };
                    (*id, text)
                })
            })
            .collect();

        let started = Instant::now();
        let reranked = rerank_top_k(
            &mut reranker,
            &row.query,
            &candidates,
            candidates.len().max(1),
        )
        .expect("rerank");
        latencies_ms.push(started.elapsed().as_secs_f64() * 1000.0);

        stage2_ranks.push(
            reranked
                .iter()
                .position(|(id, _)| *id == positive)
                .map(|p| p + 1),
        );

        if i < 3 {
            let neg = negative_ids[i];
            let s1_pos = stage1_rank.unwrap_or(usize::MAX);
            let s1_neg = stage1_ids
                .iter()
                .position(|id| *id == neg)
                .map(|p| p + 1)
                .unwrap_or(usize::MAX);
            let s2_pos = stage2_ranks[i].unwrap_or(usize::MAX);
            let s2_neg = reranked
                .iter()
                .position(|(id, _)| *id == neg)
                .map(|p| p + 1)
                .unwrap_or(usize::MAX);
            println!(
                "sample {i}: stage1 pos={s1_pos} neg={s1_neg} | stage2 pos={s2_pos} neg={s2_neg}"
            );
        }
    }

    let stage1_top25 = stage1_top25_hits as f32 / rows.len() as f32;
    println!("\n=== Phase 5 held-out rerank benchmark ===");
    println!("rows:                 {}", rows.len());
    println!("Stage1 top-25 recall: {:.4}", stage1_top25);
    println!(
        "Stage1 Recall@1/3/5:  {:.4} / {:.4} / {:.4}",
        recall_at(&stage1_ranks, 1),
        recall_at(&stage1_ranks, 3),
        recall_at(&stage1_ranks, 5)
    );
    println!(
        "Stage2 Recall@1/3/5:  {:.4} / {:.4} / {:.4}",
        recall_at(&stage2_ranks, 1),
        recall_at(&stage2_ranks, 3),
        recall_at(&stage2_ranks, 5)
    );
    println!(
        "rerank latency:       mean {:.2} ms, max {:.2} ms (n={})",
        mean(&latencies_ms),
        latencies_ms.iter().copied().fold(0.0, f64::max),
        latencies_ms.len()
    );

    // The plan's target. Stage 2 cannot recover a positive that Stage 1 never
    // put in the top-25 pool, so assert both the ceiling and the final metric.
    assert!(
        stage1_top25 >= 0.85,
        "Stage 1 top-25 recall {:.4} < 0.85: the reranker cannot recover missing positives",
        stage1_top25
    );
    assert!(
        recall_at(&stage2_ranks, 5) >= 0.85,
        "Stage 2 Recall@5 {:.4} < 0.85 target",
        recall_at(&stage2_ranks, 5)
    );
    assert!(
        recall_at(&stage2_ranks, 5) >= recall_at(&stage1_ranks, 5),
        "reranker degraded Recall@5"
    );
}
