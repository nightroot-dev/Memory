pub mod schema;

/// Instructions injected into the model's context on session start via MCP
/// initialize. Demoted from standing orders to on-request guidance after the
/// pre-registered usefulness window measured 0 feedback closes across 7
/// retrievals in 4 sessions (2026-10-26..31): look things up when stuck or
/// asked, learn only outcome-changing events, close the loop only when a
/// recall actually changed a decision. Kept lean and honest (B4 budget).
pub const CMHM_INSTRUCTIONS: &str = r#"
You have a local causal memory store (CMHM). Use it on request, not as a ritual: look something up when it could change a decision, skip it otherwise. Calls cost time and tokens.

## Setup (first use only)
Call `project_detect` when you first use the store. It returns a ready-to-use `context_id` (same id on repeat calls); pass that id to every call.

## Retrieve - on request
Call `memory_retrieve_text` only when:
- you hit an error, a failed build, or a dead end,
- the user asks what this project knows about something, or
- you are choosing between real alternatives and a past attempt is plausible.
Skip it for routine steps and when the context is already loaded.

## Learn - only outcome-changing events
Call `memory_learn_text` when an event changes what you would do next: a failure you diagnosed or fixed, a decision you made, a configuration or pitfall you discovered. Skip routine progress ("ran the tests"). When the learn carries a fix the user hit, quote the user's own symptom words in `state` - verbatim phrasing is what later queries match.

## Feedback - optional
If a recall actually changed what you did, close the loop once with `memory_feedback` (acted-on node_ids, `retrieve_id`, outcome "success"/"failure"). Skip it when the recall changed nothing.

## Reading results honestly
Retrievals return stored text plus edge statistics. `evidence` is a count of observations, not a confidence percentage. A `no_match` routing result means nothing relevant exists - create a context only if the work is worth remembering later.
"#;

pub mod tools;

use serde_json::Value;

/// Dispatch a tool call by name. Returns `None` if the tool name is not registered.
///
/// Centralising dispatch here (instead of in main.rs) means:
///   1. The parity test can verify every TOOLS registry entry has a dispatch arm.
///   2. main.rs becomes a thin stdio loop — no business logic.
///   3. Future transports (HTTP, WebSocket) share the same dispatch without duplication.
pub fn dispatch(name: &str, rt: &tools::SharedRuntime, args: &Value) -> Option<schema::ToolCallResult> {
    match name {
        "memory_retrieve"      => Some(tools::handle_retrieve(rt, args)),
        "memory_learn"         => Some(tools::handle_learn(rt, args)),
        "memory_retrieve_text" => Some(tools::handle_retrieve_text(rt, args)),
        "memory_learn_text"    => Some(tools::handle_learn_text(rt, args)),
        "memory_feedback"      => Some(tools::handle_memory_feedback(rt, args)),
        "context_create"       => Some(tools::handle_context_create(rt, args)),
        "context_list"         => Some(tools::handle_context_list(rt, args)),
        "context_delete"       => Some(tools::handle_context_delete(rt, args)),
        "context_route"        => Some(tools::handle_context_route(rt, args)),
        "memory_stats"         => Some(tools::handle_memory_stats(rt, args)),
        "memory_list"          => Some(tools::handle_memory_list(rt, args)),
        "memory_get"           => Some(tools::handle_memory_get(rt, args)),
        "project_detect"       => Some(tools::handle_project_detect(rt, args)),
        "memory_promote"       => Some(tools::handle_memory_promote(rt, args)),
        _ => None,
    }
}
