//! Auto-load test: projections written to cfg.projections_dir are
//! registered by engine::start() and enable 384-dim learn without any
//! explicit register_projection call.

use cmhm_core::engine;
use cmhm_core::config::Config;
use cmhm_core::authority::CallerRole;
use cmhm_core::EMBED_DIM;
use std::path::Path;
use tempfile::tempdir;

/// Write a 384 -> 128 identity projection binary to `dir/{model_id}.bin`.
/// Format: [input_dim: u32 LE][matrix: 128 * input_dim f32 LE][bias: 128 f32 LE]
fn write_identity_projection(dir: &Path, model_id: &str, input_dim: usize) {
    std::fs::create_dir_all(dir).expect("create proj dir");
    let mut bytes = Vec::with_capacity(4 + EMBED_DIM * input_dim * 4 + EMBED_DIM * 4);
    bytes.extend_from_slice(&(input_dim as u32).to_le_bytes());
    for r in 0..EMBED_DIM {
        for c in 0..input_dim {
            let v: f32 = if r == c { 1.0 } else { 0.0 };
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    for _ in 0..EMBED_DIM {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    let path = dir.join(format!("{}.bin", model_id));
    std::fs::write(&path, &bytes).expect("write projection bin");
}

#[test]
fn engine_start_autoloads_projection_from_dir() {
    let tmp = tempdir().expect("tempdir");
    let proj_dir = tmp.path().join("projections");
    write_identity_projection(&proj_dir, "autoload-test-model-384d", 384);

    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = proj_dir;

    let handle = engine::start(cfg).expect("engine start");

    let result = handle.learn_with_text_context(
        vec![0.1f32; 384],
        vec![0.2f32; 384],
        vec![0.3f32; 384],
        0.5,
        0.5,
        "autoload-test-model-384d",
        0.5,
        String::new(),
        None,
        CallerRole::User,
        None,
        None,
        Some((
            "autoload state text".to_string(),
            "autoload action text".to_string(),
            "autoload outcome text".to_string(),
        )),
    false,
    );

    assert!(
        result.is_ok(),
        "learn must succeed via auto-loaded projection, got: {result:?}"
    );
    assert!(handle.node_count() >= 1, "at least one node must be created");

    handle.shutdown().ok();
}

#[test]
fn engine_start_survives_corrupt_projection_file() {
    let tmp = tempdir().expect("tempdir");
    let proj_dir = tmp.path().join("projections");
    std::fs::create_dir_all(&proj_dir).expect("create proj dir");
    std::fs::write(proj_dir.join("corrupt-model.bin"), b"not a real projection")
        .expect("write corrupt bin");

    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = proj_dir;

    let handle = engine::start(cfg).expect("engine must start despite corrupt projection");

    let result = handle.learn_with_text_context(
        vec![0.1f32; 384],
        vec![0.2f32; 384],
        vec![0.3f32; 384],
        0.5,
        0.5,
        "corrupt-model",
        0.5,
        String::new(),
        None,
        CallerRole::User,
        None,
        None,
        None,
    false,
    );
    assert!(
        result.is_err(),
        "learn with unregistered model_id must fail closed"
    );

    handle.shutdown().ok();
}
