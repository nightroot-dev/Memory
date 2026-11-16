// cmhm_mcp/tests/phase5_preflight.rs
//
// Phase 5 Slice 4 gate: a recorded BLOCKED_PROCESS must veto the same planned
// action, an explicit human override must bypass it once, and an unrelated
// action must remain allowed. EXPERIENCE_RULE entries are advisory only.

use std::sync::Arc;
use parking_lot::Mutex;
use serde_json::{json, Value};
use tempfile::TempDir;

use cmhm_core::{Config, ContextRuntime};
use cmhm_mcp_lib::{dispatch, schema::ToolCallResult, tools};

fn make_runtime(tmp: &TempDir) -> tools::SharedRuntime {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CARGO_MANIFEST_DIR has no parent");
    let cfg = Config {
        data_dir: tmp.path().to_path_buf(),
        projections_dir: workspace_root.join("data").join("projections"),
        ..Config::default()
    };
    let rt = ContextRuntime::new(cfg, "phase5-preflight".to_string()).unwrap();
    Arc::new(Mutex::new(rt))
}

fn json_block(r: &ToolCallResult) -> Value {
    assert_eq!(r.content.len(), 2, "expected text + JSON block");
    let text = r.content[1].text.trim();
    let inner = text
        .strip_prefix("```json")
        .and_then(|s| s.strip_suffix("```"))
        .expect("JSON block is fenced")
        .trim();
    serde_json::from_str(inner).expect("valid JSON block")
}

#[test]
fn blocked_process_vetoes_override_and_rule_is_advisory() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    // 1. Record a failed workflow.
    let recorded = dispatch(
        "record_experience",
        &rt,
        &json!({
            "disposition": "blocked_process",
            "domain": "rust",
            "tool_or_module": "cargo",
            "symbols": ["E0382", "worker.rs"],
            "symptom": "use of moved value when the worker thread captures the payload",
            "decision_payload": "Never move a captured payload into a scoped thread without Arc; prefer Arc<Mutex<_>> and clone the Arc.",
            "action_signature": "cargo build --release",
            "root_cause": "payload was moved into the thread closure and then used after join",
            "recommended_fix": "clone the payload or wrap it in Arc before spawning",
            "failure_count": 2
        }),
    )
    .expect("record_experience is registered");
    assert_ne!(recorded.isError, Some(true), "record failed: {:?}", recorded.content);
    let rec = json_block(&recorded);
    assert_eq!(rec["status"], "RECORDED");
    let blocked_id = rec["node_id"].as_u64().expect("node id");
    assert_eq!(rec["disposition"], "blocked_process");
    assert_eq!(rec["blocked"]["failure_count"], 2);

    // 2. The exact action must be vetoed.
    let blocked = dispatch(
        "preflight_check",
        &rt,
        &json!({
            "planned_action": "cargo build --release",
            "target": "worker.rs",
            "context": "E0382 use of moved value"
        }),
    )
    .expect("preflight_check is registered");
    assert_ne!(blocked.isError, Some(true), "preflight errored: {:?}", blocked.content);
    let b = json_block(&blocked);
    assert_eq!(b["status"], "BLOCKED_BY_EXPERIENCE", "response: {b}");
    assert_eq!(b["rule_id"].as_u64(), Some(blocked_id));
    assert_eq!(b["requires_user_override"], true);
    assert!(b["confidence"].as_f64().unwrap() >= 0.78, "confidence: {}", b["confidence"]);

    // 3. An explicit human override bypasses the veto.
    let overridden = dispatch(
        "preflight_check",
        &rt,
        &json!({
            "planned_action": "cargo build --release",
            "target": "worker.rs",
            "context": "E0382 use of moved value",
            "user_override": true
        }),
    )
    .expect("preflight_check is registered");
    let o = json_block(&overridden);
    assert_eq!(o["status"], "OVERRIDE_GRANTED", "response: {o}");
    assert_eq!(o["requires_user_override"], false);

    // 4. An unrelated action stays allowed.
    let allowed = dispatch(
        "preflight_check",
        &rt,
        &json!({ "planned_action": "cargo fmt --check" }),
    )
    .expect("preflight_check is registered");
    let a = json_block(&allowed);
    assert_eq!(a["status"], "ALLOWED", "response: {a}");

    // 5. EXPERIENCE_RULE is advisory, never a veto.
    let rule = dispatch(
        "record_experience",
        &rt,
        &json!({
            "disposition": "experience_rule",
            "domain": "postgres",
            "tool_or_module": "pgbackrest",
            "symbols": ["archive_mode"],
            "symptom": "WAL archiving stops after a replica failover",
            "decision_payload": "Set archive_mode=always on replicas, not only on the primary"
        }),
    )
    .expect("record_experience is registered");
    assert_ne!(rule.isError, Some(true), "rule record failed: {:?}", rule.content);
    let rule_json = json_block(&rule);
    assert_eq!(rule_json["disposition"], "experience_rule");

    let advisory = dispatch(
        "preflight_check",
        &rt,
        &json!({
            "planned_action": "enable archive_mode on the replica",
            "context": "WAL archiving stopped after failover"
        }),
    )
    .expect("preflight_check is registered");
    let adv = json_block(&advisory);
    assert_eq!(adv["status"], "ALLOWED", "rule must not veto: {adv}");
    assert_eq!(
        adv["advisory_rule"]["rule_id"].as_u64(),
        rule_json["node_id"].as_u64(),
        "advisory rule must be the recorded experience_rule"
    );

    // 6. The typed payload survives the read path.
    let got = dispatch(
        "memory_get",
        &rt,
        &json!({ "node_id": blocked_id }),
    )
    .expect("memory_get is registered");
    let got_json = json_block(&got);
    assert_eq!(got_json["disposition"], "blocked_process");
    assert_eq!(got_json["blocked"]["action_signature"], "cargo build --release");
}

