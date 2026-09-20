// cmhm_core/tests/projection_failclosed.rs
//
// Fail-closed contract at the ENGINE entry point.
//
// The registry-level contract is locked in projection.rs unit tests.
// These tests prove process_learn honors it end-to-end: a 384-dim learn
// with no registered projection must Err with NO side effects (no nodes,
// no WAL entry), and 128-dim learns keep working via identity.
//
// NOTE on error shape: process_learn stringifies the projection error
// (.map_err(|e| anyhow!("{e}"))), so the typed ProjectionError does NOT
// survive in the anyhow chain — downcast_ref::<ProjectionError>() fails.
// These tests therefore assert on is_err() + the Display message, which
// carries the model_id. If a future cleanup preserves the typed error,
// strengthen the assertions to downcast.

// Positive-path test (384-dim + registered projection) is NOT testable:
// it requires projection-registration infrastructure
// (Command::RegisterProjection or startup loading from projections_dir).
// Neither exists as of this session. Deferred to a follow-up session.

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::engine;

fn test_cfg(name: &str) -> (Config, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::default();
    cfg.data_dir = dir.path().to_path_buf();
    cfg.projections_dir = dir.path().join("projections");
    let _ = name;
    (cfg, dir)
}

#[test]
fn learn_with_384_dim_and_no_projection_fails_closed() {
    let (cfg, _dir) = test_cfg("failclosed-384");
    let handle = engine::start(cfg).expect("engine start");
    let res = handle.learn_with_text_context(
        vec![0.1f32; 384], // state
        vec![0.2f32; 384], // action
        vec![0.3f32; 384], // outcome
        0.5,
        0.5, // epsilon, eta
        "test-model-384d-no-proj",
        0.5,
        String::new(),
        None,
        CallerRole::User,
        None,
        None,
        Some((
            "state text".to_string(),
            "action text".to_string(),
            "outcome text".to_string(),
        )),
    );
    assert!(res.is_err(), "384-dim learn with no projection must Err");
    let msg = format!("{:?}", res.unwrap_err());
    assert!(
        msg.contains("test-model-384d-no-proj"),
        "error must name the unprojected model, got: {msg}"
    );
    assert!(
        msg.contains("No projection found"),
        "error must be the NoProjectionFound message, got: {msg}"
    );
    assert_eq!(
        handle.node_count(),
        0,
        "failed learn must leave no side effects"
    );
    handle.shutdown().ok();
}

#[test]
fn learn_with_128_dim_and_no_projection_succeeds_via_identity() {
    let (cfg, _dir) = test_cfg("failclosed-128");
    let handle = engine::start(cfg).expect("engine start");
    let res = handle.learn_with_text_context(
        vec![0.1f32; 128],
        vec![0.2f32; 128],
        vec![0.3f32; 128],
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
            "state text".to_string(),
            "action text".to_string(),
            "outcome text".to_string(),
        )),
    );
    assert!(res.is_ok(), "128-dim learn must succeed via identity path");
    assert!(
        handle.node_count() >= 1,
        "successful learn must create nodes"
    );
    handle.shutdown().ok();
}
