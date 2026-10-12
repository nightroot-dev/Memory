//! M6 restore drill: a real engine scope survives backup → delete → restore.
//!
//! This is the recorded-drill shape from the launch plan: learn, snapshot,
//! back up, destroy the original, restore elsewhere, reopen, counts match.

use cmhm_core::authority::CallerRole;
use cmhm_core::backup::{backup_scope, restore_scope, verify_backup};
use cmhm_core::config::Config;
use cmhm_core::crypto::Keystore;
use cmhm_core::engine;
use cmhm_core::persistence::register_scope_key;
use cmhm_core::text_to_embedding;
use tempfile::tempdir;

#[test]
fn backup_restore_drill() {
    let tmp = tempdir().unwrap();
    let scope_dir = tmp.path().join("scope");
    std::fs::create_dir_all(&scope_dir).unwrap();

    // Live scope with sealed storage (mirrors runtime adopt path).
    let key = Keystore::load_or_create(&scope_dir).unwrap();
    register_scope_key(&scope_dir, key);
    let mut cfg = Config::default();
    cfg.data_dir = scope_dir.clone();
    cfg.projections_dir = scope_dir.join("projections");
    let handle = engine::start(cfg).unwrap();
    for i in 0..5 {
        handle
            .learn_with_text_context(
                text_to_embedding(&format!("drill state {i}")).to_vec(),
                text_to_embedding("probed").to_vec(),
                text_to_embedding("worked").to_vec(),
                0.2,
                0.5,
                "hash-128d",
                0.5,
                String::new(),
                None,
                CallerRole::Agent,
                None,
                None,
                Some((format!("drill state {i}"), "probed".into(), "worked".into())),
            false,
            )
            .unwrap();
    }
    let before = handle.node_count();
    assert!(before > 0);
    handle.shutdown().unwrap();

    // Back up, destroy the original, restore elsewhere, reopen.
    let backups = tmp.path().join("backups");
    let bundle = backup_scope(&scope_dir, "tnt-drill", "scope-drill", 1, &backups).unwrap();
    assert!(verify_backup(&bundle).is_ok());
    std::fs::remove_dir_all(&scope_dir).unwrap();

    let restored = tmp.path().join("restored");
    restore_scope(&bundle, &restored).unwrap();

    // Re-register the restored key (it came back with the bundle).
    let key2 = Keystore::load_or_create(&restored).unwrap();
    register_scope_key(&restored, key2);
    let mut cfg2 = Config::default();
    cfg2.data_dir = restored.clone();
    cfg2.projections_dir = restored.join("projections");
    let h2 = engine::start(cfg2).unwrap();
    assert_eq!(
        h2.node_count(),
        before,
        "restored scope must hold exactly the backed-up nodes"
    );
    h2.shutdown().unwrap();
}