#[test]
fn paraphrased_blocked_action_still_vetoes_and_override_is_one_shot() {
    let tmp = TempDir::new().unwrap();
    let rt = make_runtime(&tmp);

    let recorded = dispatch(
        "record_experience",
        &rt,
        &json!({
            "disposition": "blocked_process",
            "domain": "rust",
            "tool_or_module": "cargo",
            "symbols": ["E0382", "worker.rs"],
            "symptom": "use of moved value when the worker thread captures the payload",
            "decision_payload": "Wrap the payload in Arc and clone the Arc before spawning.",
            "action_signature": "cargo build --release",
            "root_cause": "payload moved into the thread closure",
            "recommended_fix": "clone an Arc instead of moving the payload",
            "failure_count": 1
        }),
    )
    .expect("record_experience is registered");
    assert_ne!(recorded.isError, Some(true), "record failed: {:?}", recorded.content);
    let blocked_id = json_block(&recorded)["node_id"].as_u64().expect("node id");

    // Case and trailing punctuation must not evade the action anchor.
    for variant in ["CARGO BUILD --RELEASE", "cargo build --release;"] {
        let r = dispatch(
            "preflight_check",
            &rt,
            &json!({
                "planned_action": variant,
                "target": "worker.rs",
                "context": "E0382 use of moved value"
            }),
        )
        .expect("preflight_check is registered");
        let v = json_block(&r);
        assert_eq!(v["status"], "BLOCKED_BY_EXPERIENCE", "variant {variant:?}: {v}");
        assert_eq!(v["rule_id"].as_u64(), Some(blocked_id));
    }

    // A paraphrase that keeps the real command tokens must still veto.
    let paraphrase = dispatch(
        "preflight_check",
        &rt,
        &json!({
            "planned_action": "please run cargo build in release mode",
            "target": "worker.rs",
            "context": "E0382 moved value in the worker thread"
        }),
    )
    .expect("preflight_check is registered");
    let p = json_block(&paraphrase);
    assert_eq!(p["status"], "BLOCKED_BY_EXPERIENCE", "paraphrase: {p}");
    assert!(
        p["action_overlap"].as_f64().unwrap() >= 0.34,
        "paraphrase overlap too low: {p}"
    );

    // Sharing only the tool name must not veto an unrelated subcommand.
    let shared_tool = dispatch(
        "preflight_check",
        &rt,
        &json!({ "planned_action": "cargo fmt --check", "target": "worker.rs" }),
    )
    .expect("preflight_check is registered");
    let s = json_block(&shared_tool);
    assert_eq!(s["status"], "ALLOWED", "shared tool name vetoed: {s}");

    // Override is one-shot: the next call without it is blocked again.
    let override_once = dispatch(
        "preflight_check",
        &rt,
        &json!({
            "planned_action": "cargo build --release",
            "target": "worker.rs",
            "context": "E0382 use of moved value",
            "user_override": true
        }),
    )
    .expect("preflight_check is registered");
    assert_eq!(json_block(&override_once)["status"], "OVERRIDE_GRANTED");

    let after_override = dispatch(
        "preflight_check",
        &rt,
        &json!({
            "planned_action": "cargo build --release",
            "target": "worker.rs",
            "context": "E0382 use of moved value"
        }),
    )
    .expect("preflight_check is registered");
    assert_eq!(
        json_block(&after_override)["status"],
        "BLOCKED_BY_EXPERIENCE",
        "override must not persist"
    );
}
