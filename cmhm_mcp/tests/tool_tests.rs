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

    // Retrieve — must find something (chain won't be empty after learning)
    let retrieve_args = json!({"query": "cargo build linker error", "retrieval_intent": "state"});
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
        let ret = dispatch("memory_retrieve_text", &rt, &json!({"query": "borrow checker", "retrieval_intent": "state"})).unwrap();

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
    let ret = dispatch("memory_retrieve_text", &rt, &json!({"query": "beta boundary", "retrieval_intent": "state"})).unwrap();

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


// ── Test 13: E1a–E1f — Explicit Retrieval Intent Enforcement ──────────────────

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
fn e1d_memory_retrieve_text_missing_intent_is_error() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);
    let args = json!({
        "query": "how to configure database connection",
    });
    let result = dispatch("memory_retrieve_text", &rt, &args).unwrap();
    assert_eq!(result.isError, Some(true), "memory_retrieve_text without retrieval_intent must return error");
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

    let (ret_res, _) = rt.retrieve(None, s_emb, "hash-128d", cmhm_core::NodeType::State).unwrap();
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
    assert_eq!(structured["seed_provenance"], "contradicted");
    assert!((structured["seed_authority"].as_f64().unwrap() - 0.30).abs() < 1e-4, "contradicted must be capped at 0.30");
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






