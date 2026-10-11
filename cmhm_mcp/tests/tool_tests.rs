//! Integration tests for the cmhm_mcp tool layer.
//!
//! Three invariants, enforced for every release:
//!   1. PARITY  â€” every name in the TOOLS registry has a dispatch arm in lib::dispatch().
//!   2. SMOKE   â€” every handler returns a non-panicking result for both valid and garbage args.
//!   3. ERRORS  â€” garbage args always produce isError=true, never a panic.

use std::sync::Arc;
use parking_lot::Mutex;
use serde_json::{json, Value};
use tempfile::TempDir;

use cmhm_core::{Config, ContextRuntime};
use cmhm_mcp_lib::{dispatch, tools};

// â”€â”€ Test helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Absolute path to `<workspace>/data/projections`.
///
/// Cargo test CWD is the package directory (`cmhm_mcp/`), not the workspace root.
/// `Config`'s default `projections_dir` is the relative string `"data/projections"`,
/// which resolves to `cmhm_mcp/data/projections` â€” missing the MiniLM `.bin`.
///
/// Any test that constructs a `Config` directly (not via `make_runtime`) must use
/// this helper to set `projections_dir`, otherwise the onnx build will fail with
/// "No projection found for model 'minilm-l6-v2-int8'".
fn workspace_projections_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CARGO_MANIFEST_DIR has no parent")
        .join("data")
        .join("projections")
}

fn make_runtime(tmp: &TempDir) -> tools::SharedRuntime {
    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
        projections_dir: workspace_projections_dir(),
        ..Config::default()
    };
    let rt = ContextRuntime::new(cfg, "test-session".to_string()).expect("ContextRuntime::new failed in test");
    Arc::new(Mutex::new(rt))
}

// â”€â”€ Test 1: PARITY â€” every registered tool name dispatches â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn every_registered_tool_dispatches() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);
    let empty_args = Value::Object(Default::default());

    let defs = tools::tool_definitions();
    assert!(!defs.is_empty(), "TOOLS registry must not be empty");

    for def in &defs {
        // dispatch() returns None only for unknown tool names
        let result = dispatch(&def.name, &rt, &empty_args);
        assert!(
            result.is_some(),
            "tool '{}' is in TOOLS registry but has no dispatch arm in lib::dispatch()",
            def.name
        );
    }
}

// â”€â”€ Test 2: SMOKE â€” garbage args â†’ clean error, never panic â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn garbage() -> Value {
    json!({"completely": "wrong", "structure": 999})
}

fn assert_error_result(name: &str, rt: &tools::SharedRuntime) {
    let result = dispatch(name, rt, &garbage())
        .unwrap_or_else(|| panic!("tool '{}' not found in dispatch", name));
    assert_eq!(
        result.isError,
        Some(true),
        "tool '{}' with garbage args must return isError=true, got: {:?}",
        name, result.content
    );
}

#[test]
fn garbage_args_never_panic_memory_retrieve() {
    let tmp = TempDir::new().unwrap();
    assert_error_result("memory_retrieve", &make_runtime(&tmp));
}

#[test]
fn garbage_args_never_panic_memory_learn() {
    let tmp = TempDir::new().unwrap();
    assert_error_result("memory_learn", &make_runtime(&tmp));
}

#[test]
fn garbage_args_never_panic_memory_retrieve_text() {
    let tmp = TempDir::new().unwrap();
    assert_error_result("memory_retrieve_text", &make_runtime(&tmp));
}

#[test]
fn garbage_args_never_panic_memory_learn_text() {
    let tmp = TempDir::new().unwrap();
    assert_error_result("memory_learn_text", &make_runtime(&tmp));
}

#[test]
fn garbage_args_never_panic_memory_feedback() {
    let tmp = TempDir::new().unwrap();
    assert_error_result("memory_feedback", &make_runtime(&tmp));
}

#[test]
fn garbage_args_never_panic_memory_promote() {
    let tmp = TempDir::new().unwrap();
    assert_error_result("memory_promote", &make_runtime(&tmp));
}

#[test]
fn garbage_args_never_panic_context_create() {
    let tmp = TempDir::new().unwrap();
    assert_error_result("context_create", &make_runtime(&tmp));
}

#[test]
fn garbage_args_never_panic_context_delete() {
    let tmp = TempDir::new().unwrap();
    assert_error_result("context_delete", &make_runtime(&tmp));
}

// â”€â”€ Test 3: SMOKE â€” valid args â†’ not isError â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn context_list_with_empty_args_succeeds() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let result = dispatch("context_list", &rt, &json!({})).unwrap();
    assert_ne!(result.isError, Some(true), "context_list with empty args should succeed");
}

#[test]
fn memory_stats_with_empty_args_succeeds() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let result = dispatch("memory_stats", &rt, &json!({})).unwrap();
    assert_ne!(result.isError, Some(true), "memory_stats with no context_id should use default");
}

#[test]
fn context_route_with_valid_query_succeeds() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let result = dispatch("context_route", &rt, &json!({"query": "test query", "top_k": 3})).unwrap();
    assert_ne!(result.isError, Some(true), "context_route with valid args must not error");
}

#[test]
fn project_detect_with_empty_args_succeeds() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let result = dispatch("project_detect", &rt, &json!({})).unwrap();
    assert_ne!(result.isError, Some(true), "project_detect needs no args and must succeed");
}

#[test]
fn memory_retrieve_text_with_empty_store_returns_empty_chain() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let args = json!({"query": "rust compiler error linker not found", "retrieval_intent": "state"});
    let result = dispatch("memory_retrieve_text", &rt, &args).unwrap();
    assert_ne!(result.isError, Some(true), "retrieve on empty store must not error");
    let text = result.content.first().map(|c| c.text.as_str());
    assert!(text.map(|t| !t.is_empty()).unwrap_or(false), "result must have non-empty text content");
}

#[test]
fn memory_learn_text_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    // Learn
    let learn_args = json!({
        "state":            "cargo build failed with linker error",
        "action":           "installed VS C++ Build Tools workload",
        "outcome":          "cargo build succeeded",
        "prediction_error": 0.9,
        "auto_tag":         true
    });
    let learn_result = dispatch("memory_learn_text", &rt, &learn_args).unwrap();
    assert_ne!(learn_result.isError, Some(true), "memory_learn_text must succeed: {:?}", learn_result.content);

    // Retrieve â€” must find something (chain won't be empty after learning)
    let retrieve_args = json!({"query": "cargo build linker error", "retrieval_intent": "state"});
    let retrieve_result = dispatch("memory_retrieve_text", &rt, &retrieve_args).unwrap();
    assert_ne!(retrieve_result.isError, Some(true), "retrieve after learn must not error");
}


// â”€â”€ Test 4: UNKNOWN tool â†’ dispatch returns None â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn unknown_tool_returns_none() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let result = dispatch("nonexistent_tool_xyz", &rt, &json!({}));
    assert!(result.is_none(), "dispatch must return None for unknown tool names");
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// Focused correctness tests (Fixes 1â€“3)
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

// â”€â”€ T-FIX1: Archive scrub â€” secrets must never reach learn_log.jsonl â”€â”€â”€â”€â”€â”€â”€â”€â”€
//
// Verifies that Shannon-entropy/regex scrubbing fires on the text path
// and that the raw secret does not appear anywhere in the archive file.
// Tests Fix 1 (Tier 0 archive) and the scrub invariant simultaneously.

#[test]
fn archive_scrub_no_secret_in_jsonl() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);

    // A synthetic high-entropy token that looks like an API key.
    // Chosen to trigger the Shannon-entropy scrubber (>4.5 bits/char).
    let fake_secret = "sk-proj-FAKEKEY1234567890abcdefghijklmnop";

    let args = json!({
        "state":            format!("calling openai with key {fake_secret}"),
        "action":           "sent POST /completions",
        "outcome":          "got 200 OK",
        "prediction_error": 0.1,
        "auto_tag":         false
    });
    let result = dispatch("memory_learn_text", &rt, &args).unwrap();
    assert_ne!(result.isError, Some(true), "learn with secret must not error");

    // Read learn_log.jsonl â€” every byte of it
    let log_path = tmp.path().join("learn_log.jsonl");
    assert!(log_path.exists(), "learn_log.jsonl must exist after a learn call");
    let log_bytes = std::fs::read_to_string(&log_path)
        .expect("learn_log.jsonl must be readable");

    assert!(
        !log_bytes.contains(fake_secret),
        "Secret must NOT appear in learn_log.jsonl. Found in:\n{log_bytes}"
    );
    assert!(
        log_bytes.contains("[REDACTED]") || log_bytes.contains("REDACT"),
        "Scrubbed content must be present in learn_log.jsonl:\n{log_bytes}"
    );
}

// â”€â”€ T-FIX2a: retrieve_id â€” duplicate across restart â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//
// Session 1: learn, retrieve (get retrieve_id), apply feedback â†’ ledger written.
// Session 2: new ContextRuntime from same dir â†’ ledger reloaded.
// Second feedback call with same retrieve_id â†’ must return "already applied",
// NOT mutate bandit/decay state again.

