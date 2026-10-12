// cmhm_core/tests/b6a_disposition.rs
//
// B6a — visible learn disposition (design note approved 2026-10-12, seq 1 of 2).
//
// The birth/dedup decision was silent: an agent could not tell whether its
// learn created nodes or folded into an existing one (H-12 corrections applied
// invisibly). `LearnResult::disposition` reports what actually happened.
// Observation only — NOTHING about what gets stored changes.
//
// PRESERVATION RULE: failing test → fix production code, not the assertion.

use cmhm_core::authority::CallerRole;
use cmhm_core::engine;
use cmhm_core::LearnDisposition;
use cmhm_core::{Config, EMBED_DIM};
use std::path::PathBuf;

fn unit_vec(dim: usize) -> Vec<f32> {
    assert!(dim < EMBED_DIM);
    let mut v = vec![0.0f32; EMBED_DIM];
    v[dim] = 1.0;
    v
}

fn test_config(data_dir: PathBuf) -> Config {
    Config {
        data_dir: data_dir.clone(),
        projections_dir: data_dir.join("projections"),
        ..Config::default()
    }
}

#[test]
fn b6a_disposition_birthed_then_deduped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = engine::start(test_config(dir.path().to_path_buf())).expect("start");

    // Novel learn → birthed.
    let first = handle
        .learn_with_text_context(
            unit_vec(10),
            unit_vec(11),
            unit_vec(12),
            0.3,
            0.5,
            "hash-128d",
            0.5,
            String::new(),
            None,
            CallerRole::User,
            None,
            None,
            Some((
                "b6a first novel state text".to_string(),
                "b6a first action".to_string(),
                "b6a first outcome".to_string(),
            )),
            false,
        )
        .expect("first learn");
    assert_eq!(
        first.disposition,
        LearnDisposition::Birthed,
        "a novel state must report Birthed"
    );

    // Identical state/action/outcome → all three dedup (cosine 1.0 >= 0.85),
    // and the disposition names the node the state folded into. Low surprise
    // (pe=0.3) so the epsilon>0.5 and salience>0.70 escape hatches stay off and
    // the ordinary near-dup path decides.
    let second = handle
        .learn_with_text_context(
            unit_vec(10),
            unit_vec(11),
            unit_vec(12),
            0.3,
            0.5,
            "hash-128d",
            0.5,
            String::new(),
            None,
            CallerRole::User,
            None,
            None,
            Some((
                "b6a first novel state text".to_string(),
                "b6a first action".to_string(),
                "b6a first outcome".to_string(),
            )),
            false,
        )
        .expect("second learn");
    match second.disposition {
        LearnDisposition::DedupedTo(id) => assert_eq!(
            id, first.state_node_id,
            "the deduped state must name the ORIGINAL node, not a new one"
        ),
        other => panic!("repeat learn must report DedupedTo, got {other:?}"),
    }
    // Dedup semantics unchanged: the same node id comes back.
    assert_eq!(second.state_node_id, first.state_node_id);
}

#[test]
fn b6a_disposition_distinct_states_all_birth() {
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = engine::start(test_config(dir.path().to_path_buf())).expect("start");
    // pe > 0.5 escape hatch guarantees birth even at high cosine; here the
    // states are simply distinct so both birth for the ordinary reason.
    for i in 0..3u32 {
        let r = handle
            .learn_with_text_context(
                unit_vec(10 + i as usize * 20),
                unit_vec(11 + i as usize * 20),
                unit_vec(12 + i as usize * 20),
                0.9,
                0.5,
                "hash-128d",
                0.5,
                String::new(),
                None,
                CallerRole::User,
                None,
                None,
                Some((
                    format!("b6a distinct state {i}"),
                    format!("b6a distinct action {i}"),
                    format!("b6a distinct outcome {i}"),
                )),
                false,
            )
            .expect("learn");
        assert_eq!(
            r.disposition,
            LearnDisposition::Birthed,
            "distinct state {i} must birth"
        );
    }
}

#[test]
fn b6a_strict_no_dup_skips_corrections() {
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = engine::start(test_config(dir.path().to_path_buf())).expect("start");

    // Low-authority learn first (User role passes values through).
    let first = handle
        .learn_with_context(
            unit_vec(20),
            unit_vec(21),
            unit_vec(22),
            0.3,
            0.5,
            "hash-128d",
            0.2,
            "inferred".to_string(),
            None,
            CallerRole::User,
            None,
            None,
            false,
        )
        .expect("first learn");

    // Same state, HIGHER authority, strict=true: same node back, values kept.
    let strict = handle
        .learn_with_context(
            unit_vec(20),
            unit_vec(21),
            unit_vec(22),
            0.3,
            0.5,
            "hash-128d",
            0.9,
            "user_explicit".to_string(),
            None,
            CallerRole::User,
            None,
            None,
            true,
        )
        .expect("strict repeat");
    assert_eq!(strict.state_node_id, first.state_node_id);
    assert_eq!(strict.disposition, LearnDisposition::DedupedTo(first.state_node_id));
    let kept = handle
        .get_node_record(strict.state_node_id)
        .expect("record");
    assert!(
        (kept.authority - 0.2).abs() < 1e-4,
        "strict repeat must NOT raise authority (H-12 skipped), got {}",
        kept.authority
    );
    assert_eq!(kept.provenance, "inferred", "strict repeat must NOT rewrite provenance");

    // Control: same repeat WITHOUT strict applies H-12 and raises authority.
    let loose = handle
        .learn_with_context(
            unit_vec(20),
            unit_vec(21),
            unit_vec(22),
            0.3,
            0.5,
            "hash-128d",
            0.9,
            "user_explicit".to_string(),
            None,
            CallerRole::User,
            None,
            None,
            false,
        )
        .expect("loose repeat");
    assert_eq!(loose.state_node_id, first.state_node_id);
    let raised = handle
        .get_node_record(loose.state_node_id)
        .expect("record");
    assert!(
        (raised.authority - 0.9).abs() < 1e-4,
        "default policy must raise authority (H-12), got {}",
        raised.authority
    );
}
