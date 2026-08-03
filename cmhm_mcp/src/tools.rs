use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::{json, Value};
use std::sync::Arc;
use parking_lot::Mutex;
use cmhm_core::{ContextRuntime, ContextType, text_to_embedding, extract_tags, scrub::scrub_learn};
use crate::schema::{
    RetrieveParams, LearnParams, LearnTextParams, RetrieveTextParams,
    CreateContextParams, ListContextsParams, DeleteContextParams,
    RouteParams, MemoryStatsParams, ContextTypeParam, ProjectDetectParams,
    MemoryFeedbackParams,
    ToolCallResult, ContentItem, ToolDefinition,
};

/// Model ID used for all text-path embeddings (memory_learn_text, memory_retrieve_text).
///
/// Must match `HashEmbedder::model_id()` so nodes stored by text tools are
/// retrievable by embedding tools (and vice versa) when the same hash backend is active.
/// When the `onnx` feature is enabled and adopted, update this to `"minilm-l6-v2-int8-384d"`.
const TEXT_MODEL_ID: &str = "hash-128d";

// --- Retrieve audit log (JSONL) -----------------------------------------------
//
// Every retrieve call appends one line to `retrieve_log.jsonl` in the data dir.
// This file is:
//   - Append-only, never truncated — safe to tail / import incrementally.
//   - Separate from the memory store — a bug in logging never affects retrieval.
//   - The labeled-pairs corpus for Stage 4 reranker training and offline replay.
//   - Required to fit λ/α/θ_min (N11) and validate bandit posteriors (N26).
//
// Schema (one JSON object per line):
//   { "ts": <unix_ms>, "ctx": "<uuid>", "model": "<model_id>",
//     "query": "<text or base64_hash>", "nodes": [<id>, ...], "n": <count> }

fn append_retrieve_log(
    data_dir: &std::path::Path,
    ctx_id: &str,
    model_id: &str,
    query_repr: &str,        // text for text-path, base64 hash for embedding-path
    node_ids: &[u64],
) {
    use std::io::Write;
    let path = data_dir.join("retrieve_log.jsonl");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let line = format!(
        "{{\"ts\":{},\"ctx\":\"{}\",\"model\":\"{}\",\"query\":{},\"nodes\":{:?},\"n\":{}}}\n",
        ts, ctx_id, model_id,
        serde_json::to_string(query_repr).unwrap_or_else(|_| "\"\"".to_string()),
        node_ids, node_ids.len()
    );
    // Best-effort: if the log write fails, do NOT fail the retrieve
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}

// --- Embedding helpers -------------------------------------------------------

