use serde::{Deserialize, Serialize};
use serde_json::Value;
use schemars::JsonSchema;

// --- JSON-RPC 2.0 Base Types -------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: Some(result), error: None }
    }
    pub fn err(id: Option<Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message: message.into() }),
        }
    }
}

// --- MCP Protocol Types ------------------------------------------------------

#[allow(non_snake_case)]
#[derive(Debug, Serialize)]
pub struct InitializeResult {
    pub protocolVersion: String,
    pub capabilities: ServerCapabilities,
    pub serverInfo: ServerInfo,
    /// Plain-text guidance sent to the model on every session via MCP spec.
    /// MCP clients surface this automatically — no manual pasting needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    pub tools:   ToolsCapability,
    pub prompts: PromptsCapability,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize)]
pub struct ToolsCapability   { pub listChanged: bool }

#[allow(non_snake_case)]
#[derive(Debug, Serialize)]
pub struct PromptsCapability { pub listChanged: bool }

#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name:    String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct ToolsListResult {
    pub tools: Vec<ToolDefinition>,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize)]
pub struct ToolDefinition {
    pub name:        String,
    pub description: String,
    pub inputSchema: Value,
}

#[derive(Debug, Deserialize)]
pub struct ToolCallParams {
    pub name:      String,
    pub arguments: Option<Value>,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize)]
pub struct ToolCallResult {
    pub content: Vec<ContentItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isError: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ContentItem {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text:         String,
}

impl ContentItem {
    pub fn text(s: impl Into<String>) -> Self {
        Self { content_type: "text".to_string(), text: s.into() }
    }
    /// Emit a machine-readable JSON block as a second content item.
    /// Callers (tests, agents) can parse `content[1].text` as JSON.
    /// The `type` field is "text" for MCP wire-format compatibility.
    pub fn json(v: &serde_json::Value) -> Self {
        Self {
            content_type: "text".to_string(),
            text: format!("```json\n{}\n```", serde_json::to_string_pretty(v)
                .unwrap_or_else(|_| "{}".to_string())),
        }
    }
}


// --- Tool Input Schemas (auto-derived via schemars) --------------------------
//
// Every field here is the single source of truth for:
//   • the JSON schema sent to MCP clients in tools/list
//   • the parameter parsing in tool handlers
//
// To add a field: add it here → the schema updates automatically on next build.
// No manual inputSchema JSON editing required.

/// Query the causal memory for the most relevant history.
/// Call this BEFORE making a decision to recall what was previously learned.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetrieveParams {
    /// Base64-encoded little-endian float32 array, exactly 128 floats (512 bytes).
    pub query_embedding: String,

    /// Embedding model identifier. Used to select the projection matrix.
    /// Use "default" if no specific model projection is registered.
    #[serde(default = "default_model")]
    #[schemars(default = "default_model")]
    pub model_id: String,

    /// UUID or display name of the context to retrieve from.
    /// Omit to use the default context.
    pub context_id: Option<String>,

    /// The kind of memory node to use as the retrieval seed.
    ///
    /// **Required.** Controls which node roles are considered as seed candidates:
    /// - `"state"` — retrieve the causal chain most relevant to the current situation (default use case).
    /// - `"action"` — seed from action nodes.
    /// - `"outcome"` — seed from outcome nodes.
    ///
    /// Unknown values return an `invalid_params` error.
    pub retrieval_intent: String,
}


/// Record a (state → action → outcome) learning signal into causal memory.
/// Call this AFTER an action's result is known.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LearnParams {
    /// Base64-encoded 128-dim float32 embedding of the situation (state).
    pub state_embedding: String,

    /// Base64-encoded 128-dim float32 embedding of the action taken.
    pub action_embedding: String,

    /// Base64-encoded 128-dim float32 embedding of what happened (outcome).
    pub outcome_embedding: String,

    /// How wrong was the prediction? 0.0 = perfect outcome, 1.0 = complete failure.
    #[schemars(range(min = 0.0, max = 1.0))]
    pub prediction_error: f32,

    /// Importance / urgency of this experience in [0, 1]. Boosts memory salience.
    #[serde(default = "default_eta")]
    #[schemars(default = "default_eta")]
    pub emotional_tag: f32,

    /// Embedding model identifier. Defaults to "default".
    #[serde(default = "default_model")]
    #[schemars(default = "default_model")]
    pub model_id: String,

    /// UUID or display name of the context to learn into.
    /// Omit to use the default context.
    pub context_id: Option<String>,

    /// Explicit source-trust level ∈ [0, 1] for the state node.
    /// 0.0 = explicitly contradicted. 1.0 = explicitly authoritative. Default 0.5 = neutral.
    #[serde(default = "default_authority")]
    #[schemars(default = "default_authority")]
    pub authority: f32,

    /// Machine-readable source class (e.g. `"user_explicit"`, `"corrected"`, `""` = unknown).
    /// Non-empty differing provenance between top-2 candidates triggers conflict detection.
    #[serde(default)]
    pub provenance: String,

    /// Optional node ID this experience supersedes.
    /// The engine sets `old_node.superseded_by = Some(new_state_id)`, excluding the stale
    /// node from future seed selection.
    pub supersedes: Option<u64>,

    /// Optional semantic assertion / entity key (e.g. `"db_pool_size"`, `"compiler_target"`).
    #[serde(default)]
    pub assertion_key: Option<String>,

    /// Optional epistemic polarity: `+1` (assert/true), `-1` (refute/false), or `None` (neutral).
    #[serde(default)]
    pub polarity: Option<i8>,
}




