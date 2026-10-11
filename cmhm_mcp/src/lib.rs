pub mod schema;

/// Standing orders injected into the model's context on every session via MCP
/// initialize. Deliberately tiered (B4): setup is one call, retrieval and
/// learning fire on real events, and the text is honest about what the
/// numbers mean. Lives in the lib so its properties are testable.
pub const CMHM_INSTRUCTIONS: &str = r#"
You have a causal memory store (CMHM). Use it when it can help and skip it when it can't - every call costs time and tokens.

## Setup (once per session)
Call `project_detect`. It returns a ready-to-use `context_id`: it reuses this project's context or creates one, and returns the same id on repeat calls. Use that id for the session; no further setup is needed.

## Retrieve - only when there is something to recall
Call `memory_retrieve_text` on that context only when:
- you start a task in a domain where this project has history,
- you hit an error, a failed build, or a dead end, or
- you are choosing between real alternatives and a past attempt is plausible.
Skip it for trivial edits, routine lookups and routine steps.

## Learn - only outcome-changing events
Call `memory_learn_text` when an event changes what you would do next: a failure you diagnosed or fixed, a decision you made, a configuration or pitfall you discovered. Skip routine progress ("ran the tests", "edited a file"). One summary at the end of a session is enough.

## Reading results honestly
Retrievals return the stored text plus edge statistics. `evidence` is a count of observations, not a confidence percentage. A `no_match` routing result means nothing relevant exists - create a context only if the work is worth remembering later.
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
        "project_detect"       => Some(tools::handle_project_detect(rt, args)),
        "memory_promote"       => Some(tools::handle_memory_promote(rt, args)),
        _ => None,
    }
}