pub fn decode_embedding(b64: &str) -> Result<Vec<f32>> {
    let bytes = BASE64.decode(b64.trim())?;
    if bytes.len() % 4 != 0 {
        anyhow::bail!("Embedding byte length {} is not divisible by 4", bytes.len());
    }
    let floats: Vec<f32> = bytes.chunks(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    if floats.iter().any(|x| x.is_nan()) {
        anyhow::bail!("Embedding contains NaN values");
    }
    Ok(floats)
}

#[allow(dead_code)]
pub fn encode_embedding(v: &[f32]) -> String {
    let bytes: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
    BASE64.encode(&bytes)
}

// --- Type for shared runtime -------------------------------------------------

pub type SharedRuntime = Arc<Mutex<ContextRuntime>>;

// --- Tool registry -----------------------------------------------------------
//
// Each entry defines exactly three things:
//   1. Tool name  (what MCP clients see)
//   2. Description (what the AI reads to decide when to call it)
//   3. inputSchema (auto-derived from the Params struct — never edit manually)
//
// To add a tool: add a new ToolEntry here + a handler below + a match arm in main.rs.
// To add a parameter: add a field to the Params struct in schema.rs.

struct ToolEntry {
    name:        &'static str,
    description: &'static str,
    schema:      fn() -> Value,
}

fn schema_of<T: schemars::JsonSchema>() -> Value {
    let schema = schemars::schema_for!(T);
    let raw = serde_json::to_value(schema).unwrap_or(json!({"type":"object"}));
    clean_schema(raw)
}

/// Strip schemars internals from a generated schema before sending to MCP clients.
///
/// Removes: $schema, title, definitions, format: "uint"
/// Inlines:  $ref → the definition it points to (so clients see flat properties)
fn clean_schema(mut v: Value) -> Value {
    if let Some(obj) = v.as_object_mut() {
        // Pull definitions out before we recurse so we can inline $refs
        let defs: serde_json::Map<String, Value> = obj
            .remove("definitions")
            .and_then(|d| d.as_object().cloned())
            .unwrap_or_default();

        // Strip metadata that's only meaningful to schemars / Rust
        obj.remove("$schema");
        obj.remove("title");

        // Inline $ref references in properties
        if let Some(props) = obj.get_mut("properties").and_then(|p| p.as_object_mut()) {
            for prop in props.values_mut() {
                *prop = resolve_ref(prop.clone(), &defs);
                // Strip per-field internals
                if let Some(o) = prop.as_object_mut() {
                    o.remove("title");
                    // "format: uint" is a Rust-ism; drop it — type: integer is enough
                    if o.get("format").and_then(|f| f.as_str()) == Some("uint") {
                        o.remove("format");
                    }
                }
            }
        }
    }
    v
}

/// Resolve a single value: if it is (or wraps) a `$ref`, inline the definition.
fn resolve_ref(v: Value, defs: &serde_json::Map<String, Value>) -> Value {
    match &v {
        // Plain $ref → inline the definition
        Value::Object(o) if o.contains_key("$ref") => {
            let r = o["$ref"].as_str().unwrap_or("");
            let key = r.strip_prefix("#/definitions/").unwrap_or(r);
            let mut inlined = defs.get(key).cloned().unwrap_or(v);
            // schemars emits enum definitions without a top-level "type" field.
            // Add "type":"string" if the definition has an "enum" array but no "type".
            if let Some(obj) = inlined.as_object_mut() {
                if obj.contains_key("enum") && !obj.contains_key("type") {
                    obj.insert("type".to_string(), json!("string"));
                }
                obj.remove("title");
            }
            inlined
        }
        // allOf: [{ $ref }] + possible default/description — schemars uses this for
        // fields that have both a type reference and a default value.
        // Flatten: inline the ref and merge back default + description.
        Value::Object(o) if o.contains_key("allOf") => {
            if let Some(arr) = o["allOf"].as_array() {
                if arr.len() == 1 {
                    let mut inlined = resolve_ref(arr[0].clone(), defs);
                    if let Some(inner) = inlined.as_object_mut() {
                        // Merge in default and description from the wrapper
                        if let Some(def) = o.get("default") {
                            inner.insert("default".to_string(), def.clone());
                        }
                        if let Some(desc) = o.get("description") {
                            inner.insert("description".to_string(), desc.clone());
                        }
                        inner.remove("title");
                    }
                    return inlined;
                }
            }
            v
        }
        // Flatten to just the inner type (nullable implied by being optional)
        Value::Object(o) if o.contains_key("anyOf") => {
            if let Some(arr) = o["anyOf"].as_array() {
                // Filter out the null variant; keep the non-null one
                let non_null: Vec<Value> = arr.iter()
                    .filter(|x| x.get("type").and_then(|t| t.as_str()) != Some("null"))
                    .map(|x| resolve_ref(x.clone(), defs))
                    .collect();
                if non_null.len() == 1 {
                    return non_null.into_iter().next().unwrap();
                }
            }
            v
        }
        _ => v,
    }
}

const TOOLS: &[ToolEntry] = &[
    ToolEntry {
        name: "memory_retrieve",
        description: "Query causal memory for the history most relevant to a situation. \
            Returns a ranked causal chain: which states led to which actions, \
            which actions caused which outcomes, and the confidence of each link. \
            Call this BEFORE making a decision.",
        schema: || schema_of::<RetrieveParams>(),
    },
    ToolEntry {
        name: "memory_learn",
        description: "Record a (state → action → outcome) experience into causal memory. \
            Call this AFTER an action's result is known. \
            prediction_error encodes how wrong the prediction was: \
            0.0 = outcome was exactly expected, 1.0 = complete surprise/failure.",
        schema: || schema_of::<LearnParams>(),
    },
    ToolEntry {
        name: "context_create",
        description: "Create an isolated memory context for a project, topic, or session. \
            Every context has its own completely separate causal graph — \
            nothing leaks between them. \
            Returns the context UUID for use in memory_learn / memory_retrieve.",
        schema: || schema_of::<CreateContextParams>(),
    },
    ToolEntry {
        name: "context_list",
        description: "List all memory contexts with their UUIDs, names, states, \
            learn counts, and last-access timestamps. \
            Use this to find a context UUID or check what memory exists.",
        schema: || schema_of::<ListContextsParams>(),
    },
    ToolEntry {
        name: "context_delete",
        description: "Permanently delete a context and all its causal memory. \
            IRREVERSIBLE — there is no undo. \
            The built-in default context cannot be deleted.",
        schema: || schema_of::<DeleteContextParams>(),
    },
    ToolEntry {
        name: "context_route",
        description: "Automatically route a query to the best-matching context(s) \
            using probabilistic scoring (keyword overlap + recency + frequency). \
            Returns contexts ranked by score [0–1]. \
            Call this first when you are unsure which context to use.",
        schema: || schema_of::<RouteParams>(),
    },
    ToolEntry {
        name: "memory_stats",
        description: "Get runtime statistics for a memory context: \
            node count, learn count, version, last-access time, \
            and whether it is currently loaded in RAM.",
        schema: || schema_of::<MemoryStatsParams>(),
    },
    ToolEntry {
        name: "memory_retrieve_text",
        description: "Retrieve causal memory using plain text — no embeddings needed. \
            PREFERRED over memory_retrieve for most agents. \
            Call this BEFORE making a decision to recall what was previously learned \
            about similar situations. The server converts text to an embedding internally.",
        schema: || schema_of::<RetrieveTextParams>(),
    },
    ToolEntry {
        name: "memory_learn_text",
        description: "Record a (state → action → outcome) experience as plain text — \
            no embeddings needed. PREFERRED over memory_learn for most agents. \
            Call this AFTER an action's result is known. \
            Automatically extracts routing keywords (auto_tag=true by default) to \
            improve future context_route accuracy.",
        schema: || schema_of::<LearnTextParams>(),
    },
    ToolEntry {
        name: "project_detect",
        description: "Detect the current project from the agent's working environment. \
            Returns project name, directory, git remote, and whether the directory \
            is a generic/default location. \
            Call this FIRST at session start to determine which memory context to use \
            without asking the user.",
        schema: || schema_of::<ProjectDetectParams>(),
    },
    ToolEntry {
        name: "memory_feedback",
        description: "Provide outcome feedback after acting on retrieved memory. \
            Call this after memory_retrieve_text returned a causal chain and you now know \
            whether following that advice led to success or failure. \
            Updates each node's usefulness posterior (Beta bandit) and forgetting half-life. \
            Nodes that help succeed get longer memory; nodes that mislead get shorter memory. \
            This is the core learning loop: retrieve → act → observe → feedback.",
        schema: || schema_of::<MemoryFeedbackParams>(),
    },
];

pub fn tool_definitions() -> Vec<ToolDefinition> {
    TOOLS.iter().map(|t| ToolDefinition {
        name:        t.name.to_string(),
        description: t.description.to_string(),
        inputSchema: (t.schema)(),
    }).collect()
}

// --- Tool handlers -----------------------------------------------------------

pub fn handle_retrieve(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: RetrieveParams = match serde_json::from_value(args.clone()) {
        Ok(p) => p,
        Err(e) => return error_result(format!("Invalid arguments: {e}")),
    };
    let emb = match decode_embedding(&params.query_embedding) {
        Ok(e) => e,
        Err(e) => return error_result(format!("Embedding decode error: {e}")),
    };

    let mut rt = rt.lock();
    let ctx_id = params.context_id.as_deref();

    match rt.retrieve(ctx_id, emb, &params.model_id) {
        Ok((result, ctx)) => {
            // Audit log: append one JSONL line per retrieve (labeled-pairs corpus)
            let node_ids: Vec<u64> = result.causal_chain.iter()
                .flat_map(|s| [s.from_node, s.to_node]).collect();
            append_retrieve_log(
                rt.data_dir(),
                &ctx.id.to_string(),
                &params.model_id,
                &params.query_embedding[..params.query_embedding.len().min(32)], // truncate b64
                &node_ids,
            );

            let mut text = format!(
                "## CMHM Causal Memory\n**Context**: {} ({})\n\n{}\n\n",
                ctx.display_name, ctx.id, result.explanation
            );
            if result.causal_chain.is_empty() {
                text.push_str("No relevant causal history found yet.\n");
            } else {
                text.push_str("### Chain Details\n");
                for (i, step) in result.causal_chain.iter().enumerate() {
                    text.push_str(&format!(
                        "{}. Node {} → Node {} | confidence: {:.1}% | evidence: {:.0} samples{}{}\n",
                        i + 1, step.from_node, step.to_node,
                        step.mean_weight * 100.0, step.evidence,
                        if step.is_intervention { " [agent intervention]" } else { "" },
                        if step.is_curious { " ⚠ uncertain" } else { "" },
                    ));
                }
                text.push_str(&format!(
                    "\n_Dense context vector available ({} dims)_",
                    result.dense_context.len()
                ));
            }
            ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
        }
        Err(e) => error_result(format!("Retrieve failed: {e}")),
    }
}

pub fn handle_learn(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: LearnParams = match serde_json::from_value(args.clone()) {
        Ok(p) => p,
        Err(e) => return error_result(format!("Invalid arguments: {e}")),
    };
    let state_emb   = match decode_embedding(&params.state_embedding)   { Ok(e) => e, Err(e) => return error_result(format!("state_embedding: {e}")) };
    let action_emb  = match decode_embedding(&params.action_embedding)  { Ok(e) => e, Err(e) => return error_result(format!("action_embedding: {e}")) };
    let outcome_emb = match decode_embedding(&params.outcome_embedding) { Ok(e) => e, Err(e) => return error_result(format!("outcome_embedding: {e}")) };

    if params.prediction_error < 0.0 || params.prediction_error > 1.0 {
        return error_result("prediction_error must be in [0.0, 1.0]");
    }

    let mut rt = rt.lock();
    let ctx_id = params.context_id.as_deref();

    match rt.learn(ctx_id, state_emb, action_emb, outcome_emb,
                   params.prediction_error, params.emotional_tag, &params.model_id) {
        Ok((result, ctx)) => {
            let outcome_label = if params.prediction_error <= 0.7 { "SUCCESS" } else { "FAILURE" };
            let text = format!(
                "## CMHM Learning Signal Processed\n\
                **Context**: {} ({})\n\
                **Outcome**: {} (e = {:.3})\n\n\
                Node IDs created/found:\n\
                - State:   {}\n\
                - Action:  {}\n\
                - Outcome: {}\n\
                - Context version: {}\n\n\
                The causal graph has been updated.",
                ctx.display_name, ctx.id,
                outcome_label, params.prediction_error,
                result.state_node_id, result.action_node_id, result.outcome_node_id,
                ctx.version,
            );
            ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
        }
        Err(e) => error_result(format!("Learn failed: {e}")),
    }
}

pub fn handle_context_create(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: CreateContextParams = match serde_json::from_value(args.clone()) {
        Ok(p)  => p,
        Err(e) => return error_result(format!("Invalid arguments: {e}")),
    };

    let ctx_type = ctx_type_from_param(&params.context_type);
    let parent_id = params.parent_id
        .map(|s| cmhm_core::ContextId::from_str(&s));

    let mut rt = rt.lock();
    match rt.create_context(
        params.display_name.clone(),
        ctx_type,
        parent_id,
        params.description,
        params.tags,
    ) {
        Ok(desc) => {
            let text = format!(
                "## Context Created\n\
                **Name**: {}\n\
                **ID**: {}\n\
                **Type**: {}\n\
                **Created at**: {}\n\n\
                Use `context_id: \"{}\"` in memory_learn and memory_retrieve to isolate memory to this context.",
                desc.display_name,
                desc.id,
                desc.context_type,
                desc.created_at,
                desc.id,
            );
            ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
        }
        Err(e) => error_result(format!("context_create failed: {e}")),
    }
}

pub fn handle_context_list(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: ListContextsParams = serde_json::from_value(args.clone())
        .unwrap_or(ListContextsParams { context_type: None });

    let rt = rt.lock();
    let mut contexts = rt.list_contexts();

    // Apply optional type filter
    if let Some(ref filter) = params.context_type {
        let target = ctx_type_from_param(filter);
        contexts.retain(|c| c.context_type == target);
    }

    if contexts.is_empty() {
        return ToolCallResult {
            content: vec![ContentItem::text("No contexts found.")],
            isError: None,
        };
    }

    let mut text = format!("## Memory Contexts ({} total)\n\n", contexts.len());
    text.push_str("| Name | ID | Type | State | Learns | Version | Last Access |\n");
    text.push_str("|---|---|---|---|---|---|---|\n");
    for c in &contexts {
        text.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} | {} |\n",
            c.display_name,
            c.id,
            c.context_type,
            c.state,
            c.learn_count,
            c.version,
            c.last_accessed,
        ));
    }
    text.push_str("\n_Use the ID in context_id field of memory_learn / memory_retrieve._");

    ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
}

