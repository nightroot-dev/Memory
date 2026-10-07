// M1 acceptance: scope layout + exclusive writer lock.
//
// - Layout is versioned; unknown versions fail closed.
// - IDs are opaque (no paths).
// - A second runtime opening the same context is REFUSED loudly
//   (fail-closed); the engine never starts. The refusal (not just the
//   success path) is what this file proves.

use cmhm_core::config::Config;
use cmhm_core::scope::{ScopeId, ScopeLayout, TenantId, SCOPE_LAYOUT_VERSION};
use cmhm_core::{CallerRole, ContextRuntime, ContextType};

fn test_cfg(name: &str) -> (Config, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("cmhm_scope_m1_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = dir.clone();
    cfg.projections_dir = dir.join("projections");
    (cfg, dir)
}

#[test]
fn scope_layout_version_is_recorded() {
    let (cfg, _dir) = test_cfg("version");
    let layout = ScopeLayout::create(
        &cfg.data_dir,
        TenantId::generate(),
        ScopeId::generate(),
        "hash-128d",
    )
    .unwrap();
    let meta = layout.meta().unwrap();
    assert_eq!(meta.version, SCOPE_LAYOUT_VERSION);
    assert_eq!(meta.epoch, 0);
}

#[test]
fn scope_ids_reject_paths() {
    assert!(ScopeId::import("../../etc").is_err());
    assert!(ScopeId::import("").is_err());
    assert!(TenantId::import("a/b").is_err());
}

#[test]
fn tenant_id_is_stable_across_runtimes() {
    let (cfg, _dir) = test_cfg("tenant");
    let rt1 = ContextRuntime::new(cfg.clone(), "s1".into()).unwrap();
    let rt2 = ContextRuntime::new(cfg, "s2".into()).unwrap();
    assert_eq!(rt1.tenant_id().as_str(), rt2.tenant_id().as_str());
}

#[test]
fn second_runtime_is_refused_loudly() {
    let (cfg, _dir) = test_cfg("twowriter");
    let mut rt1 = ContextRuntime::new(cfg.clone(), "s1".into()).unwrap();
    let ctx = rt1
        .create_context("m1", ContextType::Project, None, "m1", vec![])
        .unwrap()
        .id;
    // Load in rt1: adopts scope + takes the lock.
    rt1
        .learn_text_with_context(
            Some(ctx.as_str()),
            vec![0.1; 128],
            vec![0.2; 128],
            vec![0.3; 128],
            0.9,
            0.5,
            "hash-128d",
            0.5,
            String::new(),
            None,
            CallerRole::Agent,
            None,
            None,
            Some(("s".into(), "a".into(), "o".into())),
        )
        .unwrap();

    // Second runtime, same directory: must FAIL loudly, engine never starts.
    let mut rt2 = ContextRuntime::new(cfg, "s2".into()).unwrap();
    let err = rt2
        .retrieve_unscoped(
            Some(ctx.as_str()),
            vec![0.1; 128],
            "hash-128d",
            "",
        )
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("already locked by another writer")
            && msg.contains("Refusing to start a second engine"),
        "second writer must be refused loudly, got: {msg}"
    );
}
