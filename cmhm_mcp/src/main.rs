mod schema;
mod tools;

use std::io::{self, BufRead, Write};
use std::sync::Arc;
use parking_lot::Mutex;
use anyhow::Result;
use serde_json::{json, Value};
use tracing::info;

use cmhm_core::{Config, ContextRuntime};
use uuid::Uuid;
use schema::{JsonRpcRequest, JsonRpcResponse, ToolCallParams, InitializeResult, ToolsListResult};
use tools::SharedRuntime;
// dispatch() lives in lib.rs — single source of truth, testable
use cmhm_mcp_lib::dispatch;

/// Standing orders injected into the model's context on every session via MCP initialize.
/// Written as behavioral rules — the model follows these autonomously, no user prompting.
const CMHM_INSTRUCTIONS: &str = r#"
You are connected to a causal memory system (CMHM). Follow these rules automatically on every turn — do not wait for the user to ask.

## Rule 0 — Session start: detect the project automatically
At the very start of every session, call `project_detect` (no parameters).
It returns the working directory, project name from manifest (Cargo.toml / package.json / pyproject.toml), git remote, and a recommended action.
- If it says "is_default_dir: yes" → use the `default` context for the whole session.
- If it says "is_default_dir: no" → follow its recommended action (route then create if needed).
Never ask the user for the project name. Always detect it yourself.

## Rule 1 — Before every decision, retrieve memory
Before writing code, choosing a library, designing an API, or making any non-trivial decision:
1. Call `context_route` with a 2-5 word description of the current task.
2. Take the top-ranked context.
3. Call `memory_retrieve_text` on that context with a plain-text description of the current situation.
4. Read the causal chain. If it shows a past failure for this approach, prefer the alternative that succeeded.

## Rule 2 — After every action, record the outcome
After any action whose result is now known (build succeeded/failed, test passed/failed, API call returned, decision proved correct/wrong):
1. Call `memory_learn_text` with:
   - `state`:  plain text describing the situation before the action
   - `action`: plain text describing what you did
   - `outcome`: plain text describing what happened
   - `prediction_error`: 0.0 if outcome was expected, 1.0 if complete surprise or failure
   - `auto_tag`: true (default) — automatically tags the context with keywords for better future routing
2. Use the same context as the retrieve call.

## Rule 3 — New project or topic = new context
When `project_detect` returns a project that has no matching context (context_route score < 0.5):
- Call `context_create` with display_name from project_detect, relevant tags, and a domain description.
- Use that context's UUID for all memory calls this session.

## Rule 4 — Embeddings (advanced / optional)
Only use `memory_retrieve` / `memory_learn` (embedding variants) if you have access to a real embedding
model. Encode text as a 128-dim float32 vector, normalize it, then Base64-encode as little-endian bytes
(512 bytes total). Use `model_id` to identify the model (e.g. "deepseek", "model-a", "openai").
For most tasks, the text variants (`memory_retrieve_text` / `memory_learn_text`) are preferred.

## What each tool does
- `project_detect`        → reads cwd, manifest, git to identify the project. Call first. No parameters.
- `context_route`         → given a text query, returns ranked contexts by relevance score [0-1]
- `memory_retrieve_text`  → retrieve causal history using plain text — PREFERRED
- `memory_learn_text`     → record (state → action → outcome) as plain text — PREFERRED, auto-tags context
- `memory_retrieve`       → retrieve using a Base64-encoded embedding (advanced)
- `memory_learn`          → record using Base64-encoded embeddings (advanced)
- `context_create`        → creates a new isolated memory context (nothing leaks between contexts)
- `context_list`          → lists all contexts with UUIDs, states, learn counts
- `context_delete`        → permanently removes a context (irreversible)
- `memory_stats`          → returns node count, version, whether context is loaded in RAM
"#;

fn main() -> Result<()> {
    // Tracing to stderr (stdout is reserved for MCP JSON-RPC)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            std::env::var("CMHM_LOG").unwrap_or_else(|_| "info".to_string())
        )
        .init();

    info!("CMHM MCP server starting (Context Runtime enabled)");

    let cfg = load_config();
    info!("Data dir: {:?} | max_active_contexts: {}", cfg.data_dir, cfg.max_active_contexts);

    // Generate a UUID v4 session ID — constant for this process lifetime.
    // Written into every learn_log.jsonl and retrieve_log.jsonl entry
    // so all log files from one session can be joined by session_id.
    let session_id = Uuid::new_v4().to_string();
    info!("Session ID: {}", session_id);

    // Start the Context Runtime (replaces the single EngineHandle)
    let runtime = ContextRuntime::new(cfg, session_id)?;
    let shared: SharedRuntime = Arc::new(Mutex::new(runtime));
    info!("Context Runtime started");

    // MCP stdio loop
    let stdin  = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l)  => l,
            Err(e) => { tracing::error!("stdin read error: {e}"); break; }
        };
        if line.trim().is_empty() { continue; }

        let response = handle_message(&line, &shared);
        let serialized = serde_json::to_string(&response)?;
        writeln!(out, "{}", serialized)?;
        out.flush()?;
    }

    info!("stdin closed — shutting down Context Runtime");
    // Crash-only design: WAL ensures consistency regardless. Shutdown is best-effort.
    match Arc::try_unwrap(shared) {
        Ok(mutex) => {
            let mut rt = mutex.into_inner(); // parking_lot: returns T, no Result
            rt.shutdown();
        }
        Err(_) => tracing::warn!("Could not unwrap runtime Arc — WAL ensures consistency"),
    }
    Ok(())
}