pub fn handle_context_delete(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: DeleteContextParams = match serde_json::from_value(args.clone()) {
        Ok(p) => p,
        Err(e) => return error_result(format!("Invalid arguments: {e}")),
    };

    let id = cmhm_core::ContextId::from_str(&params.context_id);
    let mut rt = rt.lock();

    // Try UUID first, fallback to name lookup
    let resolved_id = if rt.get_descriptor(&id).is_some() {
        id
    } else {
        // Try by name: list and find
        let all = rt.list_contexts();
        match all.iter().find(|c| c.display_name.to_lowercase() == params.context_id.to_lowercase()) {
            Some(c) => c.id.clone(),
            None => return error_result(format!("E_NOT_FOUND: '{}' not found", params.context_id)),
        }
    };

    match rt.delete_context(&resolved_id) {
        Ok(()) => {
            let text = format!(
                "## Context Deleted\nContext `{}` and all its memory data have been permanently erased.\nThis action is irreversible.",
                resolved_id
            );
            ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
        }
        Err(e) => error_result(format!("context_delete failed: {e}")),
    }
}

pub fn handle_context_route(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: RouteParams = match serde_json::from_value(args.clone()) {
        Ok(p) => p,
        Err(e) => return error_result(format!("Invalid arguments: {e}")),
    };

    let rt = rt.lock();
    let results = rt.route_top_k(&params.query, params.top_k);

    if results.is_empty() {
        return ToolCallResult {
            content: vec![ContentItem::text("No contexts available for routing.")],
            isError: None,
        };
    }

    let mut text = format!(
        "## Context Routing\n**Query**: \"{}\"\n\n**Recommended contexts** (top {}):\n\n",
        params.query, params.top_k
    );
    for (i, (desc, score)) in results.iter().enumerate() {
        text.push_str(&format!(
            "{}. **{}** (score: {:.3})\n   ID: `{}`\n   Type: {} | State: {} | Learns: {}\n\n",
            i + 1,
            desc.display_name,
            score,
            desc.id,
            desc.context_type,
            desc.state,
            desc.learn_count,
        ));
    }
    text.push_str("_Use the ID in the context_id field of memory_retrieve._");

    ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
}

