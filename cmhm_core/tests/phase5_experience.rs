//! Phase 5: typed experience annotation must survive a WAL-only restart.
//!
//! The mid-life snapshot is taken BEFORE the annotation, so the annotation
//! exists only as WAL record 0x08. Restart must replay it exactly once and
//! expose the trigger signature, decision payload, disposition and blocked
//! process through the read record. A third boot proves snapshot persistence.

use std::time::Duration;

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::engine;
use cmhm_core::learn_ledger::{BlockedProcess, Disposition, ExperienceMeta};

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

fn experience_meta() -> ExperienceMeta {
    ExperienceMeta {
        trigger_signature:
            "[rust | cargo | E0382] cannot move out of borrowed content".to_string(),
        decision_payload: "Clone the value or borrow instead of moving.".to_string(),
        disposition: Disposition::BlockedProcess,
        blocked: Some(BlockedProcess {
            action_signature: "cargo build".to_string(),
            root_cause: "value moved while still borrowed".to_string(),
            recommended_fix: "clone or borrow".to_string(),
            failure_count: 2,
        }),
    }
}

#[test]
fn experience_annotation_survives_wal_only_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let snapshot_path = dir.join("snapshot.bin");
    let state_id;

    {
        let handle = engine::start(test_cfg(&dir)).unwrap();
        let learned = handle
            .learn_with_text_context(
                emb(1),
                emb(2),
                emb(3),
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
                    "cannot move out of borrowed content".to_string(),
                    "cargo build".to_string(),
                    "compile error".to_string(),
                )),
                false,
            )
            .unwrap();
        state_id = learned.state_node_id;

        // Mid-life checkpoint BEFORE the annotation: 0x08 is WAL-only tail.
        handle.snapshot(snapshot_path.clone()).unwrap();
        assert!(
            handle.annotate_experience(state_id, experience_meta()).unwrap(),
            "known node must accept experience metadata"
        );
        assert!(
            !handle.annotate_experience(u64::MAX, experience_meta()).unwrap(),
            "unknown node must return false, never panic or silently succeed"
        );

        // Crash: drop without shutdown, leaving the WAL tail on disk.
        drop(handle);
        std::thread::sleep(Duration::from_millis(300));
    }

    {
        let handle = engine::start(test_cfg(&dir)).unwrap();
        let rec = handle
            .get_node_record(state_id)
            .expect("state node must survive WAL replay");
        assert_eq!(
            rec.trigger_signature,
            "[rust | cargo | E0382] cannot move out of borrowed content"
        );
        assert_eq!(
            rec.decision_payload,
            "Clone the value or borrow instead of moving."
        );
        assert_eq!(rec.disposition, Disposition::BlockedProcess);
        assert_eq!(rec.blocked.as_ref().unwrap().failure_count, 2);
        handle.shutdown().unwrap();
    }

    {
        // Third boot: the shutdown snapshot includes the annotation.
        let handle = engine::start(test_cfg(&dir)).unwrap();
        let rec = handle.get_node_record(state_id).unwrap();
        assert_eq!(rec.disposition, Disposition::BlockedProcess);
        assert_eq!(rec.trigger_signature, experience_meta().trigger_signature);
        handle.shutdown().unwrap();
    }
}