fn handle_message(line: &str, rt: &SharedRuntime) -> JsonRpcResponse {
    let req: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(r)  => r,
        Err(e) => return JsonRpcResponse::err(None, -32700, format!("Parse error: {e}")),
    };

    let id = req.id.clone();

    match req.method.as_str() {
        // ── MCP Lifecycle ──────────────────────────────────────────────
        "initialize" => {
            let result = InitializeResult {
                protocolVersion: "2024-11-05".to_string(),
                capabilities: schema::ServerCapabilities {
                    tools:   schema::ToolsCapability   { listChanged: false },
                    prompts: schema::PromptsCapability { listChanged: false },
                },
                serverInfo: schema::ServerInfo {
                    name:    "cmhm-mcp".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                instructions: Some(CMHM_INSTRUCTIONS.to_string()),
            };
            match serde_json::to_value(result) {
                Ok(v)  => JsonRpcResponse::ok(id, v),
                Err(e) => JsonRpcResponse::err(id, -32603, format!("Internal: serialize error: {e}")),
            }
        }

        "notifications/initialized" | "initialized" => {
            JsonRpcResponse::ok(id, Value::Null)
        }

        "ping" => JsonRpcResponse::ok(id, json!({})),

        // ── Tool Discovery ─────────────────────────────────────────────
        "tools/list" => {
            let result = ToolsListResult { tools: tools::tool_definitions() };
            match serde_json::to_value(result) {
                Ok(v)  => JsonRpcResponse::ok(id, v),
                Err(e) => JsonRpcResponse::err(id, -32603, format!("Internal: serialize error: {e}")),
            }
        }

        // ── Prompt Discovery ───────────────────────────────────────────
        "prompts/list" => {
            JsonRpcResponse::ok(id, json!({
                "prompts": [{
                    "name": "cmhm-guide",
                    "description": "How to use CMHM: context lifecycle, memory workflow, and routing."
                }]
            }))
        }

        "prompts/get" => {
            JsonRpcResponse::ok(id, json!({
                "description": "CMHM usage guide",
                "messages": [{
                    "role": "user",
                    "content": { "type": "text", "text": CMHM_INSTRUCTIONS }
                }]
            }))
        }

        // ── Tool Execution ─────────────────────────────────────────────
        "tools/call" => {
            let params: ToolCallParams = match req.params
                .and_then(|p| serde_json::from_value(p).ok())
            {
                Some(p) => p,
                None    => return JsonRpcResponse::err(id, -32602, "Invalid params for tools/call"),
            };

            let args = params.arguments.as_ref()
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            // Single dispatch table lives in lib.rs — tested for parity there
            let tool_result = match dispatch(&params.name, rt, &args) {
                Some(r) => r,
                None => return JsonRpcResponse::err(
                    id, -32601,
                    format!("Unknown tool: '{}'. Call tools/list to see available tools.",
                        params.name)
                ),
            };

            match serde_json::to_value(tool_result) {
                Ok(v)  => JsonRpcResponse::ok(id, v),
                Err(e) => JsonRpcResponse::err(id, -32603, format!("Internal: serialize error: {e}")),
            }
        }

        // ── Unknown method ─────────────────────────────────────────────
        other => {
            tracing::debug!("Unknown MCP method: {other}");
            JsonRpcResponse::err(id, -32601, format!("Method not found: {other}"))
        }
    }
}

// --- Config discovery --------------------------------------------------------
// Agent-agnostic: no hardcoded agent names, project paths, or model IDs.
// Search order:
//   1. CMHM_CONFIG env var  (any MCP client can set this)
//   2. config.toml next to the binary
//   3. config.toml in current working directory
//   4. <UserConfigDir>/cmhm/config.toml
//   5. Built-in defaults (data/ relative to cwd)

fn load_config() -> Config {
    if let Ok(path) = std::env::var("CMHM_CONFIG") {
        let p = std::path::PathBuf::from(&path);
        match Config::from_file(&p) {
            Ok(cfg) => { info!("Loaded config from CMHM_CONFIG={:?}", p); return cfg; }
            Err(e)  => tracing::warn!("CMHM_CONFIG={:?} set but failed to parse: {e}", p),
        }
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.toml")));
    let cwd  = Some(std::path::PathBuf::from("config.toml"));
    let user = user_config_dir().map(|d| d.join("cmhm").join("config.toml"));

    for candidate in [exe_dir, cwd, user].iter().flatten() {
        if candidate.exists() {
            match Config::from_file(candidate) {
                Ok(cfg) => { info!("Loaded config from {:?}", candidate); return cfg; }
                Err(e)  => tracing::warn!("Failed to parse {:?}: {e}", candidate),
            }
        }
    }

    info!("No config.toml found — using built-in defaults");
    Config::default()
}

fn user_config_dir() -> Option<std::path::PathBuf> {
    if let Ok(v) = std::env::var("APPDATA") { return Some(v.into()); }
    if let Ok(v) = std::env::var("XDG_CONFIG_HOME") { return Some(v.into()); }
    std::env::var("HOME").ok().map(|h| std::path::PathBuf::from(h).join(".config"))
}