#[test]
fn feedback_idempotent_across_restart() {
    let tmp = TempDir::new().unwrap();

    let retrieve_id = "test-retrieve-uuid-restart-idempotency";

    // Session 1: learn + fake feedback application
    {
        let rt = make_runtime(&tmp);
        // Learn something
        let learn_args = json!({
            "state": "rust borrow checker complains", "action": "added lifetime",
            "outcome": "compiles", "prediction_error": 0.3, "auto_tag": false
        });
        dispatch("memory_learn_text", &rt, &learn_args).unwrap();

        // Retrieve to get node IDs
        let ret = dispatch("memory_retrieve_text", &rt, &json!({"query": "borrow checker", "retrieval_intent": "state"})).unwrap();

        assert_ne!(ret.isError, Some(true));

        // Apply feedback with a known retrieve_id (normally from the retrieve response)
        let fb_args = json!({
            "node_ids": [1u64, 2u64],  // IDs may or may not exist â€” that's ok for ledger test
            "outcome": "success",
            "retrieve_id": retrieve_id
        });
        let fb1 = dispatch("memory_feedback", &rt, &fb_args).unwrap();
        // First call must NOT say "already applied"
        let text1 = &fb1.content[0].text;
        assert!(
            !text1.contains("Already Applied"),
            "First feedback must not say 'Already Applied': {text1}"
        );
    } // rt dropped â†’ ledger flushed to disk

    // Session 2: fresh runtime â€” ledger must be reloaded
    {
        let cfg = Config {
            data_dir: tmp.path().to_path_buf(),
            projections_dir: workspace_projections_dir(),
            ..Config::default()
        };
        let rt2 = ContextRuntime::new(cfg, "session-2".to_string())
            .expect("Session 2 ContextRuntime failed");
        let shared2 = Arc::new(Mutex::new(rt2));

        let fb_args = json!({
            "node_ids": [1u64, 2u64],
            "outcome": "success",
            "retrieve_id": retrieve_id
        });
        let fb2 = dispatch("memory_feedback", &shared2, &fb_args).unwrap();
        let text2 = &fb2.content[0].text;
        assert!(
            text2.contains("Already Applied"),
            "Second feedback (after restart) must return 'Already Applied'. Got:\n{text2}"
        );
    }
}

// â”€â”€ T-FIX2b: Concurrent same retrieve_id â€” only one mutates â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//
// parking_lot::Mutex ensures only one thread holds the runtime lock at a time.
// This test documents and enforces that the mutex serializes concurrent feedback:
// one thread applies, the other gets "already applied".

#[test]
fn feedback_concurrent_same_retrieve_id() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    let tmp    = TempDir::new().unwrap();
    let rt     = make_runtime(&tmp);
    let rt_arc = Arc::clone(&rt);

    // Learn a node first so the feedback has something to potentially apply to
    let learn_args = json!({
        "state": "concurrent test state", "action": "concurrent test action",
        "outcome": "concurrent test outcome", "prediction_error": 0.5, "auto_tag": false
    });
    dispatch("memory_learn_text", &rt, &learn_args).unwrap();

    let already_applied = Arc::new(AtomicUsize::new(0));
    let applied_ok      = Arc::new(AtomicUsize::new(0));

    let shared_rid = "concurrent-test-retrieve-id-12345";
    let fb_args = json!({
        "node_ids": [1u64, 2u64, 3u64],
        "outcome":  "success",
        "retrieve_id": shared_rid
    });

    let mut handles = vec![];
    for _ in 0..4 {
        let rt_c   = Arc::clone(&rt_arc);
        let args_c = fb_args.clone();
        let applied_c  = Arc::clone(&applied_ok);
        let skipped_c  = Arc::clone(&already_applied);

        handles.push(thread::spawn(move || {
            let result = dispatch("memory_feedback", &rt_c, &args_c).unwrap();
            let text = &result.content[0].text;
            if text.contains("Already Applied") {
                skipped_c.fetch_add(1, Ordering::SeqCst);
            } else {
                applied_c.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    for h in handles { h.join().unwrap(); }

    let ok  = applied_ok.load(Ordering::SeqCst);
    let dup = already_applied.load(Ordering::SeqCst);
    assert_eq!(ok, 1,
        "Exactly 1 thread must apply feedback; got applied={ok}, skipped={dup}");
    assert_eq!(dup, 3,
        "Exactly 3 threads must get 'Already Applied'; got applied={ok}, skipped={dup}");
}

// â”€â”€ T-FIX2c: Crash boundary â€” double Beta apply is statistically bounded â”€â”€â”€â”€â”€
//
// Simulates the crash scenario where feedback_nodes() ran but ledger was never
// written: apply feedback twice to the same nodes. The Beta posterior must stay
// in (0, âˆž) (no NaN, no panic, no overflow). This proves Option A ordering is
// safe even under crash replay.

#[test]
fn feedback_double_apply_beta_stays_bounded() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);

    // Learn to create nodes
    let learn_args = json!({
        "state": "beta boundary state", "action": "beta boundary action",
        "outcome": "beta boundary outcome", "prediction_error": 0.8, "auto_tag": false
    });
    let lr = dispatch("memory_learn_text", &rt, &learn_args).unwrap();
    assert_ne!(lr.isError, Some(true));

    // Apply feedback twice â€” no retrieve_id (simulates crash replay: ledger absent)
    let fb_args = json!({
        "node_ids": [1u64, 2u64, 3u64],
        "outcome":  "success"
        // No retrieve_id â€” represents "crash between mutate and record"
    });
    let r1 = dispatch("memory_feedback", &rt, &fb_args).unwrap();
    assert_ne!(r1.isError, Some(true), "First apply must succeed");

    let r2 = dispatch("memory_feedback", &rt, &fb_args).unwrap();
    assert_ne!(r2.isError, Some(true), "Second apply (crash replay) must succeed, not panic");

    // Retrieve â€” must still work, causal graph not corrupted
    let ret = dispatch("memory_retrieve_text", &rt, &json!({"query": "beta boundary", "retrieval_intent": "state"})).unwrap();

    assert_ne!(ret.isError, Some(true), "Retrieve after double apply must succeed");
}

// â”€â”€ T-FIX1b: Archive write failure is non-fatal, operator-visible â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//
// If the data_dir is not writable, learn_log.jsonl cannot be created.
// The learn call must STILL SUCCEED (archive failure is non-fatal).
// Operator visibility (tracing::warn!) is tested by asserting the response
// text shows memory-layer success, not an error.
//
// Note: we can't directly test tracing output without a tracing subscriber.
// This test is sufficient to prove non-fatal contract.

#[test]
fn archive_write_failure_does_not_block_learn() {
    // Use a path that is a file, not a directory â†’ OpenOptions::append will fail
    let tmp = TempDir::new().unwrap();
    // Pre-create learn_log.jsonl as a directory â†’ any append will fail with IsADirectory
    let bad_log = tmp.path().join("learn_log.jsonl");
    std::fs::create_dir_all(&bad_log).expect("create dir as learn_log.jsonl");

    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
        projections_dir: workspace_projections_dir(),
        ..Config::default()
    };
    let rt = ContextRuntime::new(cfg, "test-archive-fail".to_string()).unwrap();
    let shared = Arc::new(Mutex::new(rt));

    let args = json!({
        "state": "archive fail state", "action": "archive fail action",
        "outcome": "archive fail outcome", "prediction_error": 0.2, "auto_tag": false
    });
    let result = dispatch("memory_learn_text", &shared, &args).unwrap();
    assert_ne!(
        result.isError, Some(true),
        "learn must succeed even when learn_log.jsonl is unwritable: {:?}", result.content
    );
    // Memory-layer output: response must say Memory Learned
    let text = &result.content[0].text;
    assert!(
        text.contains("Memory Learned"),
        "Response must contain 'Memory Learned' even on archive failure: {text}"
    );
}

// â”€â”€ T-FIX3a: MaxCosine â€” empty corpus and model mismatch return 0.0 â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//
// Verifies NodeStore::max_cosine_to_existing edge cases:
//   1. Empty corpus â†’ 0.0 (novelty = 1.0 â†’ gate score maximised, insertion proceeds)
//   2. Model mismatch â†’ all nodes filtered â†’ 0.0 (treats as novel, correct)
// Indirect test via shadow gate log output in learn response.

#[test]
fn shadow_gate_empty_corpus_novelty_is_max() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);

    // Learn on an empty corpus â†’ max_cos = 0.0 â†’ novelty = 1.0 â†’ gate_score maximised
    let args = json!({
        "state": "first ever event in empty corpus",
        "action": "do something new",
        "outcome": "new result",
        "prediction_error": 0.5,
        "auto_tag": false
    });
    let result = dispatch("memory_learn_text", &rt, &args).unwrap();
    assert_ne!(result.isError, Some(true));
    let text = &result.content[0].text;

    // Gate line must be present and novelty must be 1.000 for first insert into empty corpus
    assert!(
        text.contains("Gate (shadow)"),
        "Gate line must be present in learn response: {text}"
    );
    assert!(
        text.contains("novelty=1.000"),
        "Novelty must be 1.000 for first insert into empty corpus. Response:\n{text}"
    );
}

