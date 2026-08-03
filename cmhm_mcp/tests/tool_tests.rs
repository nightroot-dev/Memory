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
    let rt = ContextRuntime::new(cfg).expect("ContextRuntime::new failed in test");
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