/// Create a new isolated memory context for a project, topic, or session.
/// Each context has its own completely separate causal graph.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateContextParams {
    /// Human-readable label for this context (e.g. "Rust Rewrite", "User Research").
    pub display_name: String,

    /// Scope of the context. Controls how it is displayed and filtered.
    #[serde(default = "default_context_type")]
    #[schemars(default = "default_context_type")]
    pub context_type: ContextTypeParam,

    /// UUID of a parent context. Creates a DAG hierarchy (optional).
    pub parent_id: Option<String>,

    /// Free-text description used by the router to match queries.
    /// Include domain keywords here (e.g. "video processing python cv2").
    #[serde(default)]
    pub description: String,

    /// Keywords used for automatic context routing.
    /// The router scores contexts by tag overlap with a query.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Context type — controls scope and display grouping.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ContextTypeParam {
    Session,
    Project,
    Branch,
    Workspace,
    Global,
}

/// List all memory contexts visible to this server.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListContextsParams {
    /// Filter by scope. Omit to return all contexts.
    pub context_type: Option<ContextTypeParam>,
}

/// Permanently delete a context and all its memory data.
/// This operation is irreversible. The default context cannot be deleted.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteContextParams {
    /// UUID or display name of the context to permanently erase.
    pub context_id: String,
}

/// Route a query to the best-matching context(s) using probabilistic scoring.
/// Returns a ranked list. Call this when you are unsure which context to use.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RouteParams {
    /// Plain-text query or keywords. Scored against context names, tags, and descriptions.
    pub query: String,

    /// Number of top contexts to return. Defaults to 3.
    #[serde(default = "default_top_k")]
    #[schemars(default = "default_top_k", range(min = 1.0, max = 20.0))]
    pub top_k: usize,
}

/// Retrieve statistics for a memory context.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryStatsParams {
    /// UUID or display name. Omit for the default context.
    pub context_id: Option<String>,
}

/// Detect the current project from the working environment.
/// No parameters needed — the server reads its own cwd, git, and manifests.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectDetectParams {}

// --- Defaults ----------------------------------------------------------------

pub fn default_model()        -> String { "default".to_string() }
pub fn default_eta()          -> f32    { 0.5 }
pub fn default_authority()    -> f32    { 0.5 }
pub fn default_context_type() -> ContextTypeParam { ContextTypeParam::Project }
pub fn default_top_k()        -> usize  { 3 }
pub fn default_true()         -> bool   { true }

// --- Text-native Tool Schemas ------------------------------------------------
//
// These are the PREFERRED tools for most agents.
// The server converts plain text to embeddings internally — no Base64 or
// float32 array generation needed on the agent side.

