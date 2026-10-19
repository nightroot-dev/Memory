//! R1 live-sandbox boot test — OPTIONAL, ignored by default (S-10 suite-neutral).
//!
//! Reads a SANDBOX COPY of a real context store (never the live data dir) and
//! verifies the S-05 restart-idempotency guarantee on real-world data:
//! two consecutive boots over the same dir must produce identical node count
//! and ledger dimensions. Content-agnostic: no absolute ground truth needed.
//!
//! Usage:
//!   CMHM_R1_SANDBOX_DIR=<sandbox context dir> \
//!   CMHM_R1_SANDBOX_PROJ=<sandbox projections dir> \
//!   cargo test -p cmhm_core --test r1_live_sandbox -- --ignored --nocapture
//!
//! The test fails if boot 2 != boot 1 (the owner's rollback gate: replay did
//! something unexpected on real data). It skips (non-failing) when the env
//! vars are unset so the workspace suite is unaffected.

use cmhm_core::config::Config;
use cmhm_core::engine;

#[test]
#[ignore = "manual live-sandbox check; needs CMHM_R1_SANDBOX_DIR + CMHM_R1_SANDBOX_PROJ"]
fn r1_live_sandbox_two_boots_stable() {
    let dir = std::env::var("CMHM_R1_SANDBOX_DIR")
        .map(std::path::PathBuf::from)
        .ok();
    let proj = std::env::var("CMHM_R1_SANDBOX_PROJ")
        .map(std::path::PathBuf::from)
        .ok();
    let (dir, proj) = match (dir, proj) {
        (Some(d), Some(p)) => (d, p),
        // No sandbox requested — pass trivially (suite-neutral skip).
        _ => {
            eprintln!("r1_live_sandbox: env unset, skipping manually");
            return;
        }
    };

    let mut cfg = Config::default();
    cfg.data_dir = dir.clone();
    cfg.projections_dir = proj;
    // Keep consolidation triggers off: this measures replay idempotency,
    // not consolidation drift (mirrors s05 test_cfg isolation).

    // Mirror the server's per-context scope adoption (scope.rs ensure_key):
    // WITHOUT registering the context's data key, the persistence seam treats
    // the sealed snapshot/WAL as fail-closed-unreadable → fresh empty state
    // (nodes=0), which would make both boots trivially pass. The key must be
    // loaded from the COPIED context dir and registered for THIS process.
    {
        let key = cmhm_core::crypto::Keystore::load_or_create(&dir)
            .expect("sandbox: context key.bin must load");
        cmhm_core::persistence::register_scope_key(&dir, key);
    }

    // Boot 1.
    let (n1, l1, q1) = {
        let handle = engine::start(cfg.clone()).unwrap();
        let n = handle.node_count();
        let l = handle.ledger_len();
        let q = handle.ledger_next_seq();
        handle.shutdown().unwrap();
        (n, l, q)
    };
    eprintln!(
        "r1_live_sandbox boot1: nodes={n1} episodes={l1} next_seq={q1}"
    );

    // Boot 2 — startup compaction should have bounded the tail; identical counts.
    let (n2, l2, q2) = {
        let handle = engine::start(cfg).unwrap();
        let n = handle.node_count();
        let l = handle.ledger_len();
        let q = handle.ledger_next_seq();
        handle.shutdown().unwrap();
        (n, l, q)
    };
    eprintln!(
        "r1_live_sandbox boot2: nodes={n2} episodes={l2} next_seq={q2}"
    );

    assert_eq!(n1, n2, "R1 live-sandbox: node count must NOT change across boots");
    assert_eq!(l1, l2, "R1 live-sandbox: ledger len must NOT change across boots");
    assert_eq!(q1, q2, "R1 live-sandbox: next_seq must NOT change across boots");
}