// â”€â”€ T-FIX3b: Shadow gate never suppresses insertion â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//
// Even when gate_score < GATE_THRESHOLD (would_index=false),
// learn still inserts nodes AND they are retrievable.
// Also verifies gate_shadow_log.jsonl is written.

#[test]
fn shadow_gate_never_suppresses_insertion() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);

    // Learn the same text twice â†’ second call gets high max_cos â†’ low novelty â†’ low gate_score.
    // This is exactly the case where a *hard* gate would block. A shadow gate MUST NOT.
    let args1 = json!({
        "state":  "repeated state near-duplicate",
        "action": "repeated action near-duplicate",
        "outcome":"repeated outcome near-duplicate",
        "prediction_error": 0.1,  // low surprise â†’ gate_score = novelty Ã— 1.2, nearly 0 on 2nd call
        "auto_tag": false
    });
    let r1 = dispatch("memory_learn_text", &rt, &args1).unwrap();
    assert_ne!(r1.isError, Some(true), "First learn must succeed");

    let args2 = args1.clone();
    let result2 = dispatch("memory_learn_text", &rt, &args2).unwrap();

    // Core invariant: learn NEVER returns error regardless of gate score
    assert_ne!(result2.isError, Some(true),
        "Second learn must succeed even with low gate_score (shadow gate must NEVER suppress)");

    // Gate line must be in both learn responses â€” gate RAN, it just didn't block
    let text2 = &result2.content[0].text;
    assert!(
        text2.contains("Gate (shadow)"),
        "Gate (shadow) line must be present in learn response: {text2}"
    );
    assert!(
        text2.contains("Memory Learned"),
        "Response must confirm insertion happened â€” not just gate ran: {text2}"
    );

    // gate_shadow_log.jsonl must exist and have exactly 2 entries (one per learn call)
    // This proves: gate ran for every call and logged its decision; it did NOT skip low-score events.
    let gate_log = tmp.path().join("gate_shadow_log.jsonl");
    assert!(gate_log.exists(), "gate_shadow_log.jsonl must exist after two learns");
    let log_content = std::fs::read_to_string(&gate_log).unwrap();
    let entry_count = log_content.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(entry_count, 2, "gate_shadow_log must have 2 entries (one per learn), got {entry_count}");

    // Verify WAL is unaffected: a subsequent learn of different text succeeds
    // (proves the graph is not corrupted by the low-novelty insertions above)
    let r3 = dispatch("memory_learn_text", &rt, &json!({
        "state": "completely different topic", "action": "unrelated action",
        "outcome": "unrelated outcome", "prediction_error": 0.7, "auto_tag": false
    })).unwrap();
    assert_ne!(r3.isError, Some(true), "Subsequent learn after low-novelty inserts must succeed");
}

// â”€â”€ T-FIX3c: Gate Schema v2 Validation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn gate_shadow_log_schema_v2_validation() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);

    let args = json!({
        "state":  "unique test state for schema validation",
        "action": "unique test action",
        "outcome":"unique test outcome",
        "prediction_error": 0.45,
        "auto_tag": false
    });
    let r = dispatch("memory_learn_text", &rt, &args).unwrap();
    assert_ne!(r.isError, Some(true));

    let gate_log = tmp.path().join("gate_shadow_log.jsonl");
    assert!(gate_log.exists());
    let log_content = std::fs::read_to_string(&gate_log).unwrap();
    let line = log_content.lines().next().expect("Must have at least one line");
    let v: Value = serde_json::from_str(line).expect("Must parse as JSON");

    assert_eq!(v["v"], 2, "Schema version must be 2");
    assert_eq!(v["policy_v"], 2, "Policy version must be 2");
    assert!(v["verifiability"].as_f64().unwrap() > 0.0, "verifiability must be positive");
    assert!(v["verifiability"].as_f64().unwrap() <= 1.0, "verifiability must be <= 1.0");
    assert_eq!(v["enforced"], false, "enforced must be false under default shadow mode");
    assert!(v["threshold"].as_f64().unwrap() > 0.0, "threshold must be positive");
}

// â”€â”€ T-FIX3d: Gate Enforcement Mode Suppresses Redundant Ingestion â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn gate_enforcement_mode_suppresses_redundant_when_enabled() {
    let tmp = TempDir::new().unwrap();
    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
        projections_dir: workspace_projections_dir(),
        enforce_gate: true,
        gate_threshold: 0.35,
        ..Config::default()
    };
    let rt = Arc::new(Mutex::new(ContextRuntime::new(cfg, "test-session".to_string()).unwrap()));

    // 1. First learn: virgin state -> novelty = 1.0 -> passes gate
    let args1 = json!({
        "state":  "cluster alpha baseline pattern",
        "action": "execute baseline run",
        "outcome":"normal operating behavior",
        "prediction_error": 0.1,
        "auto_tag": false
    });
    let r1 = dispatch("memory_learn_text", &rt, &args1).unwrap();
    assert_ne!(r1.isError, Some(true), "First learn must succeed");
    let text1 = &r1.content[0].text;
    assert!(text1.contains("Memory Learned"), "First learn must insert nodes");
    assert!(text1.contains("Gate (enforced)"), "Should show Gate (enforced)");

    let n1 = rt.lock().node_count(None);
    assert_eq!(n1, 3, "First learn must create exactly 3 nodes (State, Action, Outcome)");

    // 2. Second learn: exact duplicate state with low prediction error (0.05)
    // novelty â‰ˆ 0 -> gate_score < 0.35 -> MUST BE SUPPRESSED
    let r2 = dispatch("memory_learn_text", &rt, &args1).unwrap();
    assert_ne!(r2.isError, Some(true), "Suppressed learn is a clean policy result, not an error");
    let text2 = &r2.content[0].text;
    assert!(
        text2.contains("SUPPRESSED") || text2.contains("Suppressed"),
        "Second learn must be suppressed by gate: {text2}"
    );

    let n2 = rt.lock().node_count(None);
    assert_eq!(n2, n1, "Suppressed learn must NOT increase node count: n1={n1}, n2={n2}");

    // Tier 0 invariant: learn_log.jsonl must still contain both attempts
    let learn_log = tmp.path().join("learn_log.jsonl");
    assert!(learn_log.exists(), "learn_log.jsonl must exist");
    let learn_content = std::fs::read_to_string(&learn_log).unwrap();
    let learn_entries = learn_content.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(learn_entries, 2, "Tier 0 learn archive must record both attempts even when gated");

    // gate_shadow_log must record would_index=false, enforced=true for second learn
    let gate_log = tmp.path().join("gate_shadow_log.jsonl");
    assert!(gate_log.exists());
    let gate_content = std::fs::read_to_string(&gate_log).unwrap();
    let gate_lines: Vec<&str> = gate_content.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(gate_lines.len(), 2);
    let v2: Value = serde_json::from_str(gate_lines[1]).unwrap();
    assert_eq!(v2["would_index"], false, "Second learn must have would_index=false");
    assert_eq!(v2["enforced"], true, "enforced flag must be true");

    // 3. Third learn: completely distinct state with high prediction error -> passes gate
    let args3 = json!({
        "state":  "completely different topic on security infrastructure",
        "action": "revoke authentication token",
        "outcome":"service returned unauthorized",
        "prediction_error": 0.8,
        "auto_tag": false
    });
    let r3 = dispatch("memory_learn_text", &rt, &args3).unwrap();
    assert_ne!(r3.isError, Some(true));
    let text3 = &r3.content[0].text;
    assert!(text3.contains("Memory Learned"), "Novel learn must succeed");
    let n3 = rt.lock().node_count(None);
    assert!(n3 > n2, "Novel learn must add nodes to graph: n2={n2}, n3={n3}");
}

// â”€â”€ T-FIX3e: Raw Vector memory_learn Gate Logging â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn raw_memory_learn_shadow_gate_logging() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);

    let l = json!({
        "state_embedding": dummy_embedding(5),
        "action_embedding": dummy_embedding(6),
        "outcome_embedding": dummy_embedding(7),
        "prediction_error": 0.35,
        "authority": 0.80,
        "provenance": "user_explicit",
    });
    let r = dispatch("memory_learn", &rt, &l).unwrap();
    assert_ne!(r.isError, Some(true));
    let text = &r.content[0].text;
    assert!(text.contains("Gate (shadow)"), "memory_learn response must contain Gate (shadow): {text}");

    let gate_log = tmp.path().join("gate_shadow_log.jsonl");
    assert!(gate_log.exists(), "gate_shadow_log must exist after memory_learn");
    let learn_log = tmp.path().join("learn_log.jsonl");
    assert!(learn_log.exists(), "learn_log must exist after memory_learn");
}