pub fn handle_memory_stats(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: MemoryStatsParams = serde_json::from_value(args.clone())
        .unwrap_or(MemoryStatsParams { context_id: None });

    let mut rt = rt.lock();
    match rt.stats(params.context_id.as_deref()) {
        Ok(stats) => {
            let d = &stats.descriptor;
            let text = format!(
                "## Memory Stats\n\
                **Context**: {} ({})\n\
                **Type**: {} | **State**: {}\n\
                **Version**: {} | **Learns**: {}\n\
                **Access count**: {} | **Last accessed**: {}\n\
                **Loaded in RAM**: {}\n\
                **Active contexts in RAM**: {}/{}\n\
                **Created at**: {}",
                d.display_name, d.id,
                d.context_type, d.state,
                d.version, d.learn_count,
                d.access_count, d.last_accessed,
                if stats.is_loaded { "yes" } else { "no (sleeping/idle)" },
                stats.active_count,
                stats.max_active_contexts,
                d.created_at,
            );
            ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
        }
        Err(e) => error_result(format!("memory_stats failed: {e}")),
    }
}

// --- Text tool handlers -------------------------------------------------------

/// Retrieve using plain text — converts to embedding internally.
pub fn handle_retrieve_text(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: RetrieveTextParams = match serde_json::from_value(args.clone()) {
        Ok(p)  => p,
        Err(e) => return error_result(format!("Invalid arguments: {e}")),
    };

    let emb = text_to_embedding(&params.query).to_vec();
    let mut rt = rt.lock();
    let ctx_id = params.context_id.as_deref();

    // TEXT_MODEL_ID = "hash-128d" — matches HashEmbedder::model_id() (N3)
    match rt.retrieve(ctx_id, emb, TEXT_MODEL_ID) {
        Ok((result, ctx)) => {
            // Audit log: append one JSONL line per retrieve (labeled-pairs corpus)
            let node_ids: Vec<u64> = result.causal_chain.iter()
                .flat_map(|s| [s.from_node, s.to_node]).collect();
            append_retrieve_log(
                rt.data_dir(),
                &ctx.id.to_string(),
                TEXT_MODEL_ID,
                &params.query,
                &node_ids,
            );

            let mut text = format!(
                "## CMHM Causal Memory (text query)\n\
                **Context**: {} ({})\n\
                **Query**: \"{}\"\n\n{}\n\n",
                ctx.display_name, ctx.id, params.query, result.explanation
            );
            if result.causal_chain.is_empty() {
                text.push_str("No relevant causal history found yet.\n");
            } else {
                text.push_str("### Chain Details\n");
                for (i, step) in result.causal_chain.iter().enumerate() {
                    text.push_str(&format!(
                        "{}. Node {} → Node {} | confidence: {:.1}% | evidence: {:.0} samples{}{}\n",
                        i + 1, step.from_node, step.to_node,
                        step.mean_weight * 100.0, step.evidence,
                        if step.is_intervention { " [agent intervention]" } else { "" },
                        if step.is_curious { " ⚠ uncertain" } else { "" },
                    ));
                }
            }
            ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
        }
        Err(e) => error_result(format!("Retrieve failed: {e}")),
    }
}

