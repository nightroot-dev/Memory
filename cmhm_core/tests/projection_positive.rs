//! Positive-path test for runtime projection registration.
//!
//! Verifies EngineHandle::register_projection enables end-to-end
//! 384-dim learn by providing a 384 -> 128 projection.

use cmhm_core::engine;
use cmhm_core::config::Config;
use cmhm_core::authority::CallerRole;
use cmhm_core::EMBED_DIM;
use tempfile::tempdir;

/// 384 -> 128 identity projection: first 128 dims pass through,
/// remaining 256 discarded. Structurally valid for testing the
/// registration path; not semantically meaningful.
fn identity_384_to_128() -> (usize, Vec<f32>, [f32; EMBED_DIM]) {
    let input_dim = 384;
    let mut matrix = vec![0.0f32; EMBED_DIM * input_dim];
    for r in 0..EMBED_DIM {
        matrix[r * input_dim + r] = 1.0;
    }
    let bias = [0.0f32; EMBED_DIM];
    (input_dim, matrix, bias)
}

#[test]
fn learn_with_384_dim_after_registering_projection_succeeds() {
    let tmp = tempdir().expect("tempdir");
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = tmp.path().join("projections");

    let handle = engine::start(cfg).expect("engine start");

    let (input_dim, matrix, bias) = identity_384_to_128();
    handle
        .register_projection("test-model-384d", input_dim, matrix, bias)
        .expect("register projection");

    let result = handle.learn_with_text_context(
        vec![0.1f32; 384],
        vec![0.2f32; 384],
        vec![0.3f32; 384],
        0.5,
        0.5,
        "test-model-384d",
        0.5,
        String::new(),
        None,
        CallerRole::User,
        None,
        None,
        Some((
            "test state text".to_string(),
            "test action text".to_string(),
            "test outcome text".to_string(),
        )),
    false,
    );

    assert!(
        result.is_ok(),
        "learn with registered projection must succeed, got: {result:?}"
    );
    assert!(handle.node_count() >= 1, "at least one node must be created");

    handle.shutdown().ok();
}

#[test]
fn register_projection_rejects_matrix_length_mismatch() {
    let tmp = tempdir().expect("tempdir");
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = tmp.path().join("projections");

    let handle = engine::start(cfg).expect("engine start");

    let input_dim = 384;
    let matrix = vec![0.0f32; EMBED_DIM * input_dim - 1];
    let bias = [0.0f32; EMBED_DIM];

    let result = handle.register_projection("bad-model", input_dim, matrix, bias);
    assert!(result.is_err(), "wrong matrix length must fail");

    handle.shutdown().ok();
}