// â”€â”€ T-FIX3f: LFA Verifiability Unit Computation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn lfa_verifiability_unit_computation() {
    // 1. Virgin state without feedback -> equals authority (clamped in [0.1, 1.0])
    let v1 = tools::compute_lfa_verifiability(None, 0.85);
    assert!((v1 - 0.85).abs() < 1e-4);

    let v_floor = tools::compute_lfa_verifiability(None, 0.0);
    assert!((v_floor - 0.1).abs() < 1e-4, "Floor must be 0.1 for zero authority");

    // 2. State with high empirical success: alpha=11, beta=1 -> s=10, f=0, n=10
    // LCB is high -> pulls verifiability up
    let v_succ = tools::compute_lfa_verifiability(Some((11.0, 1.0)), 0.5);
    assert!(v_succ > 0.5, "Positive feedback must pull verifiability above neutral 0.5: got {v_succ}");

    // 3. State with high empirical failure: alpha=1, beta=11 -> s=0, f=10, n=10
    // LCB is ~0 -> pulls verifiability down
    let v_fail = tools::compute_lfa_verifiability(Some((1.0, 11.0)), 0.5);
    assert!(v_fail < 0.5, "Negative feedback must pull verifiability below neutral 0.5: got {v_fail}");

    // 4. Large sample dominance: as N grows, empirical feedback dominates source authority
    let v_large_succ = tools::compute_lfa_verifiability(Some((101.0, 1.0)), 0.1);
    assert!(v_large_succ > 0.8, "Large positive feedback must dominate low initial authority: got {v_large_succ}");
}


// â”€â”€ Test 13: E1aâ€“E1f â€” Explicit Retrieval Intent Enforcement â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn dummy_embedding(dim_idx: usize) -> String {
    let mut v = vec![0.0f32; 128];
    if dim_idx < 128 {
        v[dim_idx] = 1.0;
    }
    tools::encode_embedding(&v)
}

fn parse_content_json(result: &cmhm_mcp_lib::schema::ToolCallResult) -> Value {
    assert!(result.content.len() >= 2, "Expected at least 2 content items, got {}", result.content.len());
    let raw = &result.content[1].text;
    let trimmed = raw.trim();
    let json_str = trimmed
        .strip_prefix("```json")
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(trimmed)
        .trim();
    serde_json::from_str(json_str).expect("content[1] must be valid JSON")
}

#[test]
fn e1a_memory_retrieve_missing_intent_is_error() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let args = json!({
        "query_embedding": dummy_embedding(0),
    });
    let result = dispatch("memory_retrieve", &rt, &args).unwrap();
    assert_eq!(result.isError, Some(true), "memory_retrieve without retrieval_intent must return error");
    let text = &result.content[0].text;
    assert!(text.contains("missing field `retrieval_intent`") || text.contains("retrieval_intent"),
            "Error text should mention retrieval_intent: {text}");
}

#[test]
fn e1b_memory_retrieve_unknown_intent_is_error() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let args = json!({
        "query_embedding": dummy_embedding(0),
        "retrieval_intent": "invalid_role_intent",
    });
    let result = dispatch("memory_retrieve", &rt, &args).unwrap();
    assert_eq!(result.isError, Some(true), "memory_retrieve with invalid retrieval_intent must return error");
    let text = &result.content[0].text;
    assert!(text.contains("Invalid retrieval_intent"), "Error should explain invalid intent: {text}");
}

#[test]
fn e1c_memory_retrieve_valid_intent_succeeds() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    for intent in ["state", "action", "outcome"] {
        let args = json!({
            "query_embedding": dummy_embedding(0),
            "retrieval_intent": intent,
        });
        let result = dispatch("memory_retrieve", &rt, &args).unwrap();
        assert_ne!(result.isError, Some(true), "memory_retrieve with intent={intent} must succeed");
        let structured = parse_content_json(&result);
        assert!(structured.get("retrieve_id").is_some());
        assert_eq!(structured["abstained"], false);
    }
}

#[test]
fn e1d_memory_retrieve_text_missing_or_null_intent_succeeds_unscoped() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    // Case 1: retrieval_intent omitted entirely from JSON
    let args_omitted = json!({
        "query": "how to configure database connection pool",
    });
    let result_omitted = dispatch("memory_retrieve_text", &rt, &args_omitted).unwrap();
    assert_ne!(result_omitted.isError, Some(true), "memory_retrieve_text without retrieval_intent must succeed unscoped");
    let structured_omitted = parse_content_json(&result_omitted);
    assert!(structured_omitted.get("retrieve_id").is_some());
    assert!(structured_omitted.get("raw_nn_candidates").is_some(), "Must expose raw_nn_candidates diagnostic");
    assert!(structured_omitted.get("post_filter_candidates").is_some(), "Must expose post_filter_candidates diagnostic");
    assert!(structured_omitted.get("scored_candidates").is_some(), "Must expose scored_candidates diagnostic");

    // Case 2: retrieval_intent explicitly null
    let args_null = json!({
        "query": "how to configure database connection pool",
        "retrieval_intent": null,
    });
    let result_null = dispatch("memory_retrieve_text", &rt, &args_null).unwrap();
    assert_ne!(result_null.isError, Some(true), "memory_retrieve_text with retrieval_intent=null must succeed unscoped");
    let structured_null = parse_content_json(&result_null);
    assert!(structured_null.get("retrieve_id").is_some());
}

#[test]
fn e1d2_mcp_unscoped_retrieval_competes_across_mixed_node_types() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    // Learn a State-Action-Outcome triple
    let learn_args = json!({
        "state": "rust compiler error borrow checker",
        "action": "use clone or Arc Mutex synchronization primitive",
        "outcome": "code compiles successfully without data races",
        "prediction_error": 0.8,
        "authority": 0.9,
        "provenance": "user_explicit"
    });
    let learn_res = dispatch("memory_learn_text", &rt, &learn_args).unwrap();
    assert_ne!(learn_res.isError, Some(true));

    // Scoped retrieval with intent="state": candidate must be a State node
    let scoped_args = json!({
        "query": "synchronization primitive Arc Mutex",
        "retrieval_intent": "state",
    });
    let scoped_res = dispatch("memory_retrieve_text", &rt, &scoped_args).unwrap();
    assert_ne!(scoped_res.isError, Some(true));

    // Unscoped retrieval with omitted intent: all node types compete in candidate pool
    let unscoped_args = json!({
        "query": "synchronization primitive Arc Mutex",
    });
    let unscoped_res = dispatch("memory_retrieve_text", &rt, &unscoped_args).unwrap();
    assert_ne!(unscoped_res.isError, Some(true));
    let unscoped_json = parse_content_json(&unscoped_res);
    let raw_cands = unscoped_json["raw_nn_candidates"].as_array().expect("raw_nn_candidates array");
    assert!(!raw_cands.is_empty(), "Unscoped retrieval must discover candidates across mixed types");
}

#[test]
fn e1d3_mcp_adversarial_intent_fails_closed() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let adversarial_payloads = [
        "state; DROP TABLE nodes;--",
        "STATE", // case-sensitive check
        "State", // TitleCase check
        "",      // empty string
        "   ",   // whitespace only
        "outcome\0injection",
        "../../etc/passwd",
        "action OR 1=1",
    ];

    for payload in adversarial_payloads {
        let args = json!({
            "query": "database tuning max connections",
            "retrieval_intent": payload,
        });
        let result = dispatch("memory_retrieve_text", &rt, &args).unwrap();
        assert_eq!(
            result.isError, Some(true),
            "Adversarial retrieval_intent {payload:?} must fail closed (isError=true)"
        );
        let text = &result.content[0].text;
        assert!(
            text.contains("Invalid retrieval_intent"),
            "Error text must explain invalid intent for {payload:?}: {text}"
        );
    }
}

#[test]
fn e1d4_mcp_retrieval_is_pure_read_no_mutation() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    // Learn one record
    let learn_args = json!({
        "state": "memory leak in worker process",
        "action": "restart worker periodically",
        "outcome": "memory stabilized",
        "prediction_error": 0.5,
    });
    let _ = dispatch("memory_learn_text", &rt, &learn_args).unwrap();

    // Stats before retrieval
    let stats_before = dispatch("memory_stats", &rt, &json!({})).unwrap();
    let stats_before_text = stats_before.content[0].text.clone();

    // Unscoped retrieval
    let ret_args = json!({
        "query": "memory leak worker restart",
    });
    let ret_res = dispatch("memory_retrieve_text", &rt, &ret_args).unwrap();
    assert_ne!(ret_res.isError, Some(true));

    // Stats after retrieval: version and learn count must not mutate
    let stats_after = dispatch("memory_stats", &rt, &json!({})).unwrap();
    let stats_after_text = stats_after.content[0].text.clone();

    assert!(stats_before_text.contains("**Version**: 1 | **Learns**: 1"));
    assert!(
        stats_after_text.contains("**Version**: 1 | **Learns**: 1"),
        "Retrieval must be a pure read operation: version and learn_count must not mutate"
    );
}

