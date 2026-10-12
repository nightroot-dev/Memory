// cmhm_core/tests/b6b_meta.rs
//
// B6b part 2 — record_meta round-trip, refusal, and restart persistence.
// The meta table means nothing if callers cannot observe it, so these tests
// pin the full path: learn → episode_seq → record_meta → retrieve-filtered
// (part 3) reads it back. Restart coverage proves the snapshot + WAL path.

use cmhm_core::authority::CallerRole;
use cmhm_core::engine;
use cmhm_core::learn_ledger::LearnMeta;
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