/// Learn from plain text — converts to embeddings internally, auto-tags context.
pub fn handle_learn_text(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: LearnTextParams = match serde_json::from_value(args.clone()) {
        Ok(p)  => p,
        Err(e) => return error_result(format!("Invalid arguments: {e}")),
    };

    if params.prediction_error < 0.0 || params.prediction_error > 1.0 {
        return error_result("prediction_error must be in [0.0, 1.0]");
    }

    // Scrub secrets from all three text fields BEFORE embedding.
    // Secrets must never enter the vector store, WAL, or snapshot.
    // Partial redaction: useful context is preserved, only suspicious tokens replaced.
    let (state_clean, action_clean, outcome_clean, redacted) =
        scrub_learn(&params.state, &params.action, &params.outcome);

    let state_emb   = text_to_embedding(&state_clean).to_vec();
    let action_emb  = text_to_embedding(&action_clean).to_vec();
    let outcome_emb = text_to_embedding(&outcome_clean).to_vec();

    // Capture fields before moving into the lock scope
    let auto_tag     = params.auto_tag;
    let ctx_id_owned = params.context_id.clone();
    let combined     = format!("{} {} {}", state_clean, action_clean, outcome_clean);

    let mut rt_lock = rt.lock();
    let ctx_id = ctx_id_owned.as_deref();

    // TEXT_MODEL_ID = "hash-128d" — matches HashEmbedder::model_id() (N3)
    match rt_lock.learn(ctx_id, state_emb, action_emb, outcome_emb,
                        params.prediction_error, params.emotional_tag, TEXT_MODEL_ID) {
        Ok((result, ctx)) => {
            // Auto-tag: extract keywords and add to context routing metadata
            if auto_tag {
                let tags = extract_tags(&combined, 10);
                if !tags.is_empty() {
                    // Best-effort — do not fail the learn if tagging fails
                    let _ = rt_lock.add_tags(ctx_id, tags);
                }
            }

            let outcome_label = if params.prediction_error <= 0.7 { "SUCCESS" } else { "FAILURE" };
            let redaction_note = if redacted > 0 {
                format!("\n**Security**: {redacted} sensitive token(s) redacted before storage.")
            } else {
                String::new()
            };
            let text = format!(
                "## Memory Learned (text)\n\
                **Context**: {} ({})\n\
                **Outcome**: {} (prediction_error: {:.2})\n\
                **Node IDs**: state={} action={} outcome={}\n\
                **Auto-tagged**: {}{}\n\n\
                State → Action → Outcome stored as causal graph nodes.\n\
                Use `memory_retrieve_text` with a similar query to recall this experience.",
                ctx.display_name, ctx.id,
                outcome_label, params.prediction_error,
                result.state_node_id, result.action_node_id, result.outcome_node_id,
                if auto_tag { "yes" } else { "no" },
                redaction_note,
            );
            ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
        }
        Err(e) => error_result(format!("Learn failed: {e}")),
    }
}

