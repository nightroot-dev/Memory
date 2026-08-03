pub mod schema;
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
        "project_detect"       => Some(tools::handle_project_detect(rt, args)),
        _ => None,
    }
}