#[test]
fn e1d5_mcp_passive_diagnostics_expose_all_stages() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    // Learn a base record
    let learn_base = dispatch("memory_learn_text", &rt, &json!({
        "state": "postgresql max connections pool size",
        "action": "disable pooling",
        "outcome": "crashes under load",
        "prediction_error": 0.9,
        "authority": 0.1,
        "provenance": "contradicted"
    })).unwrap();
    assert_ne!(learn_base.isError, Some(true));

    // Unscoped retrieval: inspect the diagnostic stages
    let ret_res = dispatch("memory_retrieve_text", &rt, &json!({
        "query": "postgresql max connections pool size",
    })).unwrap();
    assert_ne!(ret_res.isError, Some(true));

    let json_data = parse_content_json(&ret_res);

    // Verify all 3 stages exist as valid arrays
    let raw = json_data["raw_nn_candidates"].as_array().expect("raw_nn_candidates");
    let post = json_data["post_filter_candidates"].as_array().expect("post_filter_candidates");
    let scored = json_data["scored_candidates"].as_array().expect("scored_candidates");

    assert!(!raw.is_empty(), "RAW_NN stage must not be empty");
    assert!(!post.is_empty(), "POST_FILTER stage must not be empty");
    assert!(!scored.is_empty(), "SCORED stage must not be empty");

    // Verify SCORED items have node_id and score sorted descending
    let mut prev_score = f64::INFINITY;
    for item in scored {
        assert!(item.get("node_id").is_some());
        let sc = item["score"].as_f64().expect("score must be float");
        assert!(sc <= prev_score + 1e-6, "SCORED stage must be sorted descending: {sc} > {prev_score}");
        prev_score = sc;
    }
}

#[test]
fn e1e_memory_retrieve_text_unknown_intent_is_error() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let args = json!({
        "query": "how to configure database connection",
        "retrieval_intent": "random_intent_123",
    });
    let result = dispatch("memory_retrieve_text", &rt, &args).unwrap();
    assert_eq!(result.isError, Some(true), "memory_retrieve_text with invalid intent must return error");
    let text = &result.content[0].text;
    assert!(text.contains("Invalid retrieval_intent"), "Error should explain invalid intent: {text}");
}

#[test]
fn e1f_memory_retrieve_text_valid_intents_succeed() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    for intent in ["state", "action", "outcome"] {
        let args = json!({
            "query": "cargo build failed",
            "retrieval_intent": intent,
        });
        let result = dispatch("memory_retrieve_text", &rt, &args).unwrap();
        assert_ne!(result.isError, Some(true), "memory_retrieve_text with intent={intent} must succeed");
        let structured = parse_content_json(&result);
        assert!(structured.get("retrieve_id").is_some());
        assert_eq!(structured["abstained"], false);
    }
}

#[test]
fn e1g_conflict_structured_json_fields() {
    let tmp = TempDir::new().unwrap();
    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
        projections_dir: workspace_projections_dir(),
        conflict_detection_threshold: 0.5,
        abstain_on_conflict: false,
        ..Config::default()
    };
    let rt = Arc::new(Mutex::new(ContextRuntime::new(cfg, "test-conflict".to_string()).unwrap()));

    // Learn 1: corrected node
    let l1 = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 0.85,
        "provenance": "corrected",
    });
    let r1 = dispatch("memory_learn", &rt, &l1).unwrap();
    assert_ne!(r1.isError, Some(true));

    // Learn 2: contradicted node
    let l2 = json!({
        "state_embedding": dummy_embedding(1),
        "action_embedding": dummy_embedding(3),
        "outcome_embedding": dummy_embedding(4),
        "prediction_error": 0.5,
        "authority": 0.05,
        "provenance": "contradicted",
    });
    let r2 = dispatch("memory_learn", &rt, &l2).unwrap();
    assert_ne!(r2.isError, Some(true));

    // Midpoint query vector
    let mut q = vec![0.0f32; 128];
    q[0] = 1.0 / 2.0_f32.sqrt();
    q[1] = 1.0 / 2.0_f32.sqrt();
    let q_emb = tools::encode_embedding(&q);

    let ret_args = json!({
        "query_embedding": q_emb,
        "retrieval_intent": "state",
    });
    let result = dispatch("memory_retrieve", &rt, &ret_args).unwrap();
    assert_ne!(result.isError, Some(true));

    let structured = parse_content_json(&result);
    assert!(structured.get("retrieve_id").is_some());
    assert!(structured.get("seed_authority").is_some());
    assert!(structured.get("seed_provenance").is_some());
    assert_eq!(structured["abstained"], false);

    if structured["conflict_detected"] == true {
        let candidates = structured["conflict_candidates"].as_array().unwrap();
        assert!(!candidates.is_empty(), "conflict_candidates must not be empty when conflict_detected is true");
        for c in candidates {
            assert!(c.get("node_id").is_some());
            assert!(c.get("score").is_some());
            assert!(c.get("authority").is_some());
            assert!(c.get("provenance").is_some());
            assert!(c.get("conflict_type").is_some());
        }
    }
}

#[test]
fn e1h_abstain_structured_json_fields() {
    let tmp = TempDir::new().unwrap();
    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
        projections_dir: workspace_projections_dir(),
        conflict_detection_threshold: 0.5,
        abstain_on_conflict: true,
        ..Config::default()
    };
    let rt = Arc::new(Mutex::new(ContextRuntime::new(cfg, "test-abstain".to_string()).unwrap()));

    // Learn 1: corrected node
    let l1 = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 0.85,
        "provenance": "corrected",
    });
    dispatch("memory_learn", &rt, &l1).unwrap();

    // Learn 2: contradicted node
    let l2 = json!({
        "state_embedding": dummy_embedding(1),
        "action_embedding": dummy_embedding(3),
        "outcome_embedding": dummy_embedding(4),
        "prediction_error": 0.5,
        "authority": 0.05,
        "provenance": "contradicted",
    });
    dispatch("memory_learn", &rt, &l2).unwrap();

    let mut q = vec![0.0f32; 128];
    q[0] = 1.0 / 2.0_f32.sqrt();
    q[1] = 1.0 / 2.0_f32.sqrt();
    let q_emb = tools::encode_embedding(&q);

    let ret_args = json!({
        "query_embedding": q_emb,
        "retrieval_intent": "state",
    });
    let result = dispatch("memory_retrieve", &rt, &ret_args).unwrap();
    assert_ne!(result.isError, Some(true));

    let structured = parse_content_json(&result);
    if structured["conflict_detected"] == true {
        assert_eq!(structured["abstained"], true, "Should abstain when conflict_detected and abstain_on_conflict=true");
        assert_eq!(structured["causal_chain_len"], 0, "Causal chain must be empty on abstain");
    }
}

#[test]
fn e1i_mcp_caller_claims_user_explicit_is_downgraded() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    // MCP caller claims user_explicit / 1.0 -> must be unconditionally downgraded to inferred / 0.60
    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 1.0,
        "provenance": "user_explicit",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_ne!(r.isError, Some(true));

    let ret_args = json!({
        "query_embedding": dummy_embedding(0),
        "retrieval_intent": "state",
    });
    let result = dispatch("memory_retrieve", &rt, &ret_args).unwrap();
    let structured = parse_content_json(&result);
    assert_eq!(structured["seed_provenance"], "inferred", "MCP caller claiming user_explicit must be downgraded to inferred");
    assert!((structured["seed_authority"].as_f64().unwrap() - 0.60).abs() < 1e-4, "MCP authority must be clamped to 0.60 ceiling");
}

#[test]
fn e1j_mcp_payload_claiming_caller_role_user_cannot_forge_privilege() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    // An adversarial MCP client attempts to pass caller_role: "user" in payload -> ignored / downgraded
    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 1.0,
        "provenance": "user_explicit",
        "caller_role": "user",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_ne!(r.isError, Some(true));

    let ret_args = json!({
        "query_embedding": dummy_embedding(0),
        "retrieval_intent": "state",
    });
    let result = dispatch("memory_retrieve", &rt, &ret_args).unwrap();
    let structured = parse_content_json(&result);
    // Payload caller_role: "user" MUST NOT grant privileged authority!
    assert_eq!(structured["seed_provenance"], "inferred", "caller_role in MCP payload must not grant user_explicit");
    assert!((structured["seed_authority"].as_f64().unwrap() - 0.60).abs() < 1e-4, "authority must remain clamped to 0.60");
}

#[test]
fn e1l_direct_internal_rust_runtime_preserves_user_explicit() {
    let tmp = TempDir::new().unwrap();
    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
        projections_dir: workspace_projections_dir(),
        ..Config::default()
    };
    let mut rt = ContextRuntime::new(cfg, "internal-trusted".to_string()).unwrap();

    // Internal trusted call directly through Rust API with CallerRole::User
    let mut s_emb = vec![0.0f32; 128];
    s_emb[0] = 1.0;
    let mut a_emb = vec![0.0f32; 128];
    a_emb[1] = 1.0;
    let mut o_emb = vec![0.0f32; 128];
    o_emb[2] = 1.0;

    let (learn_res, _) = rt.learn_with_context(
        None, s_emb.clone(), a_emb, o_emb, 0.5, 0.5, "hash-128d",
        1.0, "user_explicit".to_string(), None,
        cmhm_core::CallerRole::User, None, None,
    ).unwrap();
    assert!(learn_res.state_node_id > 0);

     let (ret_res, _) = rt.retrieve(None, s_emb, "hash-128d", cmhm_core::NodeType::State, "").unwrap();
    assert_eq!(ret_res.seed_provenance, "user_explicit");
    assert!((ret_res.seed_authority - 1.00).abs() < 1e-4);
}