// Called by every handler that produces an error response.
// The lib target can't see through pub fn handler -> private fn, so suppress here.
#[allow(dead_code)]
fn error_result(msg: impl Into<String>) -> ToolCallResult {
    ToolCallResult {
        content: vec![ContentItem::text(msg.into())],
        isError: Some(true),
    }
}

// Called by create/list/delete/route handlers. Same visibility issue as error_result.
#[allow(dead_code)]
fn ctx_type_from_param(p: &ContextTypeParam) -> ContextType {
    match p {
        ContextTypeParam::Session   => ContextType::Session,
        ContextTypeParam::Project   => ContextType::Project,
        ContextTypeParam::Branch    => ContextType::Branch,
        ContextTypeParam::Workspace => ContextType::Workspace,
        ContextTypeParam::Global    => ContextType::Global,
    }
}

// --- project_detect ----------------------------------------------------------

pub fn handle_project_detect(_rt: &SharedRuntime, _args: &Value) -> ToolCallResult {
    let cwd = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));

    let dir_name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Detect generic/home directories — agent should use default context for these
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let is_default_dir = cwd == std::path::Path::new(&home)
        || dir_name == "home"
        || dir_name == "root"
        || dir_name == "tmp"
        || dir_name == "temp"
        || dir_name == "/"
        || cwd.as_os_str().to_string_lossy().len() < 4;

    // Try to read a project manifest for a canonical name
    let manifest_name = detect_manifest_name(&cwd);
    let project_name = manifest_name.clone().unwrap_or_else(|| dir_name.clone());

    // Try to get git remote origin URL
    let git_remote = detect_git_remote(&cwd);

    // Try to get current git branch
    let git_branch = detect_git_branch(&cwd);

    let recommendation = if is_default_dir {
        "Use the `default` context — this is a generic directory, not a project.".to_string()
    } else {
        format!(
            "Call `context_route` with query \"{project_name}\". \
            If top score < 0.5, call `context_create` with display_name=\"{project_name}\". \
            Then use that context for all memory_learn / memory_retrieve calls this session."
        )
    };

    let text = format!(
        "## Project Detection\n\
        **Working directory**: {}\n\
        **Directory name**: {}\n\
        **Project name**: {} {}\n\
        **Git remote**: {}\n\
        **Git branch**: {}\n\
        **Is generic/home dir**: {}\n\n\
        **Recommended action**: {}",
        cwd.display(),
        dir_name,
        project_name,
        if manifest_name.is_some() { "(from manifest)" } else { "(from directory name)" },
        git_remote.as_deref().unwrap_or("not a git repo"),
        git_branch.as_deref().unwrap_or("unknown"),
        if is_default_dir { "yes" } else { "no" },
        recommendation,
    );

    ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
}

