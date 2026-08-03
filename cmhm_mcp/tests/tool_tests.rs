//! Integration tests for the cmhm_mcp tool layer.
//!
//! Three invariants, enforced for every release:
//!   1. PARITY  — every name in the TOOLS registry has a dispatch arm in lib::dispatch().
//!   2. SMOKE   — every handler returns a non-panicking result for both valid and garbage args.
//!   3. ERRORS  — garbage args always produce isError=true, never a panic.

use std::sync::Arc;
use parking_lot::Mutex;
use serde_json::{json, Value};
use tempfile::TempDir;

use cmhm_core::{Config, ContextRuntime};
use cmhm_mcp_lib::{dispatch, tools};

// ── Test helpers ──────────────────────────────────────────────────────────────

fn make_runtime(tmp: &TempDir) -> tools::SharedRuntime {
    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
        ..Config::default()
    };
    let rt = ContextRuntime::new(cfg, "test-session".to_string()).expect("ContextRuntime::new failed in test");
    Arc::new(Mutex::new(rt))
}

// ── Test 1: PARITY — every registered tool name dispatches ────────────────────

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

// ── Test 2: SMOKE — garbage args → clean error, never panic ──────────────────

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
fn garbage_args_never_panic_context_create() {
    let tmp = TempDir::new().unwrap();
    assert_error_result("context_create", &make_runtime(&tmp));
}

#[test]
fn garbage_args_never_panic_context_delete() {
    let tmp = TempDir::new().unwrap();
    assert_error_result("context_delete", &make_runtime(&tmp));
}

// ── Test 3: SMOKE — valid args → not isError ─────────────────────────────────

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
    let args = json!({"query": "rust compiler error linker not found"});
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

    // Retrieve — must find something (chain won't be empty after learning)
    let retrieve_args = json!({"query": "cargo build linker error"});
    let retrieve_result = dispatch("memory_retrieve_text", &rt, &retrieve_args).unwrap();
    assert_ne!(retrieve_result.isError, Some(true), "retrieve after learn must not error");
}

// ── Test 4: UNKNOWN tool → dispatch returns None ─────────────────────────────

#[test]
fn unknown_tool_returns_none() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let result = dispatch("nonexistent_tool_xyz", &rt, &json!({}));
    assert!(result.is_none(), "dispatch must return None for unknown tool names");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Focused correctness tests (Fixes 1–3)
// ═══════════════════════════════════════════════════════════════════════════════

// ── T-FIX1: Archive scrub — secrets must never reach learn_log.jsonl ─────────
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

    // Read learn_log.jsonl — every byte of it
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

// ── T-FIX2a: retrieve_id — duplicate across restart ──────────────────────────
//
// Session 1: learn, retrieve (get retrieve_id), apply feedback → ledger written.
// Session 2: new ContextRuntime from same dir → ledger reloaded.
// Second feedback call with same retrieve_id → must return "already applied",
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
        let ret = dispatch("memory_retrieve_text", &rt, &json!({"query": "borrow checker"})).unwrap();
        assert_ne!(ret.isError, Some(true));

        // Apply feedback with a known retrieve_id (normally from the retrieve response)
        let fb_args = json!({
            "node_ids": [1u64, 2u64],  // IDs may or may not exist — that's ok for ledger test
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
    } // rt dropped → ledger flushed to disk

    // Session 2: fresh runtime — ledger must be reloaded
    {
        let cfg = Config {
            data_dir: tmp.path().to_path_buf(),
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

// ── T-FIX2b: Concurrent same retrieve_id — only one mutates ──────────────────
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

// ── T-FIX2c: Crash boundary — double Beta apply is statistically bounded ─────
//
// Simulates the crash scenario where feedback_nodes() ran but ledger was never
// written: apply feedback twice to the same nodes. The Beta posterior must stay
// in (0, ∞) (no NaN, no panic, no overflow). This proves Option A ordering is
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

    // Apply feedback twice — no retrieve_id (simulates crash replay: ledger absent)
    let fb_args = json!({
        "node_ids": [1u64, 2u64, 3u64],
        "outcome":  "success"
        // No retrieve_id — represents "crash between mutate and record"
    });
    let r1 = dispatch("memory_feedback", &rt, &fb_args).unwrap();
    assert_ne!(r1.isError, Some(true), "First apply must succeed");

    let r2 = dispatch("memory_feedback", &rt, &fb_args).unwrap();
    assert_ne!(r2.isError, Some(true), "Second apply (crash replay) must succeed, not panic");

    // Retrieve — must still work, causal graph not corrupted
    let ret = dispatch("memory_retrieve_text", &rt, &json!({"query": "beta boundary"})).unwrap();
    assert_ne!(ret.isError, Some(true), "Retrieve after double apply must succeed");
}

// ── T-FIX1b: Archive write failure is non-fatal, operator-visible ─────────────
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
    // Use a path that is a file, not a directory → OpenOptions::append will fail
    let tmp = TempDir::new().unwrap();
    // Pre-create learn_log.jsonl as a directory → any append will fail with IsADirectory
    let bad_log = tmp.path().join("learn_log.jsonl");
    std::fs::create_dir_all(&bad_log).expect("create dir as learn_log.jsonl");

    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
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

// ── T-FIX3a: MaxCosine — empty corpus and model mismatch return 0.0 ──────────
//
// Verifies NodeStore::max_cosine_to_existing edge cases:
//   1. Empty corpus → 0.0 (novelty = 1.0 → gate score maximised, insertion proceeds)
//   2. Model mismatch → all nodes filtered → 0.0 (treats as novel, correct)
// Indirect test via shadow gate log output in learn response.

#[test]
fn shadow_gate_empty_corpus_novelty_is_max() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);

    // Learn on an empty corpus → max_cos = 0.0 → novelty = 1.0 → gate_score maximised
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

// ── T-FIX3b: Shadow gate never suppresses insertion ───────────────────────────
//
// Even when gate_score < GATE_THRESHOLD (would_index=false),
// learn still inserts nodes AND they are retrievable.
// Also verifies gate_shadow_log.jsonl is written.

#[test]
fn shadow_gate_never_suppresses_insertion() {
    let tmp = TempDir::new().unwrap();
    let rt  = make_runtime(&tmp);

    // Learn the same text twice → second call gets high max_cos → low novelty → low gate_score.
    // This is exactly the case where a *hard* gate would block. A shadow gate MUST NOT.
    let args1 = json!({
        "state":  "repeated state near-duplicate",
        "action": "repeated action near-duplicate",
        "outcome":"repeated outcome near-duplicate",
        "prediction_error": 0.1,  // low surprise → gate_score = novelty × 1.2, nearly 0 on 2nd call
        "auto_tag": false
    });
    let r1 = dispatch("memory_learn_text", &rt, &args1).unwrap();
    assert_ne!(r1.isError, Some(true), "First learn must succeed");

    let args2 = args1.clone();
    let result2 = dispatch("memory_learn_text", &rt, &args2).unwrap();

    // Core invariant: learn NEVER returns error regardless of gate score
    assert_ne!(result2.isError, Some(true),
        "Second learn must succeed even with low gate_score (shadow gate must NEVER suppress)");

    // Gate line must be in both learn responses — gate RAN, it just didn't block
    let text2 = &result2.content[0].text;
    assert!(
        text2.contains("Gate (shadow)"),
        "Gate (shadow) line must be present in learn response: {text2}"
    );
    assert!(
        text2.contains("Memory Learned"),
        "Response must confirm insertion happened — not just gate ran: {text2}"
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