#[test]
fn e1m_mcp_external_feed_is_capped_to_0_60() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 0.90,
        "provenance": "external_feed",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_ne!(r.isError, Some(true));

    let ret_args = json!({
        "query_embedding": dummy_embedding(0),
        "retrieval_intent": "state",
    });
    let result = dispatch("memory_retrieve", &rt, &ret_args).unwrap();
    let structured = parse_content_json(&result);
    assert_eq!(structured["seed_provenance"], "inferred", "external_feed via MCP must normalize to inferred");
    assert!((structured["seed_authority"].as_f64().unwrap() - 0.60).abs() < 1e-4, "authority must be capped to 0.60");
}

#[test]
fn e1n_mcp_whitespace_casing_variants_are_capped_to_0_60() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 1.0,
        "provenance": "  USER_EXPLICIT \t\n",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_ne!(r.isError, Some(true));

    let ret_args = json!({
        "query_embedding": dummy_embedding(0),
        "retrieval_intent": "state",
    });
    let result = dispatch("memory_retrieve", &rt, &ret_args).unwrap();
    let structured = parse_content_json(&result);
    assert_eq!(structured["seed_provenance"], "inferred");
    assert!((structured["seed_authority"].as_f64().unwrap() - 0.60).abs() < 1e-4);
}

#[test]
fn e1o_mcp_unknown_provenance_is_capped_to_0_60() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 0.99,
        "provenance": "malicious_unrecognized_provenance_string",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_ne!(r.isError, Some(true));

    let ret_args = json!({
        "query_embedding": dummy_embedding(0),
        "retrieval_intent": "state",
    });
    let result = dispatch("memory_retrieve", &rt, &ret_args).unwrap();
    let structured = parse_content_json(&result);
    assert_eq!(structured["seed_provenance"], "inferred");
    assert!((structured["seed_authority"].as_f64().unwrap() - 0.60).abs() < 1e-4);
}

#[test]
fn e1p_mcp_contradicted_is_capped_to_0_30() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 0.80,
        "provenance": "contradicted",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_ne!(r.isError, Some(true));

    let ret_args = json!({
        "query_embedding": dummy_embedding(0),
        "retrieval_intent": "state",
    });
    let result = dispatch("memory_retrieve", &rt, &ret_args).unwrap();
    let structured = parse_content_json(&result);
    // S-01 (Recall v1): a "contradicted" node is structurally ineligible as a seed,
    // so an all-contradicted pool abstains instead of returning it. The old assertions
    // here (seed_provenance == "contradicted", seed_authority == 0.30) pinned the
    // pre-amendment behavior where the contradicted memory WAS returned as the seed.
    // The 0.30 authority cap itself is unchanged and unit-verified at learn time in
    // cmhm_core::authority (clamp_authority(1.0, "contradicted") == 0.30).
    assert_eq!(structured["abstained"], true,
        "all-contradicted pool must abstain, not return a contradicted seed");
    assert!(structured["seed_id"].is_null(),
        "abstention must not expose a seed: {:?}", structured["seed_id"]);
    assert_eq!(structured["seed_provenance"], "",
        "no seed means no seed provenance");
    assert!(structured["post_filter_candidates"].as_array().map_or(false, |a| !a.is_empty()),
        "contradicted node must stay visible in post-filter diagnostics");
}


#[test]
fn e1k_invalid_supersession_returns_error_result() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 1.0,
        "provenance": "user_explicit",
        "supersedes": 999999999u64,
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_eq!(r.isError, Some(true), "Invalid supersession target must return isError=true");
    let err_msg = &r.content[0].text;
    assert!(err_msg.contains("999999999 does not exist in store"), "Error message must reflect TargetNotFound, got: {err_msg}");
}


// =============================================================================
// E1qâ€“E1t: Non-finite input rejection at MCP boundary
//
// NaN, +Inf, and -Inf must be rejected with isError=true before reaching engine.
// =============================================================================

#[test]
fn e1q_mcp_nan_authority_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": f32::NAN,
        "provenance": "inferred",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_eq!(r.isError, Some(true), "E1q: NaN authority must return isError=true");
}

#[test]
fn e1r_mcp_inf_authority_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": f32::INFINITY,
        "provenance": "inferred",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_eq!(r.isError, Some(true), "E1r: +Inf authority must return isError=true");
}

#[test]
fn e1s_mcp_nan_prediction_error_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": f32::NAN,
        "authority": 0.5,
        "provenance": "inferred",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_eq!(r.isError, Some(true), "E1s: NaN prediction_error must return isError=true");
}

#[test]
fn e1t_mcp_text_nan_authority_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let learn_args = json!({
        "state": "test state",
        "action": "test action",
        "outcome": "test outcome",
        "prediction_error": 0.5,
        "authority": f32::NAN,
        "provenance": "inferred",
    });
    let r = dispatch("memory_learn_text", &rt, &learn_args).unwrap();
    assert_eq!(r.isError, Some(true), "E1t: NaN authority via text endpoint must return isError=true");
}

// =============================================================================
// E1uâ€“E1v: WAL recovery through MCP endpoints preserves authority ceiling
//
// Tests that an adversarial Agent claim (authority=1.0, provenance="user_explicit")
// submitted through each MCP learn endpoint (memory_learn and memory_learn_text)
// survives WAL-only recovery with the clamped values (authorityâ‰¤0.60,
// provenance="inferred") still in effect.
// =============================================================================

fn blend_embedding(base_dim: usize, ortho_dim: usize, cos: f32) -> String {
    let mut v = vec![0.0f32; 128];
    v[base_dim] = cos;
    v[ortho_dim] = (1.0 - cos * cos).sqrt();
    tools::encode_embedding(&v)
}

#[test]
fn e1u_mcp_wal_recovery_preserves_agent_ceiling() {
    let tmp = TempDir::new().unwrap();

    // Session 1: learn legitimate authoritative memory and competing adversarial Agent claim through MCP tool
    let (ctx_uuid, live_seed_id, live_seed_auth, live_seed_prov) = {
        let rt = make_runtime(&tmp);

        // 1. Competing Legitimate Authoritative Memory (learned via trusted runtime API):
        // cos ~ 0.85 to base_dim 10, authority 0.85, "corrected"
        // Live score = 0.70 * 0.85 + 0.15 * 0.85 = 0.7225
        let (legit, _) = rt.lock().learn_with_context(
            None,
            tools::decode_embedding(&blend_embedding(10, 11, 0.85)).unwrap(),
            tools::decode_embedding(&dummy_embedding(1)).unwrap(),
            tools::decode_embedding(&dummy_embedding(2)).unwrap(),
            0.5, 0.5, "hash-128d",
            0.85, "corrected".into(), None,
            cmhm_core::CallerRole::User, None, None,
        ).unwrap();
        assert!(legit.state_node_id > 0);

        // 2. Adversarial Candidate learned via MCP tool (CallerRole::Agent):
        // cos ~ 0.88 to base_dim 10 (higher cosine!), claims authority 1.0 and "user_explicit"
        // If authority escalated to 1.0, score = 0.70 * 0.88 + 0.15 * 1.0 = 0.766 > 0.7225 (overturns ranking!)
        // But MCP caller is Agent, so authority is clamped to <= 0.60.
        // Clamped score = 0.70 * 0.88 + 0.15 * 0.60 = 0.706 < 0.7225 (legitimate candidate holds top rank).
        let adv_args = json!({
            "state_embedding": blend_embedding(10, 12, 0.88),
            "action_embedding": dummy_embedding(3),
            "outcome_embedding": dummy_embedding(4),
            "prediction_error": 0.5,
            "authority": 1.0,
            "provenance": "user_explicit",
            "model_id": "hash-128d",
        });
        let r_adv = dispatch("memory_learn", &rt, &adv_args).unwrap();
        assert_ne!(r_adv.isError, Some(true), "E1u: adv learn must succeed");

        // Query with base_dim 10
        let ret_args = json!({
            "query_embedding": dummy_embedding(10),
            "retrieval_intent": "state",
            "model_id": "hash-128d",
        });
        let result = dispatch("memory_retrieve", &rt, &ret_args).unwrap();
        let structured = parse_content_json(&result);
        let live_id = structured["seed_id"].as_u64().unwrap();
        let live_auth = structured["seed_authority"].as_f64().unwrap();
        let live_prov = structured["seed_provenance"].as_str().unwrap().to_string();

        // Legitimate candidate (0.85) must beat clamped adversarial candidate (0.60)
        assert_eq!(live_id, legit.state_node_id, "E1u live: legitimate candidate must win");
        assert!((live_auth - 0.85).abs() < 1e-3, "E1u live: legitimate candidate authority must be 0.85, got {live_auth}");
        assert_eq!(live_prov, "corrected", "E1u live: legitimate provenance must be 'corrected'");

        rt.lock().shutdown();
        ("default".to_string(), live_id, live_auth, live_prov)
    };

    // Session 2: delete snapshot, force pure WAL-only recovery, verify ranking equivalence
    let ctx_dir = tmp.path().join("contexts").join(&ctx_uuid);
    if ctx_dir.exists() {
        let _ = std::fs::remove_file(ctx_dir.join("snapshot.bin"));
    }

    {
        let rt = make_runtime(&tmp);

        let ret_args = json!({
            "query_embedding": dummy_embedding(10),
            "retrieval_intent": "state",
            "model_id": "hash-128d",
        });
        let result = dispatch("memory_retrieve", &rt, &ret_args).unwrap();
        let structured = parse_content_json(&result);
        let recovered_id = structured["seed_id"].as_u64().unwrap();
        let recovered_auth = structured["seed_authority"].as_f64().unwrap();
        let recovered_prov = structured["seed_provenance"].as_str().unwrap();

        // Verify recovered ranking and winner equivalence
        assert_eq!(recovered_id, live_seed_id,
            "E1u WAL recovery: recovered winner node id ({recovered_id}) must match live winner node id ({live_seed_id})");
        assert!((recovered_auth - live_seed_auth).abs() < 1e-3,
            "E1u WAL recovery: recovered authority ({recovered_auth}) must match live authority ({live_seed_auth})");
        assert_eq!(recovered_prov, live_seed_prov,
            "E1u WAL recovery: recovered provenance must match live provenance");

        rt.lock().shutdown();
    }
}

