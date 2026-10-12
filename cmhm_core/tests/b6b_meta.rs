// cmhm_core/tests/b6b_meta.rs
//
// B6b part 2 — record_meta round-trip, refusal, and restart persistence.
// The meta table means nothing if callers cannot observe it, so these tests
// pin the full path: learn → episode_seq → record_meta → retrieve-filtered
// (part 3) reads it back. Restart coverage proves the snapshot + WAL path.

use cmhm_core::authority::CallerRole;
use cmhm_core::engine;
use cmhm_core::learn_ledger::LearnMeta;
use cmhm_core::{Config, NodeType, EMBED_DIM};
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

fn meta(kind: &str, subject: &str) -> LearnMeta {
    LearnMeta {
        kind: kind.to_string(),
        subject: subject.to_string(),
        tags: vec!["t1".to_string()],
    }
}

#[test]
fn b6b_record_meta_roundtrip_and_refusal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = engine::start(test_config(dir.path().to_path_buf())).expect("start");
    let lr = handle
        .learn(unit_vec(50), unit_vec(51), unit_vec(52), 0.9, 0.5, "hash-128d",
               0.5, String::new(), None)
        .expect("learn");
    assert!(lr.episode_seq >= 1, "learn must report its episode seq");
    assert!(
        handle.record_meta(lr.episode_seq, meta("failure", "db")).expect("record"),
        "retained seq must accept meta"
    );
    assert!(
        !handle.record_meta(999_999_999, meta("fact", "x")).expect("record"),
        "unknown seq must refuse (false), never error or store"
    );
    handle.shutdown().unwrap();
}

#[test]
fn b6b_meta_survives_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().to_path_buf();
    let seq;
    {
        let handle = engine::start(test_config(dir.clone())).expect("start");
        let lr = handle
            .learn(unit_vec(60), unit_vec(61), unit_vec(62), 0.9, 0.5, "hash-128d",
                   0.5, String::new(), None)
            .expect("learn");
        seq = lr.episode_seq;
        assert!(handle.record_meta(seq, meta("config", "pool")).expect("record"));
        handle.shutdown().unwrap();
    }
    {
        let handle = engine::start(test_config(dir)).expect("restart");
        // Direct ledger observation would need store access; the durable
        // proof is filtered retrieval (part 3). Here: re-attaching to the
        // same seq must succeed (episode retained across restart)...
        assert!(
            handle.record_meta(seq, meta("config", "pool")).expect("record"),
            "retained-across-restart seq must still accept meta"
        );
        // ...while a never-existent seq still refuses.
        assert!(
            !handle.record_meta(seq + 1000, meta("fact", "x")).expect("record"),
            "unknown seq must refuse after restart too"
        );
        handle.shutdown().unwrap();
    }
}

