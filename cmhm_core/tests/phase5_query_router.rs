// cmhm_core/tests/phase5_query_router.rs
//
// Phase 5 Slice 2 — the query classifier must change the Stage 1 winner.
//
// One pair of State nodes:
//   lexical node: embedding e[1] (cos 0 with the query), text carries a
//                 snake_case symbol plus natural words.
//   dense node:   embedding e[0] (cos 1 with the query), natural text only.
//
// Semantic query -> dense node wins (0.60 dense / 0.40 BM25).
// Lexical query  -> lexical node wins (0.75 BM25 / 0.25 dense).
//
// Without the classifier both queries would return the dense node, so the
// two assertions together prove the router is live.

use cmhm_core::{CallerRole, Config, NodeType, EMBED_DIM};
use cmhm_core::engine::{start, EngineHandle};

fn unit_vec(dim: usize) -> Vec<f32> {
    assert!(dim < EMBED_DIM);
    let mut v = vec![0.0f32; EMBED_DIM];
    v[dim] = 1.0;
    v
}

fn test_config(data_dir: std::path::PathBuf) -> Config {
    Config {
        data_dir: data_dir.clone(),
        projections_dir: data_dir.join("projections"),
        ..Config::default()
    }
}

fn learn_state(handle: &EngineHandle, emb: Vec<f32>, text: &str) -> u64 {
    handle
        .learn_with_text_context(
            emb,
            unit_vec(2),
            unit_vec(3),
            0.5,
            0.5,
            "hash-128d",
            0.5,
            String::new(),
            None,
            CallerRole::User,
            None,
            None,
            Some((
                text.to_string(),
                "unrelated action text".to_string(),
                "unrelated outcome text".to_string(),
            )),
            false,
        )
        .expect("learn failed")
        .state_node_id
}

#[test]
fn classifier_flips_stage1_winner_between_lexical_and_semantic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = start(test_config(dir.path().to_path_buf())).expect("engine start");

    let lexical_id = learn_state(
        &handle,
        unit_vec(1),
        "preferences_reset causes preferences to reset after upgrade",
    );
    let dense_id = learn_state(
        &handle,
        unit_vec(0),
        "settings forgotten after restart",
    );

    // Natural language: no error code, extension, flag or identifier token.
    let semantic = handle
        .retrieve(
            unit_vec(0),
            "hash-128d",
            NodeType::State,
            "why do preferences reset after upgrade",
        )
        .expect("semantic retrieve");
    assert_eq!(
        semantic.seed_id,
        Some(dense_id),
        "semantic query must favor the dense match; lexical node {} was returned instead",
        lexical_id
    );

    // Exact snake_case symbol: classifier must choose 0.75 BM25 / 0.25 dense.
    let lexical = handle
        .retrieve(
            unit_vec(0),
            "hash-128d",
            NodeType::State,
            "preferences_reset",
        )
        .expect("lexical retrieve");
    assert_eq!(
        lexical.seed_id,
        Some(lexical_id),
        "lexical query must favor the exact-symbol node; dense node {} was returned instead",
        dense_id
    );
}