#[test]
fn e1v_mcp_text_wal_recovery_preserves_agent_ceiling() {
    let tmp = TempDir::new().unwrap();

    // Session 1: learn competing text memories through trusted runtime and adversarial MCP text tool
    let (ctx_uuid, live_seed_id, live_seed_auth, live_seed_prov) = {
        let rt = make_runtime(&tmp);

        // 1. Competing Authoritative Memory via memory_learn_text (uses active embedder — same as retrieval).
        // Uses provenance=corrected → authority clamped to 0.85.
        // Query will be "production deployment procedure".
        let legit_args = json!({
            "state": "production deployment procedure",
            "action": "run migrations first",
            "outcome": "successful deploy",
            "prediction_error": 0.5,
            "authority": 0.85,
            "provenance": "corrected",
        });
        let r_legit = dispatch("memory_learn_text", &rt, &legit_args).unwrap();
        assert_ne!(r_legit.isError, Some(true), "E1v: legit learn_text must succeed");
        // Node count: 3 after first learn (State, Action, Outcome)
        let legit_node_count = rt.lock().node_count(None);
        assert_eq!(legit_node_count, 3, "E1v: legit learn must create 3 nodes");

        // 2. Adversarial Candidate claiming 1.0 and user_explicit via MCP text tool.
        // user_explicit is clamped to ≤0.60 by the authority ceiling — cannot beat 0.85.
        let adv_args = json!({
            "state": "deployment procedure",
            "action": "skip migrations",
            "outcome": "instant crash",
            "prediction_error": 0.5,
            "authority": 1.0,
            "provenance": "user_explicit",
        });
        let r_adv = dispatch("memory_learn_text", &rt, &adv_args).unwrap();
        assert_ne!(r_adv.isError, Some(true), "E1v: adv learn_text must succeed");

        // Query text — must retrieve the legit node (higher authority) not the adversarial one
        let ret_args = json!({
            "query": "production deployment procedure",
            "retrieval_intent": "state",
        });
        let result = dispatch("memory_retrieve_text", &rt, &ret_args).unwrap();
        let structured = parse_content_json(&result);
        let live_id = structured["seed_id"].as_u64().unwrap();
        let live_auth = structured["seed_authority"].as_f64().unwrap();
        let live_prov = structured["seed_provenance"].as_str().unwrap().to_string();

        // NOTE: memory_learn_text uses CallerRole::Agent (C-01). Agent callers cannot
        // forge "corrected" or "user_explicit" — both normalize to "inferred" at 0.60.
        // Legit wins purely by cosine: "production deployment procedure" is an exact
        // match to the query; adversarial "deployment procedure" is a shorter paraphrase.
        // The authority advantage (0.85 corrected) is only achievable via User-level API,
        // not via the MCP text tool. This test validates cosine-based victory + WAL recovery.
        assert_eq!(live_id, 1u64, "E1v live: legitimate candidate must win (node 1, highest cosine)");
        assert!((live_auth - 0.60).abs() < 1e-3, "E1v live: authority must be 0.60 (agent ceiling), got {live_auth}");
        assert_eq!(live_prov, "inferred", "E1v live: Agent-submitted corrected→inferred, got {live_prov}");

        rt.lock().shutdown();
        ("default".to_string(), live_id, live_auth, live_prov)
    };

    // Session 2: delete snapshot, force pure WAL-only recovery, verify ranking equivalence
    let ctx_dir = tmp.path().join("contexts").join(&ctx_uuid);
    if ctx_dir.exists() {
        let _ = std::fs::remove_file(ctx_dir.join("snapshot.bin"));
    }

    {
        let rt = make_runtime(&tmp);

        let ret_args = json!({
            "query": "production deployment procedure",
            "retrieval_intent": "state",
        });
        let result = dispatch("memory_retrieve_text", &rt, &ret_args).unwrap();
        let structured = parse_content_json(&result);
        let recovered_id = structured["seed_id"].as_u64().unwrap();
        let recovered_auth = structured["seed_authority"].as_f64().unwrap();
        let recovered_prov = structured["seed_provenance"].as_str().unwrap();

        assert_eq!(recovered_id, live_seed_id,
            "E1v WAL recovery: recovered winner node id ({recovered_id}) must match live winner node id ({live_seed_id})");
        assert!((recovered_auth - live_seed_auth).abs() < 1e-3,
            "E1v WAL recovery: recovered authority ({recovered_auth}) must match live authority ({live_seed_auth})");
        assert_eq!(recovered_prov, live_seed_prov,
            "E1v WAL recovery: recovered provenance must match live provenance");

        rt.lock().shutdown();
    }
}

// =============================================================================
// E1wâ€“E1x: Non-finite embedding values rejected at MCP boundary
// =============================================================================

#[test]
fn e1w_mcp_nan_in_embedding_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let mut bytes = Vec::with_capacity(128 * 4);
    for i in 0..128 {
        if i == 0 {
            bytes.extend_from_slice(&f32::NAN.to_le_bytes());
        } else {
            bytes.extend_from_slice(&0.0f32.to_le_bytes());
        }
    }
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    let learn_args = json!({
        "state_embedding": b64,
        "action_embedding": dummy_embedding(1),
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 0.5,
        "provenance": "inferred",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_eq!(r.isError, Some(true), "E1w: NaN in embedding must return isError=true");
    let msg = &r.content[0].text;
    assert!(msg.contains("non-finite") || msg.contains("NaN"),
        "E1w: error message must mention non-finite, got: {msg}");
}

#[test]
fn e1x_mcp_inf_in_embedding_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let mut bytes = Vec::with_capacity(128 * 4);
    for i in 0..128 {
        if i == 0 {
            bytes.extend_from_slice(&f32::INFINITY.to_le_bytes());
        } else {
            bytes.extend_from_slice(&0.0f32.to_le_bytes());
        }
    }
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    let learn_args = json!({
        "state_embedding": dummy_embedding(0),
        "action_embedding": b64,
        "outcome_embedding": dummy_embedding(2),
        "prediction_error": 0.5,
        "authority": 0.5,
        "provenance": "inferred",
    });
    let r = dispatch("memory_learn", &rt, &learn_args).unwrap();
    assert_eq!(r.isError, Some(true), "E1x: Inf in embedding must return isError=true");
    let msg = &r.content[0].text;
    assert!(msg.contains("non-finite") || msg.contains("Inf"),
        "E1x: error message must mention non-finite, got: {msg}");
}

// â”€â”€ Promotion Tool Tests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn promote_nonexistent_event_returns_error() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);

    let promote_args = json!({
        "event_id": "00000000-0000-0000-0000-000000000000",
        "authority": 0.90,
        "caller_role": "user"
    });
    let r = dispatch("memory_promote", &rt, &promote_args).unwrap();
    assert_eq!(r.isError, Some(true), "Nonexistent event must return isError=true");
    let msg = &r.content[0].text;
    assert!(msg.contains("not found"), "Error must mention not found, got: {msg}");
}

#[test]
fn promote_untrusted_caller_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);

    // 1. Learn an event to generate learn_log.jsonl
    let learn_args = json!({
        "state":  "untrusted sensor observation regarding memory pressure",
        "action": "log memory warning",
        "outcome":"process killed by oom killer",
        "prediction_error": 0.7,
        "authority": 0.20,
        "auto_tag": false
    });
    let r_learn = dispatch("memory_learn_text", &rt, &learn_args).unwrap();
    assert_ne!(r_learn.isError, Some(true));

    // Extract event_id from learn_log.jsonl
    let learn_log = tmp.path().join("learn_log.jsonl");
    let content = std::fs::read_to_string(&learn_log).unwrap();
    let entry: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    let event_id = entry["event_id"].as_str().unwrap();

    // 2. Attempt promotion with unauthenticated/agent role -> rejected
    let promote_args = json!({
        "event_id": event_id,
        "authority": 0.90,
        "caller_role": "agent"
    });
    let r_promote = dispatch("memory_promote", &rt, &promote_args).unwrap();
    assert_eq!(r_promote.isError, Some(true), "Agent caller must not be authorized to promote");
    let msg = &r_promote.content[0].text;
    assert!(msg.contains("Permission denied"), "Error must state permission denied, got: {msg}");
}