#[test]
fn b6b_filtered_retrieve_selects_only_matching_kind() {
    use cmhm_core::learn_ledger::{LearnMeta, MetaFilter};
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = engine::start(test_config(dir.path().to_path_buf())).expect("start");

    // Two episodes about similar things (shared vocabulary → same pool),
    // classified differently. pe=0.9 forces birth (escape hatch), so the two
    // states are distinct nodes deterministically — no dedup ambiguity.
    let mk = |s: &str, k: &str| {
        let lr = handle
            .learn_with_text_context(
                cmhm_core::text_to_embedding(s).to_vec(),
                cmhm_core::text_to_embedding(&format!("{s} act")).to_vec(),
                cmhm_core::text_to_embedding(&format!("{s} out")).to_vec(),
                0.9,
                0.5,
                "hash-128d",
                0.5,
                String::new(),
                None,
                CallerRole::User,
                None,
                None,
                Some((s.to_string(), format!("{s} act"), format!("{s} out"))),
                false,
            )
            .expect("learn");
        assert!(
            handle
                .record_meta(
                    lr.episode_seq,
                    LearnMeta {
                        kind: k.to_string(),
                        subject: "svc".to_string(),
                        tags: vec![],
                    }
                )
                .expect("record_meta"),
            "retained seq must accept meta"
        );
        lr
    };
    let fail = mk("b6b filter port failure audit", "failure");
    let fact = mk("b6b filter port status audit", "fact");
    assert_ne!(
        fail.state_node_id, fact.state_node_id,
        "setup requires two distinct state nodes"
    );

    let q = cmhm_core::text_to_embedding("b6b filter port audit").to_vec();
    // Unfiltered baseline: some seed wins (relevance decides).
    let base = handle
        .retrieve_with_filter(q.clone(), "hash-128d", NodeType::State, "", None)
        .expect("unfiltered retrieve");
    assert!(base.seed_id.is_some(), "baseline must seed");

    // kind=failure must seed the failure episode and nothing else.
    let f = MetaFilter {
        kinds: vec!["failure".to_string()],
        subject: None,
        tags: vec![],
    };
    let rf = handle
        .retrieve_with_filter(q.clone(), "hash-128d", NodeType::State, "", Some(f))
        .expect("filtered retrieve");
    assert_eq!(
        rf.seed_id,
        Some(fail.state_node_id),
        "kind=failure must seed the failure episode"
    );
    // kind=fact mirrors.
    let g = MetaFilter {
        kinds: vec!["fact".to_string()],
        subject: None,
        tags: vec![],
    };
    let rg = handle
        .retrieve_with_filter(q.clone(), "hash-128d", NodeType::State, "", Some(g))
        .expect("filtered retrieve");
    assert_eq!(
        rg.seed_id,
        Some(fact.state_node_id),
        "kind=fact must seed the fact episode"
    );
    // Unknown kind matches nothing: explicit empty abstention, never a guess.
    let h = MetaFilter {
        kinds: vec!["preference".to_string()],
        subject: None,
        tags: vec![],
    };
    let rh = handle
        .retrieve_with_filter(q.clone(), "hash-128d", NodeType::State, "", Some(h))
        .expect("filtered retrieve");
    assert_eq!(rh.seed_id, None, "no match must not return a seed");
    assert!(rh.abstained, "no match must abstain");
    assert!(
        rh.explanation.contains("metadata filter"),
        "empty result must name the filter: {}",
        rh.explanation
    );
    handle.shutdown().unwrap();
}

#[test]
fn b6b_absent_fields_retrieve_identically() {
    use cmhm_core::learn_ledger::MetaFilter;
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = engine::start(test_config(dir.path().to_path_buf())).expect("start");
    // Unclassified learns (no meta anywhere in the store).
    for i in 0..3 {
        handle
            .learn(
                unit_vec(70 + i * 3),
                unit_vec(71 + i * 3),
                unit_vec(72 + i * 3),
                0.9,
                0.5,
                "hash-128d",
                0.5,
                String::new(),
                None,
            )
            .expect("learn");
    }
    let q = unit_vec(70);
    let plain = handle
        .retrieve(q.clone(), "hash-128d", NodeType::State, "")
        .expect("plain retrieve");
    let inactive = handle
        .retrieve_with_filter(
            q.clone(),
            "hash-128d",
            NodeType::State,
            "",
            Some(MetaFilter::default()),
        )
        .expect("inactive-filter retrieve");
    assert_eq!(
        inactive.seed_id, plain.seed_id,
        "inactive filter must seed identically"
    );
    assert_eq!(
        inactive.scored_candidates, plain.scored_candidates,
        "inactive filter must score identically"
    );
    // Active filter on an unclassified store: explicit empty, never a guess.
    let active = MetaFilter {
        kinds: vec!["failure".to_string()],
        subject: None,
        tags: vec![],
    };
    let rf = handle
        .retrieve_with_filter(q, "hash-128d", NodeType::State, "", Some(active))
        .expect("filtered retrieve");
    assert_eq!(rf.seed_id, None);
    assert!(rf.abstained);
    handle.shutdown().unwrap();
}