/// Read project name from the first manifest found in `dir`.
// Called only from handle_project_detect. The lib target can't trace
// through pub fn boundaries, so targeted #[allow] instead of crate-wide.
#[allow(dead_code)]
fn detect_manifest_name(dir: &std::path::Path) -> Option<String> {
    // Cargo.toml → [package] name
    let cargo = dir.join("Cargo.toml");
    if cargo.exists() {
        if let Ok(src) = std::fs::read_to_string(&cargo) {
            for line in src.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("name") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let name = rest.trim().trim_matches('"').trim_matches('\'');
                    if !name.is_empty() { return Some(name.to_string()); }
                }
            }
        }
    }

    // package.json → "name"
    let pkg = dir.join("package.json");
    if pkg.exists() {
        if let Ok(src) = std::fs::read_to_string(&pkg) {
            if let Ok(v) = serde_json::from_str::<Value>(&src) {
                if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
                    return Some(name.to_string());
                }
            }
        }
    }

    // pyproject.toml → [tool.poetry] name or [project] name
    let pyproject = dir.join("pyproject.toml");
    if pyproject.exists() {
        if let Ok(src) = std::fs::read_to_string(&pyproject) {
            for line in src.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("name") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let name = rest.trim().trim_matches('"').trim_matches('\'');
                    if !name.is_empty() { return Some(name.to_string()); }
                }
            }
        }
    }

    None
}