/// Record a (state → action → outcome) experience as plain text.
///
/// **Preferred over `memory_learn`** — no embedding generation required.
/// The server converts text to a deterministic hash-projection embedding
/// and optionally extracts routing tags to improve future `context_route` accuracy.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LearnTextParams {
    /// Plain text describing the situation that was observed (the problem, context, state).
    /// Example: "opencode config does not include modelx-9.2 in the nvidia provider"
    pub state: String,

    /// Plain text describing the action taken.
    /// Example: "added provider.nvidia.models with status:active override to opencode.jsonc"
    pub action: String,

    /// Plain text describing what happened (the outcome / result).
    /// Example: "model modelx-9.2 now appears in opencode model selector"
    pub outcome: String,

    /// How wrong was the prediction? 0.0 = perfect outcome, 1.0 = complete failure.
    /// Use 0.0–0.3 for successes, 0.7–1.0 for failures, 0.4–0.6 for mixed results.
    #[schemars(range(min = 0.0, max = 1.0))]
    pub prediction_error: f32,

    /// Importance / urgency of this experience in [0, 1]. Boosts memory salience.
    /// Default 0.5. Use higher values for critical or hard-won knowledge.
    #[serde(default = "default_eta")]
    #[schemars(default = "default_eta")]
    pub emotional_tag: f32,

    /// UUID or display name of the context to learn into.
    /// Omit to use the default (or currently active) context.
    pub context_id: Option<String>,

    /// Automatically extract keywords from the text and add them to the
    /// context's routing tags. Improves future `context_route` accuracy.
    /// Default: true.
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    pub auto_tag: bool,

    /// Explicit source-trust level ∈ [0, 1] for the state node.
    /// 0.0 = explicitly contradicted. 1.0 = explicitly authoritative. Default 0.5 = neutral.
    #[serde(default = "default_authority")]
    #[schemars(default = "default_authority")]
    pub authority: f32,

    /// Machine-readable source class (e.g. `"user_explicit"`, `"corrected"`, `""` = unknown).
    #[serde(default)]
    pub provenance: String,

    /// Optional node ID this experience supersedes.
    /// See `memory_learn` documentation for full semantics.
    pub supersedes: Option<u64>,

    /// Optional semantic assertion / entity key (e.g. `"db_pool_size"`, `"compiler_target"`).
    #[serde(default)]
    pub assertion_key: Option<String>,

    /// Optional epistemic polarity: `+1` (assert/true), `-1` (refute/false), or `None` (neutral).
    #[serde(default)]
    pub polarity: Option<i8>,
}



/// Retrieve causal memory using plain text — no embeddings needed.
///
/// **Preferred over `memory_retrieve`** — no embedding generation required.
/// Call this BEFORE making a decision to recall what was previously learned
/// about similar situations.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetrieveTextParams {
    /// Plain text query describing the situation you want to recall history for.
    /// Example: "nvidia provider missing model in opencode catalog"
    pub query: String,

    /// UUID or display name of the context to retrieve from.
    /// Omit to use the default context, or use the ID returned by `context_route`.
    pub context_id: Option<String>,

    /// The kind of memory node to use as the retrieval seed.
    ///
    /// **Optional.** Controls which node roles are considered as seed candidates:
    /// - `"state"` — restrict seed candidates to State nodes (default use case).
    /// - `"action"` — restrict seed candidates to Action nodes.
    /// - `"outcome"` — restrict seed candidates to Outcome nodes.
    /// - Omit entirely (null / absent) — **unscoped retrieval**: all node types compete
    ///   in the candidate pool. Required for cross-type contradiction safety evaluation.
    ///
    /// Unknown non-null values return an `invalid_params` error.
    #[serde(default)]
    pub retrieval_intent: Option<String>,
}


/// Provide outcome feedback after a retrieve → act → observe cycle.
///
/// Call this after `memory_retrieve_text` returned a causal chain and you now know
/// whether following that advice led to success or failure.
///
/// ## What this does
/// - Updates the Beta(α, β) usefulness posterior for each node that was retrieved.
/// - Updates the half-life of each node (success → extends life, failure → shortens it).
/// - These updates make future retrieval scores more accurate over time.
///
/// ## Example usage
/// ```text
/// // After retrieve returned advice that led to a build fix:
/// memory_feedback { node_ids: [12, 34, 67], outcome: "success", context_id: "..." }
///
/// // After retrieve returned advice that led to a wrong approach:
/// memory_feedback { node_ids: [12, 34], outcome: "failure", context_id: "..." }
/// ```
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryFeedbackParams {
    /// List of node IDs that were retrieved and acted upon.
    /// These are the IDs shown in the `causal_chain` of the `memory_retrieve_text` response.
    /// Distinguishes *used* nodes from *shown* nodes: include only nodes you acted on.
    pub node_ids: Vec<u64>,

    /// Outcome of acting on the retrieved memory.
    /// `"success"`: the retrieved advice was correct and led to the desired outcome.
    /// `"failure"`: the retrieved advice was wrong or did not help.
    pub outcome: String,

    /// UUID or display name of the context the nodes belong to.
    /// Must match the context used for the retrieve call.
    pub context_id: Option<String>,

    /// Retrieve ID returned by the preceding `memory_retrieve_text` or `memory_retrieve` call.
    /// Format: `"retrieve_id: <uuid>"` — copy the UUID from the response.
    ///
    /// **Enables idempotency**: if the same retrieve_id is submitted more than once,
    /// only the first feedback application changes the bandit/decay state.
    /// Subsequent calls return a `"already applied"` acknowledgement without mutating state.
    ///
    /// **Enables join**: links this feedback to the retrieve log entry for offline analysis.
    /// Omit only if you cannot track retrieve IDs (no deduplication in that case).
    pub retrieve_id: Option<String>,
}