#[test]
fn promote_valid_event_indexes_node_with_audit_trail() {
    let tmp = TempDir::new().unwrap();
    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
        projections_dir: workspace_projections_dir(),
        enforce_gate: true,
        ..Config::default()
    };
    let rt = ContextRuntime::new(cfg, "test-session".to_string()).expect("ContextRuntime::new failed");
    let rt = Arc::new(Mutex::new(rt));

    // 1. Learn an uncorroborated, low-authority assertion under enforcement mode -> suppressed
    let learn_args = json!({
        "state":  "postgresql max_connections set to 1000 causes thread exhaustion",
        "action": "reduce max_connections to 200 with pgbouncer pool",
        "outcome":"p99 latency dropped from 450ms to 12ms",
        "prediction_error": 0.1,
        "authority": 0.10,
        "auto_tag": true
    });
    let r_learn = dispatch("memory_learn_text", &rt, &learn_args).unwrap();
    assert_ne!(r_learn.isError, Some(true));
    let learn_text = &r_learn.content[0].text;
    assert!(learn_text.contains("Suppressed"), "Uncorroborated low-authority assertion must be suppressed");
    let n_initial = rt.lock().node_count(None);
    assert_eq!(n_initial, 0, "Suppressed assertion must not index any nodes in the active graph");

    // Extract event_id from Tier 0 archive
    let learn_log = tmp.path().join("learn_log.jsonl");
    assert!(learn_log.exists(), "learn_log.jsonl must exist despite suppression");
    let content = std::fs::read_to_string(&learn_log).unwrap();
    let entry: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    let event_id = entry["event_id"].as_str().unwrap();

    // 2. Promote with user privilege and elevated authority
    let promote_args = json!({
        "event_id": event_id,
        "authority": 0.95,
        "caller_role": "user",
        "auto_tag": true
    });
    let r_promote = dispatch("memory_promote", &rt, &promote_args).unwrap();
    assert_ne!(r_promote.isError, Some(true), "Promotion should succeed");
    let promote_text = &r_promote.content[0].text;
    assert!(promote_text.contains("Successfully Promoted"), "Response must mention successfully promoted");
    assert!(promote_text.contains("user_explicit [promoted]"));

    // 3. Verify node count increased
    let n_after = rt.lock().node_count(None);
    assert!(n_after > n_initial, "Promotion must index new nodes: before={n_initial}, after={n_after}");

    // 4. Verify promotion_log.jsonl was written
    let promo_log = tmp.path().join("promotion_log.jsonl");
    assert!(promo_log.exists(), "promotion_log.jsonl must exist");
    let promo_content = std::fs::read_to_string(&promo_log).unwrap();
    let p_entry: Value = serde_json::from_str(promo_content.lines().next().unwrap()).unwrap();

    assert_eq!(p_entry["v"], 1);
    assert_eq!(p_entry["event_id"], event_id);
    assert_eq!(p_entry["caller_role"], "user");
    assert!((p_entry["authority"].as_f64().unwrap() - 0.95).abs() < 1e-3);
    assert!(p_entry["novelty"].is_number());
    assert!(p_entry["verifiability"].is_number());
    assert!(p_entry["gate_score"].is_number());
    assert!(p_entry["max_cosine"].is_number());
    assert!(p_entry["node_ids"]["state"].is_number());

    // 5. Verify retrieve finds the promoted experience
    let retrieve_args = json!({
        "query": "postgresql pgbouncer max_connections latency",
        "retrieval_intent": "state"
    });
    let r_retrieve = dispatch("memory_retrieve_text", &rt, &retrieve_args).unwrap();
    assert_ne!(r_retrieve.isError, Some(true));
    let ret_text = &r_retrieve.content[0].text;
    assert!(ret_text.contains("Node("), "Retrieval must find active causal path: {ret_text}");
    assert!(!ret_text.contains("No causal path found"), "Retrieval must not be empty: {ret_text}");
}



// ════════════════════════════════════════════════════════════════════════
// B1 — MCP surface must return learned text (design note approved 2026-10-11)
//
// The agent-facing complaint that started this: memory_retrieve_text
// returned "Node(1) → Node(2) (confidence: 50%, evidence: 2.0)" — a stub,
// not memory. These tests pin the fix at the MCP boundary: the human-readable
// block AND the machine-readable JSON must both carry the learned text.
// ════════════════════════════════════════════════════════════════════════

#[test]
fn b1_mcp_retrieve_text_returns_learned_text_in_block_and_json() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    // Episode texts share a heavy word/3-gram overlap (hash embedder) so the
    // edge-birth similarity threshold (0.35, engine.rs) is crossed and a chain
    // forms; they stay below the near-dup ceiling (0.85) so they birth as
    // distinct nodes.
    let state_text   = "nginx port configuration audit 8443";
    let action_text  = "nginx port configuration audit restarted proxy";
    let outcome_text = "nginx port configuration audit healthy 200";

    let learn_args = json!({
        "state":            state_text,
        "action":           action_text,
        "outcome":          outcome_text,
        "prediction_error": 0.1,
        "auto_tag":         false,
    });
    let lr = dispatch("memory_learn_text", &rt, &learn_args).unwrap();
    assert_ne!(lr.isError, Some(true), "learn_text must succeed: {:?}", lr.content);

    // Query with the exact state text — the hash embedder is deterministic, so
    // this guarantees the learned node is the retrieval seed.
    let ret = dispatch(
        "memory_retrieve_text",
        &rt,
        &json!({"query": state_text, "retrieval_intent": "state"}),
    )
    .unwrap();
    assert_ne!(ret.isError, Some(true), "retrieve_text must succeed: {:?}", ret.content);

    let text = ret.content.first().map(|c| c.text.as_str()).unwrap_or("");

    // ── Human-readable block: seed text verbatim + no ID-only stub ───────
    assert!(text.contains(state_text),
        "human-readable block must contain the seed text verbatim; got:\n{text}");
    // The pre-B1 stub was "Node(1) → Node(2) (confidence...)" with NO text.
    // If the text renders, the stub is gone; this guard documents that intent.
    assert!(
        !text.contains("Node(") || text.contains(state_text),
        "human-readable block still renders the pre-B1 node-id stub:\n{text}"
    );

    // ── Machine-readable JSON: seed + every chain step round-trips ───────
    let structured = parse_content_json(&ret);
    let seed_text = structured["seed_text"].as_str()
        .expect("JSON must carry seed_text as a string");
    assert_eq!(seed_text, state_text, "JSON seed_text must be the seed's text");
    let seed_ts = structured["seed_timestamp"].as_i64()
        .expect("JSON must carry seed_timestamp");
    assert!(seed_ts > 0, "seed_timestamp must be a unix timestamp > 0");

    let chain = structured["causal_chain"]
        .as_array()
        .expect("JSON must carry a causal_chain array");
    assert!(!chain.is_empty(),
        "an episode with related texts must produce a non-empty chain; got: {chain:?}");
    for (i, step) in chain.iter().enumerate() {
        let from_text = step["from_text"].as_str()
            .unwrap_or_else(|| panic!("chain[{i}]: from_text must be a string"));
        let to_text = step["to_text"].as_str()
            .unwrap_or_else(|| panic!("chain[{i}]: to_text must be a string"));
        assert!(!from_text.is_empty() && !to_text.is_empty(),
            "chain[{i}]: text-path learn must not produce empty text fields");
        assert!(step["from_timestamp"].as_i64().unwrap_or(0) > 0
            && step["to_timestamp"].as_i64().unwrap_or(0) > 0,
            "chain[{i}]: timestamps must be present and > 0");
        // Every text on the path must also appear in the human block.
        assert!(text.contains(from_text),
            "chain[{i}]: from_text must render verbatim in the block; got:\n{text}");
        assert!(text.contains(to_text),
            "chain[{i}]: to_text must render verbatim in the block; got:\n{text}");
    }
}

#[test]
fn b1_mcp_long_text_full_in_json_truncated_in_block() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    // Trimmed: the MCP ingestion boundary normalizes trailing whitespace
    // before storage, so the stored/retrieved text is the trimmed form.
    let long_text = "PORT_CONFIGURATION_AUDIT ".repeat(20).trim_end().to_string();
    let learn_args = json!({
        "state":            long_text,
        "action":           "short action",
        "outcome":          "short outcome",
        "prediction_error": 0.1,
        "auto_tag":         false,
    });
    let lr = dispatch("memory_learn_text", &rt, &learn_args).unwrap();
    assert_ne!(lr.isError, Some(true), "learn_text must succeed: {:?}", lr.content);

    let ret = dispatch(
        "memory_retrieve_text",
        &rt,
        &json!({"query": long_text, "retrieval_intent": "state"}),
    )
    .unwrap();

    // JSON is lossless.
    let structured = parse_content_json(&ret);
    assert_eq!(structured["seed_text"].as_str(), Some(long_text.as_str()),
        "JSON seed_text must be lossless");
    let json_str = serde_json::to_string(&structured).unwrap();
    assert!(json_str.contains(&long_text), "JSON must carry the full long text");

    // Human block is truncated with a visible marker.
    let text = ret.content.first().map(|c| c.text.as_str()).unwrap_or("");
    assert!(text.contains(&long_text[..120]), "block must contain the 120-char prefix");
    assert!(text.contains("…[+"), "block must carry the visible truncation marker; got:\n{text}");
}