/// Get the git remote origin URL via `git remote get-url origin`.
#[allow(dead_code)]
fn detect_git_remote(dir: &std::path::Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Get current git branch via `git branch --show-current`.
#[allow(dead_code)]
fn detect_git_branch(dir: &std::path::Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

// --- memory_feedback ---------------------------------------------------------

pub fn handle_memory_feedback(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: MemoryFeedbackParams = match serde_json::from_value(args.clone()) {
        Ok(p)  => p,
        Err(e) => return error_result(format!("Invalid arguments: {e}")),
    };

    if params.node_ids.is_empty() {
        return error_result("node_ids must not be empty");
    }

    let outcome_str = params.outcome.trim().to_lowercase();
    let success = match outcome_str.as_str() {
        "success" | "ok" | "true" | "1" => true,
        "failure" | "fail" | "false" | "0" => false,
        other => return error_result(format!(
            "outcome must be 'success' or 'failure', got: {other:?}"
        )),
    };

    let ctx_id_owned = params.context_id.clone();
    let mut rt_lock = rt.lock();
    let ctx_id = ctx_id_owned.as_deref();

    match rt_lock.feedback_nodes(ctx_id, params.node_ids.clone(), success) {
        Ok((updated, ctx)) => {
            let outcome_label = if success { "SUCCESS" } else { "FAILURE" };
            let text = format!(
                "## Memory Feedback Applied\n\
                **Context**: {} ({})\n\
                **Outcome**: {}\n\
                **Nodes requested**: {}\n\
                **Nodes updated**: {}\n\n\
                Each updated node's usefulness prior (Beta posterior) and forgetting \
                half-life have been adjusted. Future retrievals will rank these nodes \
                {} when similar situations arise.",
                ctx.display_name, ctx.id,
                outcome_label,
                params.node_ids.len(),
                updated,
                if success { "higher" } else { "lower" },
            );
            ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
        }
        Err(e) => error_result(format!("Feedback failed: {e}")),
    }
}
