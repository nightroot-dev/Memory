//! M3 acceptance: crypto-shredding.
//!
//! 1. With a registered key, learns persist across restart (sealed at rest,
//!    plaintext nowhere on disk).
//! 2. After key destruction, the sealed copies are unrecoverable: restart
//!    loads an empty state (fail-closed, no panic), and no secret bytes
//!    appear in plaintext anywhere.

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::crypto::Keystore;
use cmhm_core::engine;
use cmhm_core::persistence::{register_scope_key, unregister_scope_key};
use cmhm_core::{text_to_embedding, NodeType};
use tempfile::tempdir;

const SECRET: &str = "shred marker zqx secret seven seven";

fn learn_secret(handle: &cmhm_core::engine::EngineHandle) {
    handle
        .learn_with_text_context(
            text_to_embedding(SECRET).to_vec(),
            text_to_embedding("probed it").to_vec(),
            text_to_embedding("it worked").to_vec(),
            // NOTE: prediction_error MUST stay well under the 0.5 force-birth
            // line. This test measures encryption round-trips, not the
            // (pre-existing, separately filed) WAL-replay duplication that
            // force-birth learns trigger on every restart.
            0.2,
            0.5,
            "hash-128d",
            0.5,
            String::new(),
            None,
            CallerRole::Agent,
            None,
            None,
            Some((SECRET.into(), "probed it".into(), "it worked".into())),
        )
        .unwrap();
}

fn cfg_for(dir: &std::path::Path) -> Config {
    let mut cfg = Config::default();
    cfg.data_dir = dir.to_path_buf();
    cfg.projections_dir = dir.join("projections");
    cfg
}

#[test]
fn sealed_round_trip_and_shred() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path();

    // Session 1: register key, learn, shutdown (snapshot + WAL sealed).
    let key = Keystore::load_or_create(dir).unwrap();
    register_scope_key(dir, key);
    let handle = engine::start(cfg_for(dir)).unwrap();
    learn_secret(&handle);
    assert_eq!(handle.node_count(), 3);
    handle.shutdown().unwrap();

    // Ciphertext on disk: secret must not appear in plaintext anywhere.
    let mut found_plaintext = false;
    for entry in walk_files(dir) {
        if let Ok(bytes) = std::fs::read(&entry) {
            if bytes.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()) {
                found_plaintext = true;
            }
        }
    }
    assert!(!found_plaintext, "secret must never touch disk in plaintext");

    // Session 2 (key still registered): full recovery.
    let key2 = Keystore::load_or_create(dir).unwrap();
    register_scope_key(dir, key2);
    let handle2 = engine::start(cfg_for(dir)).unwrap();
    assert_eq!(handle2.node_count(), 3, "sealed state must recover with key");
    let q = text_to_embedding(SECRET).to_vec();
    let r = handle2
        .retrieve(q, "hash-128d", NodeType::State, SECRET)
        .unwrap();
    assert!(
        r.post_filter_candidates.len() >= 1,
        "seed pool must survive sealed restart"
    );
    handle2.shutdown().unwrap();

    // Shred: destroy the key and forget the registration.
    Keystore::destroy(dir).unwrap();
    unregister_scope_key(dir);
    assert!(!Keystore::key_path(dir).exists());

    // Session 3 (no key): fail closed — empty state, no panic, no recovery.
    let handle3 = engine::start(cfg_for(dir)).unwrap();
    assert_eq!(
        handle3.node_count(),
        0,
        "without the key, sealed state must NOT recover"
    );
    handle3.shutdown().unwrap();
}

fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk_files(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}
