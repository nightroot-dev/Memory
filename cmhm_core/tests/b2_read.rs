// cmhm_core/tests/b2_read.rs
//
// B2 — raw read access to stored records (reviewer build order 2026-10-12).
//
// Retrieval returns ranked chains; these tools dump VERBATIM records so an
// agent (or a debugger) can see exactly what was stored. Read-only: no WAL,
// no state change, no scoring. Uses the same Command-passing pattern as
// NodeCount/GetNodeBeta.
//
// PRESERVATION RULE: failing test → fix production code, not the assertion.

use cmhm_core::authority::CallerRole;
use cmhm_core::engine;
use cmhm_core::{Config, ContextType, NodeType, EMBED_DIM};
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

// ── list returns exactly what was learned, verbatim ─────────────────────

#[test]
fn b2_list_contains_learned_records_verbatim() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut rt = cmhm_core::ContextRuntime::new(
        test_config(dir.path().to_path_buf()),
        "b2".to_string(),
    )
    .expect("runtime");
    let ctx = rt
        .create_context("b2", ContextType::default(), None, "", vec![])
        .expect("create")
        .id
        .to_string();

    // Empty store lists empty (not an error).
    assert!(rt.list_node_records(Some(&ctx), 0, 50).unwrap().is_empty());

    let texts = ["b2 list state one", "b2 list state two"];
    let mut ids = Vec::new();
    for (i, t) in texts.iter().enumerate() {
        let (r, _) = rt
            .learn_text_with_context(
                Some(&ctx),
                unit_vec(10 + i * 10),
                unit_vec(11 + i * 10),
                unit_vec(12 + i * 10),
                0.9,
                0.5,
                "hash-128d",
                0.5,
                String::new(),
                None,
                CallerRole::User,
                None,
                None,
                Some((t.to_string(), "act".to_string(), "out".to_string())),
            false,
            )
            .expect("learn");
        ids.push(r.state_node_id);
    }

    let rows = rt.list_node_records(Some(&ctx), 0, 50).unwrap();
    // 2 learns × 3 nodes each.
    assert_eq!(rows.len(), 6, "list must return every stored node");
    for (id, t) in ids.iter().zip(texts.iter()) {
        let row = rows.iter().find(|r| &r.id == id).unwrap_or_else(|| {
            panic!("learned state node {id} must be listed")
        });
        assert_eq!(row.text, *t, "listed text must be verbatim");
        assert_eq!(row.node_type, NodeType::State);
        assert!(row.timestamp > 0, "listed record must carry a timestamp");
    }
}

// ── pagination is stable and bounded ────────────────────────────────────

#[test]
fn b2_list_pagination_stable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut rt = cmhm_core::ContextRuntime::new(
        test_config(dir.path().to_path_buf()),
        "b2".to_string(),
    )
    .expect("runtime");
    let ctx = rt
        .create_context("b2", ContextType::default(), None, "", vec![])
        .expect("create")
        .id
        .to_string();

    for i in 0..4 {
        rt.learn_with_context(
            Some(&ctx),
            unit_vec(i),
            unit_vec(20 + i),
            unit_vec(40 + i),
            0.9,
            0.5,
            "hash-128d",
            0.5,
            String::new(),
            None,
            CallerRole::User,
            None,
            None,
        false,
        )
        .expect("learn");
    }
    // 4 learns × 3 nodes = 12 rows, ordered by id.
    let p1 = rt.list_node_records(Some(&ctx), 0, 5).unwrap();
    let p2 = rt.list_node_records(Some(&ctx), 5, 5).unwrap();
    let p3 = rt.list_node_records(Some(&ctx), 10, 5).unwrap();
    let p4 = rt.list_node_records(Some(&ctx), 12, 5).unwrap();
    assert_eq!(p1.len(), 5);
    assert_eq!(p2.len(), 5);
    assert_eq!(p3.len(), 2);
    assert!(p4.is_empty(), "past-the-end offset lists empty");
    let mut ids: Vec<u64> = p1.iter().chain(p2.iter()).chain(p3.iter()).map(|r| r.id).collect();
    let sorted = {
        let mut s = ids.clone();
        s.sort_unstable();
        s
    };
    assert_eq!(ids, sorted, "pages must be id-ordered and non-overlapping");
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 12, "pages must cover every node exactly once");
}

// ── get returns one verbatim record; unknown id is None ─────────────────

#[test]
fn b2_get_record_verbatim_and_unknown_is_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut rt = cmhm_core::ContextRuntime::new(
        test_config(dir.path().to_path_buf()),
        "b2".to_string(),
    )
    .expect("runtime");
    let ctx = rt
        .create_context("b2", ContextType::default(), None, "", vec![])
        .expect("create")
        .id
        .to_string();

    assert!(
        rt.get_node_record(Some(&ctx), 999_999_999).unwrap().is_none(),
        "unknown id must be None, not an error"
    );

    let (r, _) = rt
        .learn_text_with_context(
            Some(&ctx),
            unit_vec(30),
            unit_vec(31),
            unit_vec(32),
            0.9,
            0.5,
            "hash-128d",
            0.7,
            "user_explicit".to_string(),
            None,
            CallerRole::User,
            None,
            None,
            Some((
                "b2 get state text".to_string(),
                "b2 get action text".to_string(),
                "b2 get outcome text".to_string(),
            )),
        false,
        )
        .expect("learn");
    let rec = rt
        .get_node_record(Some(&ctx), r.state_node_id)
        .unwrap()
        .expect("learned node must be retrievable");
    assert_eq!(rec.id, r.state_node_id);
    assert_eq!(rec.text, "b2 get state text");
    assert_eq!(rec.node_type, NodeType::State);
    assert!((rec.authority - 0.7).abs() < 1e-4, "authority must round-trip");
    assert_eq!(rec.provenance, "user_explicit");
    assert!(rec.timestamp > 0);

    // No embedding bytes ride along (records stay small by construction).
    let rec_json = serde_json::to_value(&rec).expect("record serializes");
    assert!(
        rec_json.get("embedding").is_none(),
        "records must not carry embeddings"
    );
}
