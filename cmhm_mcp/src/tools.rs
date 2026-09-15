use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::{json, Value};
use std::sync::Arc;
use parking_lot::Mutex;
use uuid::Uuid;
use cmhm_core::{ContextRuntime, ContextType, text_to_embedding, extract_tags, scrub::scrub_learn, node::NodeType};
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
pub const TEXT_MODEL_ID: &str = "hash-128d";

/// Shadow gate policy version — bump when the gate formula changes.
/// All gate_shadow_log.jsonl entries are stamped with this so decisions
/// can be compared across formula versions during retrospective analysis.
pub const GATE_POLICY_V: u32 = 2;

/// Shadow gate κ (kappa): weight on prediction_error in the gate score formula.
/// gate_score = novelty × (1 + κ × |prediction_error|) × verifiability
/// κ=2.0 makes a fully-surprising event (pe=1.0) worth 3× a neutral event (pe=0.0).
pub const GATE_KAPPA: f32 = 2.0;

/// Default shadow gate threshold for "would_index" decision when not configured.
/// Calibrated on hash-128d to separate near-duplicate clutter from informative experiences.
pub const GATE_THRESHOLD: f32 = 0.35;

// ─── Sidecar log writers ─────────────────────────────────────────────────────
//
// All three log files are:
//   - Append-only JSONL, never truncated — safe to tail / import incrementally.
//   - Separate from the memory store — log bugs never affect memory operations.
//   - Best-effort: file write failures emit tracing::warn! but never fail callers.
//
// Files:
//   learn_log.jsonl      — Tier 0 raw learn archive (every attempt, pre-gate)
//   retrieve_log.jsonl   — retrieve audit log with retrieve_id and ranked scores
//   gate_shadow_log.jsonl — shadow gate: "would index" decisions without enforcement

/// Append one line to `retrieve_log.jsonl`.
///
/// Schema v=2:
/// ```json
/// { "v":2, "retrieve_id":"<uuid>", "session_id":"<uuid>", "ts":<unix_ms>,
///   "ctx_id":"<uuid>", "model":"<model_id>", "query":"<text or b64-prefix>",
///   "shown":[{"fn":<from_node>,"tn":<to_node>,"score":<f32>},...], "n":<count> }
/// ```
#[allow(clippy::too_many_arguments)]
fn append_retrieve_log(
    data_dir:    &std::path::Path,
    retrieve_id: &str,
    session_id:  &str,
    ctx_id:      &str,
    model_id:    &str,
    query_repr:  &str,
    chain:       &[(u64, u64, f32)], // (from_node, to_node, mean_weight)
) {
    let path = data_dir.join("retrieve_log.jsonl");
    let ts = unix_ms();
    // Build shown array: [{"fn":X,"tn":Y,"score":Z}]
    let shown: String = chain.iter()
        .map(|(f, t, s)| format!("{{\"fn\":{},\"tn\":{},\"score\":{:.4}}}", f, t, s))
        .collect::<Vec<_>>()
        .join(",");
    let query_json = serde_json::to_string(query_repr).unwrap_or_else(|_| "\"\"".to_string());
    let line = format!(
        "{{\"v\":2,\"retrieve_id\":\"{}\",\"session_id\":\"{}\",\"ts\":{},\"ctx_id\":\"{}\",\
         \"model\":\"{}\",\"query\":{},\"shown\":[{}],\"n\":{}}}\n",
        retrieve_id, session_id, ts, ctx_id, model_id, query_json, shown, chain.len()
    );
    best_effort_append(&path, &line, "retrieve_log");
}

/// Append one line to `learn_log.jsonl` — the Tier 0 raw learn archive.
///
/// Called BEFORE the gate and BEFORE engine write, with pre-scrubbed text.
/// Secrets MUST be scrubbed by caller before passing state/action/outcome here.
///
/// Schema v=1:
/// ```json
/// { "v":1, "event_id":"<uuid>", "session_id":"<uuid>", "ctx_id":"<uuid>",
///   "ts":<unix_ms>, "policy_v":1, "kind":"learn_text",
///   "state":"<scrubbed>", "action":"<scrubbed>", "outcome":"<scrubbed>",
///   "prediction_error":<f32>, "emotional_tag":<f32|null>, "tags":["<str>",...] }
/// ```
#[allow(clippy::too_many_arguments)]
fn append_learn_archive(
    data_dir:         &std::path::Path,
    event_id:         &str,
    session_id:       &str,
    ctx_id:           &str,
    kind:             &str,           // "learn_text" | "learn_emb"
    state:            &str,           // pre-scrubbed
    action:           &str,           // pre-scrubbed
    outcome:          &str,           // pre-scrubbed
    prediction_error: f32,
    emotional_tag:    Option<f32>,
    tags:             &[String],
) {
    let path = data_dir.join("learn_log.jsonl");
    let ts = unix_ms();
    let eta_json = match emotional_tag {
        Some(v) => format!("{:.4}", v),
        None    => "null".to_string(),
    };
    let tags_json: String = tags.iter()
        .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "\"\"".to_string()))
        .collect::<Vec<_>>()
        .join(",");
    let state_json  = serde_json::to_string(state).unwrap_or_else(|_| "\"\"".to_string());
    let action_json = serde_json::to_string(action).unwrap_or_else(|_| "\"\"".to_string());
    let outcome_json= serde_json::to_string(outcome).unwrap_or_else(|_| "\"\"".to_string());
    let line = format!(
        "{{\"v\":1,\"event_id\":\"{}\",\"session_id\":\"{}\",\"ctx_id\":\"{}\",\
         \"ts\":{},\"policy_v\":1,\"kind\":\"{}\",\
         \"state\":{},\"action\":{},\"outcome\":{},\
         \"prediction_error\":{:.4},\"emotional_tag\":{},\"tags\":[{}]}}\n",
        event_id, session_id, ctx_id,
        ts, kind,
        state_json, action_json, outcome_json,
        prediction_error, eta_json, tags_json
    );
    best_effort_append(&path, &line, "learn_log");
}

/// Compute LFA verifiability V ∈ (0.0, 1.0] for ingestion gating.
///
/// Combines empirical evidence (Wilson score Lower Confidence Bound over nearest node's
/// feedback track record) with caller-authorized source trust:
/// - If empirical feedback is available (N = successes + failures > 0):
///   computes Wilson LCB at 95% confidence on the observed success rate, blended with
///   the authorized source trust via sample-size shrinkage:
///     w = N / (N + 4.0)
///     V = (1 - w) * authority + w * LCB
/// - If no empirical feedback is available (virgin state region):
///   V = max(0.1, authority).
pub fn compute_lfa_verifiability(
    nearest_bandit: Option<(f32, f32)>,
    authority: f32,
) -> f32 {
    let base_auth = authority.clamp(0.0, 1.0);
    if let Some((alpha, beta)) = nearest_bandit {
        let s = (alpha - 1.0).max(0.0) as f64;
        let f = (beta - 1.0).max(0.0) as f64;
        let n = s + f;
        if n > 0.0 {
            let (lcb, _) = cmhm_core::consolidation::wilson_ci(s, n);
            let w = (n / (n + 4.0)) as f32;
            return ((1.0 - w) * base_auth + w * (lcb as f32)).clamp(0.01, 1.0);
        }
    }
    base_auth.clamp(0.1, 1.0)
}

/// Append one line to `gate_shadow_log.jsonl`.
///
/// Logs the ingestion gate's per-event decision: all components, the score,
/// and whether it would be indexed.
/// In shadow mode (enforce=false), insertion always proceeds regardless.
///
/// Schema v=2:
/// ```json
/// { "v":2, "event_id":"<uuid>", "ts":<unix_ms>, "ctx_id":"<uuid>",
///   "novelty":<f32>, "prediction_error":<f32>, "verifiability":<f32>,
///   "gate_score":<f32>, "threshold":<f32>, "would_index":<bool>,
///   "policy_v":2, "enforced":<bool> }
/// ```
#[allow(clippy::too_many_arguments)]
fn append_gate_shadow_log(
    data_dir:         &std::path::Path,
    event_id:         &str,
    ctx_id:           &str,
    novelty:          f32,
    prediction_error: f32,
    verifiability:    f32,
    gate_score:       f32,
    threshold:        f32,
    enforced:         bool,
) {
    let path = data_dir.join("gate_shadow_log.jsonl");
    let ts = unix_ms();
    let would_index = gate_score >= threshold;
    let line = format!(
        "{{\"v\":2,\"event_id\":\"{}\",\"ts\":{},\"ctx_id\":\"{}\",\
         \"novelty\":{:.4},\"prediction_error\":{:.4},\"verifiability\":{:.4},\
         \"gate_score\":{:.4},\"threshold\":{:.4},\"would_index\":{},\"policy_v\":{},\"enforced\":{}}}\n",
        event_id, ts, ctx_id,
        novelty, prediction_error, verifiability,
        gate_score, threshold, would_index, GATE_POLICY_V, enforced
    );
    best_effort_append(&path, &line, "gate_shadow_log");
}

/// Append `line` to `path`. Best-effort: log a warning on failure, never panic.
fn best_effort_append(path: &std::path::Path, line: &str, log_name: &str) {
    use std::io::Write;
    match std::fs::OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => {
            if f.write_all(line.as_bytes()).is_err() {
                tracing::warn!("best_effort_append: write failed for {}", log_name);
            }
        }
        Err(e) => {
            tracing::warn!("best_effort_append: open failed for {}: {}", log_name, e);
        }
    }
}

/// Current time as Unix milliseconds.
fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
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

/// Known provenance classes accepted by the authority clamping policy.
/// Unrecognized classes are normalized to "" at the MCP ingestion boundary.
const KNOWN_PROVENANCE_CLASSES: &[&str] = &[
    "", "inferred", "external_feed", "user_explicit", "corrected", "contradicted",
];

/// Convert a `ConflictType` to its machine-readable string representation.
fn conflict_type_label(ct: &cmhm_core::ConflictType) -> &'static str {
    match ct {
        cmhm_core::ConflictType::LogicalContradiction  => "logical_contradiction",
        cmhm_core::ConflictType::ScoreAmbiguity       => "score_ambiguity",
        cmhm_core::ConflictType::AuthorityPolarity    => "authority_polarity",
        cmhm_core::ConflictType::SupersessionConflict => "supersession_conflict",
    }
}

pub fn handle_retrieve(rt: &SharedRuntime, args: &Value) -> ToolCallResult {
    let params: RetrieveParams = match serde_json::from_value(args.clone()) {
        Ok(p) => p,
        Err(e) => return error_result(format!("Invalid arguments: {e}")),
    };
    let emb = match decode_embedding(&params.query_embedding) {
        Ok(e) => e,
        Err(e) => return error_result(format!("Embedding decode error: {e}")),
    };

    // Parse and require retrieval_intent — no unscoped retrieval from MCP.
    let seed_type = match cmhm_core::node::NodeType::from_str(&params.retrieval_intent) {
        Some(t) => t,
        None => return error_result(format!(
            "Invalid retrieval_intent {:?}. Must be one of: \"state\", \"action\", \"outcome\".",
            params.retrieval_intent
        )),
    };

    // Generate retrieve_id before locking — UUID v4, unique per call
    let retrieve_id = Uuid::new_v4().to_string();

    let mut rt = rt.lock();
    let ctx_id = params.context_id.as_deref();
    let session_id = rt.session_id().to_string();

    match rt.retrieve(ctx_id, emb, &params.model_id, seed_type) {
        Ok((result, ctx)) => {
            // Build chain tuples (from_node, to_node, mean_weight) for log + text
            let chain: Vec<(u64, u64, f32)> = result.causal_chain.iter()
                .map(|s| (s.from_node, s.to_node, s.mean_weight))
                .collect();

            // Log v=2: retrieve_id + session_id + ranked scores
            append_retrieve_log(
                rt.data_dir(),
                &retrieve_id,
                &session_id,
                &ctx.id.to_string(),
                &params.model_id,
                "[vector-query]",
                &chain,
            );

            let mut text = format!(
                "## CMHM Causal Memory\n**Context**: {} ({})\n\n{}\n\n",
                ctx.display_name, ctx.id, result.explanation
            );
            if result.abstained {
                text.push_str(
                    "> ⛔ **Retrieval abstained** — conflicting memories detected and \
                     `abstain_on_conflict` is enabled. No result returned. \
                     Review `conflict_candidates` and resolve with `supersedes`.\n\n"
                );
            } else if result.conflict_detected {
                let ctype = result.conflict_candidates.first()
                    .map(|c| conflict_type_label(&c.conflict_type))
                    .unwrap_or("unknown");
                let candidates_summary: Vec<String> = result.conflict_candidates.iter()
                    .map(|c| format!(
                        "node {} (score={:.3}, authority={:.2}, provenance={:?}, type={})",
                        c.node_id, c.score, c.authority, c.provenance,
                        conflict_type_label(&c.conflict_type)
                    ))
                    .collect();
                text.push_str(&format!(
                    "> ⚠ **Conflict detected** [{}] — top-2 candidates had near-equal scores. \
                     Higher-scored candidate selected. \
                     Candidates: {}. Use `supersedes` on next learn to resolve.\n\n",
                    ctype, candidates_summary.join("; ")
                ));
            }

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
            // retrieve_id: copy this into memory_feedback.retrieve_id to enable join + idempotency
            text.push_str(&format!("\n\n**retrieve_id**: `{retrieve_id}`"));

            // Machine-readable JSON block (content[1]) — structured fields for tests and agents.
            let structured = json!({
                "retrieve_id":       retrieve_id,
                "seed_id":           result.seed_id,
                "seed_authority":    result.seed_authority,
                "seed_provenance":   result.seed_provenance,
                "conflict_detected": result.conflict_detected,
                "abstained":         result.abstained,
                "conflict_candidates": result.conflict_candidates.iter().map(|c| json!({
                    "node_id":      c.node_id,
                    "score":        c.score,
                    "authority":    c.authority,
                    "provenance":   c.provenance,
                    "conflict_type": conflict_type_label(&c.conflict_type),
                })).collect::<Vec<_>>(),
                "causal_chain_len":  result.causal_chain.len(),
                "raw_nn_candidates":      result.raw_nn_candidates,
                "post_filter_candidates": result.post_filter_candidates,
                "scored_candidates":      result.scored_candidates.iter().map(|(id, score)| json!({
                    "node_id": id,
                    "score":   score,
                })).collect::<Vec<_>>(),
            });
            ToolCallResult {
                content: vec![ContentItem::text(text), ContentItem::json(&structured)],
                isError: None,
            }

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

    // Reject non-finite values at the MCP boundary.
    if !params.authority.is_finite() {
        return error_result("authority must be a finite number (not NaN or Inf)");
    }
    if !params.prediction_error.is_finite() {
        return error_result("prediction_error must be a finite number (not NaN or Inf)");
    }
    if params.prediction_error < 0.0 || params.prediction_error > 1.0 {
        return error_result("prediction_error must be in [0.0, 1.0]");
    }
    if state_emb.iter().any(|v| !v.is_finite()) {
        return error_result("state_embedding contains non-finite values (NaN or Inf)");
    }
    if action_emb.iter().any(|v| !v.is_finite()) {
        return error_result("action_embedding contains non-finite values (NaN or Inf)");
    }
    if outcome_emb.iter().any(|v| !v.is_finite()) {
        return error_result("outcome_embedding contains non-finite values (NaN or Inf)");
    }

    let mut rt = rt.lock();
    let ctx_id = params.context_id.as_deref();
    let session_id = rt.session_id().to_string();
    let data_dir = rt.data_dir().to_path_buf();

    // Resolve ctx_id string for logging (best-effort — don't fail learn if this fails)
    let ctx_id_str = rt.list_contexts().iter()
        .find(|c| ctx_id.map_or(c.display_name == "default", |id| c.id.to_string() == id || c.display_name == id))
        .map(|c| c.id.to_string())
        .unwrap_or_else(|| ctx_id.unwrap_or("default").to_string());

    let event_id = Uuid::new_v4().to_string();

    // Tier 0 learn archive: archive BEFORE gate and BEFORE engine write
    append_learn_archive(
        &data_dir, &event_id, &session_id, &ctx_id_str,
        "learn",
        "<raw vector>", "<raw vector>", "<raw vector>",
        params.prediction_error, Some(params.emotional_tag), &[],
    );

    // Ingestion Gate (Policy v2: Wilson LCB verifiability + configurable enforcement)
    let inspection = rt.inspect_gate_in_context(ctx_id, &state_emb, &params.model_id, NodeType::State);
    let max_cos = inspection.max_cosine;
    let novelty = (1.0_f32 - max_cos).clamp(0.0, 2.0);
    let verifiability = compute_lfa_verifiability(inspection.nearest_bandit, params.authority);
    let kappa = rt.config().gate_kappa;
    let threshold = rt.config().gate_threshold;
    let enforce = rt.config().enforce_gate;
    let gate_score = novelty * (1.0 + kappa * params.prediction_error.abs()) * verifiability;
    let would_index = gate_score >= threshold;

    append_gate_shadow_log(
        &data_dir, &event_id, &ctx_id_str,
        novelty, params.prediction_error, verifiability,
        gate_score, threshold, enforce,
    );

    if enforce && !would_index {
        let text = format!(
            "## CMHM Learning Signal Gated (Suppressed)\n\
            **Context**: {} ({})\n\
            **Outcome**: SUPPRESSED (e = {:.3})\n\
            **Gate (enforced)**: novelty={:.3} verifiability={:.3} score={:.3} threshold={:.3} would_index=false\n\n\
            Experience suppressed by admission gate to prevent redundant node clutter.\n\
            Raw event was durably archived to Tier 0 learn_log.",
            ctx_id_str, ctx_id_str,
            params.prediction_error,
            novelty, verifiability, gate_score, threshold,
        );
        return ToolCallResult { content: vec![ContentItem::text(text)], isError: None };
    }

    // Normalize provenance at the MCP ingestion boundary (same policy as handle_learn_text).
    let provenance = if KNOWN_PROVENANCE_CLASSES.contains(&params.provenance.as_str()) {
        params.provenance
    } else {
        tracing::warn!(
            "handle_learn: unknown provenance class {:?} normalized to \"\" (neutral)",
            params.provenance
        );
        String::new()
    };

    // Security boundary: External MCP requests possess no authenticated session identity.
    // All external MCP invocations are unconditionally classified as CallerRole::Agent.
    // Privileged roles (CallerRole::User, CallerRole::System) are only reachable via internal
    // trusted Rust runtime APIs (ContextRuntime::learn_with_context).
    let caller_role = cmhm_core::CallerRole::Agent;

    match rt.learn_with_context(

        ctx_id, state_emb, action_emb, outcome_emb,
        params.prediction_error, params.emotional_tag, &params.model_id,
        params.authority, provenance, params.supersedes,
        caller_role, params.assertion_key, params.polarity,
    ) {

        Ok((result, ctx)) => {
            let outcome_label = if params.prediction_error <= 0.7 { "SUCCESS" } else { "FAILURE" };
            let gate_label = if enforce { "Gate (enforced)" } else { "Gate (shadow)" };
            let text = format!(
                "## CMHM Learning Signal Processed\n\
                **Context**: {} ({})\n\
                **Outcome**: {} (e = {:.3})\n\
                **{gate_label}**: novelty={:.3} score={:.3} would_index={}\n\n\
                Node IDs created/found:\n\
                - State:   {}\n\
                - Action:  {}\n\
                - Outcome: {}\n\
                - Context version: {}\n\n\
                The causal graph has been updated.",
                ctx.display_name, ctx.id,
                outcome_label, params.prediction_error,
                novelty, gate_score, would_index,
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

    // Parse retrieval_intent:
    //   None/absent → seed_type = None  (unscoped: all node types compete — required for
    //                                    cross-type contradiction safety evaluation).
    //   Some(s) and parseable → seed_type = Some(t)  (type-restricted pool).
    //   Some(s) and unparseable → fail closed.
    let seed_type: Option<cmhm_core::node::NodeType> = match &params.retrieval_intent {
        None => None,
        Some(s) => match cmhm_core::node::NodeType::from_str(s) {
            Some(t) => Some(t),
            None => return error_result(format!(
                "Invalid retrieval_intent {:?}. Must be one of: \"state\", \"action\", \"outcome\", or omitted for unscoped retrieval.",
                s
            )),
        },
    };

    // Generate retrieve_id before locking — UUID v4, unique per call
    let retrieve_id = Uuid::new_v4().to_string();

    let mut rt = rt.lock();
    let ctx_id = params.context_id.as_deref();
    let session_id = rt.session_id().to_string();

    // TEXT_MODEL_ID = "hash-128d" — matches HashEmbedder::model_id() (N3)
    // Dispatch: unscoped when retrieval_intent absent, scoped when present.
    let retrieve_result = match seed_type {
        None    => rt.retrieve_unscoped(ctx_id, emb, TEXT_MODEL_ID),
        Some(t) => rt.retrieve(ctx_id, emb, TEXT_MODEL_ID, t),
    };
    match retrieve_result {
        Ok((result, ctx)) => {
            // Build chain tuples (from_node, to_node, mean_weight) for log + text
            let chain: Vec<(u64, u64, f32)> = result.causal_chain.iter()
                .map(|s| (s.from_node, s.to_node, s.mean_weight))
                .collect();

            // Log v=2: retrieve_id + session_id + ranked scores
            append_retrieve_log(
                rt.data_dir(),
                &retrieve_id,
                &session_id,
                &ctx.id.to_string(),
                TEXT_MODEL_ID,
                &params.query,
                &chain,
            );

            let mut text = format!(
                "## CMHM Causal Memory (text query)\n\
                **Context**: {} ({})\n\
                **Query**: \"{}\"\n\n{}\n\n",
                ctx.display_name, ctx.id, params.query, result.explanation
            );
            if result.abstained {
                text.push_str(
                    "> ⛔ **Retrieval abstained** — conflicting memories detected and \
                     `abstain_on_conflict` is enabled. No result returned. \
                     Review `conflict_candidates` and resolve with `supersedes`.\n\n"
                );
            } else if result.conflict_detected {
                let ctype = result.conflict_candidates.first()
                    .map(|c| conflict_type_label(&c.conflict_type))
                    .unwrap_or("unknown");
                let candidates_summary: Vec<String> = result.conflict_candidates.iter()
                    .map(|c| format!(
                        "node {} (score={:.3}, authority={:.2}, provenance={:?}, type={})",
                        c.node_id, c.score, c.authority, c.provenance,
                        conflict_type_label(&c.conflict_type)
                    ))
                    .collect();
                text.push_str(&format!(
                    "> ⚠ **Conflict detected** [{}] — top-2 candidates had near-equal scores. \
                     Higher-scored candidate selected. \
                     Candidates: {}. Use `supersedes` on next learn to resolve.\n\n",
                    ctype, candidates_summary.join("; ")
                ));
            }

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
            // retrieve_id: copy into memory_feedback.retrieve_id to enable join + idempotency
            text.push_str(&format!("\n\n**retrieve_id**: `{retrieve_id}`"));

            // Machine-readable JSON block (content[1])
            let structured = json!({
                "retrieve_id":       retrieve_id,
                "seed_id":           result.seed_id,
                "seed_authority":    result.seed_authority,
                "seed_provenance":   result.seed_provenance,
                "conflict_detected": result.conflict_detected,
                "abstained":         result.abstained,
                "conflict_candidates": result.conflict_candidates.iter().map(|c| json!({
                    "node_id":      c.node_id,
                    "score":        c.score,
                    "authority":    c.authority,
                    "provenance":   c.provenance,
                    "conflict_type": conflict_type_label(&c.conflict_type),
                })).collect::<Vec<_>>(),
                "causal_chain_len":  result.causal_chain.len(),
                "raw_nn_candidates":      result.raw_nn_candidates,
                "post_filter_candidates": result.post_filter_candidates,
                "scored_candidates":      result.scored_candidates.iter().map(|(id, score)| json!({
                    "node_id": id,
                    "score":   score,
                })).collect::<Vec<_>>(),
            });
            ToolCallResult {
                content: vec![ContentItem::text(text), ContentItem::json(&structured)],
                isError: None,
            }
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

    // Reject non-finite values at the MCP boundary.
    if !params.authority.is_finite() {
        return error_result("authority must be a finite number (not NaN or Inf)");
    }
    if !params.prediction_error.is_finite() {
        return error_result("prediction_error must be a finite number (not NaN or Inf)");
    }
    if params.prediction_error < 0.0 || params.prediction_error > 1.0 {
        return error_result("prediction_error must be in [0.0, 1.0]");
    }

    // Scrub secrets from all three text fields BEFORE embedding AND before archiving.
    // Secrets must never enter the vector store, WAL, snapshot, OR the Tier 0 archive.
    // Partial redaction: useful context is preserved, only suspicious tokens replaced.
    let (state_clean, action_clean, outcome_clean, redacted) =
        scrub_learn(&params.state, &params.action, &params.outcome);

    // Generate a stable event_id linking learn_log + gate_shadow_log entries.
    let event_id = Uuid::new_v4().to_string();

    let state_emb   = text_to_embedding(&state_clean).to_vec();
    let action_emb  = text_to_embedding(&action_clean).to_vec();
    let outcome_emb = text_to_embedding(&outcome_clean).to_vec();

    // Capture fields before locking
    let auto_tag     = params.auto_tag;
    let ctx_id_owned = params.context_id.clone();
    let combined     = format!("{} {} {}", state_clean, action_clean, outcome_clean);
    let pe           = params.prediction_error;
    let eta          = params.emotional_tag;

    let mut rt_lock = rt.lock();
    let ctx_id      = ctx_id_owned.as_deref();
    let session_id  = rt_lock.session_id().to_string();
    let data_dir    = rt_lock.data_dir().to_path_buf();

    // Resolve ctx_id string for logging (best-effort — don't fail learn if this fails)
    let ctx_id_str = rt_lock.list_contexts().iter()
        .find(|c| ctx_id.map_or(c.display_name == "default", |id| c.id.to_string() == id || c.display_name == id))
        .map(|c| c.id.to_string())
        .unwrap_or_else(|| ctx_id.unwrap_or("default").to_string());

    // ── Fix 1: Tier 0 learn archive ──────────────────────────────────────────
    // Archive BEFORE gate and BEFORE engine write.
    // Secrets already scrubbed above. Failure here emits warn! but does not stop learn.
    let tags_for_archive: Vec<String> = if auto_tag {
        extract_tags(&combined, 10)
    } else {
        vec![]
    };
    append_learn_archive(
        &data_dir, &event_id, &session_id, &ctx_id_str,
        "learn_text",
        &state_clean, &action_clean, &outcome_clean,
        pe, Some(eta), &tags_for_archive,
    );

    // ── Fix 3: Ingestion Gate (Policy v2: Wilson LCB verifiability + configurable enforcement) ──
    let inspection = rt_lock.inspect_gate_in_context(ctx_id, &state_emb, TEXT_MODEL_ID, NodeType::State);
    let max_cos  = inspection.max_cosine;
    let novelty  = (1.0_f32 - max_cos).clamp(0.0, 2.0);
    let verifiability = compute_lfa_verifiability(inspection.nearest_bandit, params.authority);
    let kappa = rt_lock.config().gate_kappa;
    let threshold = rt_lock.config().gate_threshold;
    let enforce = rt_lock.config().enforce_gate;
    let gate_score = novelty * (1.0 + kappa * pe.abs()) * verifiability;
    let would_index = gate_score >= threshold;

    append_gate_shadow_log(
        &data_dir, &event_id, &ctx_id_str,
        novelty, pe, verifiability, gate_score, threshold, enforce,
    );

    if enforce && !would_index {
        let text = format!(
            "## Memory Ingestion Gated (Suppressed)\n\
            **Context**: {} ({})\n\
            **Outcome**: SUPPRESSED (prediction_error: {:.2})\n\
            **Gate (enforced)**: novelty={:.3} verifiability={:.3} score={:.3} threshold={:.3} would_index=false\n\n\
            Experience suppressed by admission gate to prevent redundant node clutter.\n\
            Raw event was durably archived to Tier 0 learn_log.",
            ctx_id_str, ctx_id_str,
            pe,
            novelty, verifiability, gate_score, threshold,
        );
        return ToolCallResult { content: vec![ContentItem::text(text)], isError: None };
    }

    // Normalize provenance at the MCP ingestion boundary.
    // Only recognized classes pass through; unknown strings are demoted to "" (neutral)
    // with a warning so the authority clamping policy can enforce the correct ceiling.
    // This does NOT authenticate the caller — it narrows the attack surface at the public API.
    let provenance = if KNOWN_PROVENANCE_CLASSES.contains(&params.provenance.as_str()) {
        params.provenance.clone()
    } else {
        tracing::warn!(
            "handle_learn_text: unknown provenance class {:?} normalized to \"\" (neutral)",
            params.provenance
        );
        String::new()
    };

    // Security boundary: External MCP requests possess no authenticated session identity.
    // All external MCP invocations are unconditionally classified as CallerRole::Agent.
    // Privileged roles (CallerRole::User, CallerRole::System) are only reachable via internal
    // trusted Rust runtime APIs (ContextRuntime::learn_with_context).
    let caller_role = cmhm_core::CallerRole::Agent;

    // TEXT_MODEL_ID = "hash-128d" — matches HashEmbedder::model_id() (N3)

    match rt_lock.learn_with_context(
        ctx_id, state_emb, action_emb, outcome_emb, pe, eta, TEXT_MODEL_ID,
        params.authority, provenance, params.supersedes,
        caller_role, params.assertion_key, params.polarity,
    ) {



        Ok((result, ctx)) => {
            // Auto-tag: extract keywords and add to context routing metadata
            if auto_tag {
                let tags = extract_tags(&combined, 10);
                if !tags.is_empty() {
                    let _ = rt_lock.add_tags(ctx_id, tags);
                }
            }

            let outcome_label = if pe <= 0.7 { "SUCCESS" } else { "FAILURE" };
            let redaction_note = if redacted > 0 {
                format!("\n**Security**: {redacted} sensitive token(s) redacted before storage.")
            } else {
                String::new()
            };
            let gate_label = if enforce { "Gate (enforced)" } else { "Gate (shadow)" };
            let text = format!(
                "## Memory Learned (text)\n\
                **Context**: {} ({})\n\
                **Outcome**: {} (prediction_error: {:.2})\n\
                **Node IDs**: state={} action={} outcome={}\n\
                **Auto-tagged**: {}{}\n\
                **{gate_label}**: novelty={:.3} score={:.3} would_index={}\n\n\
                State → Action → Outcome stored as causal graph nodes.\n\
                Use `memory_retrieve_text` with a similar query to recall this experience.",
                ctx.display_name, ctx.id,
                outcome_label, pe,
                result.state_node_id, result.action_node_id, result.outcome_node_id,
                if auto_tag { "yes" } else { "no" },
                redaction_note,
                novelty, gate_score, would_index,
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

    let ctx_id_owned  = params.context_id.clone();
    let retrieve_id   = params.retrieve_id.as_deref();
    let mut rt_lock   = rt.lock();
    let ctx_id        = ctx_id_owned.as_deref();

    // Fix 2: Idempotency check
    // If retrieve_id is provided and already in the ledger, the feedback was already applied.
    // Return an acknowledgement WITHOUT mutating bandit or decay state a second time.
    if let Some(rid) = retrieve_id {
        if rt_lock.is_feedback_applied(rid) {
            return ToolCallResult {
                content: vec![ContentItem::text(format!(
                    "## Memory Feedback (Already Applied)\n\
                    **retrieve_id**: `{rid}`\n\n\
                    Feedback for this retrieve_id was already applied in a previous call.\n\
                    No bandit or decay state was modified. This is the idempotent acknowledgement."
                ))],
                isError: None,
            };
        }
    }

    // Build the engine-level feedback key from retrieve_id.
    // Key format per node inside the engine: "{retrieve_id}/{node_id}/{success_as_01}"
    // This makes Beta(α,β) reflect evidence count, not submission count.
    // Unjoined feedback (no retrieve_id) passes None — applied unconditionally, no dedup.
    let feedback_key = retrieve_id.map(|r| r.to_string());

    match rt_lock.feedback_nodes(ctx_id, params.node_ids.clone(), success, feedback_key) {
        Ok((updated, ctx)) => {
            // Record retrieve_id in ledger AFTER successful application (not before)
            // so a crash between apply and record replays as a second apply (safe for Beta).
            if let Some(rid) = retrieve_id {
                rt_lock.record_feedback_applied(
                    rid,
                    &ctx.id.to_string(),
                    &outcome_str,
                    &params.node_ids,
                );
            }

            let outcome_label = if success { "SUCCESS" } else { "FAILURE" };
            let idempotency_note = if let Some(rid) = retrieve_id {
                format!("\n**retrieve_id**: `{rid}` — idempotency enabled, join key recorded in feedback_ledger.jsonl")
            } else {
                // Feedback without retrieve_id is valid — applied and counted.
                // It's marked "unjoined": cannot be linked to a retrieve_log entry for analysis.
                // Repeated calls are not deduplicated (at-most-once guarantee requires a retrieve_id).
                "\n_Status: unjoined — no retrieve_id provided. Feedback applied, but cannot be \
                linked to a retrieve_log entry. Repeated calls without an ID are not deduplicated._".to_string()
            };
            let text = format!(
                "## Memory Feedback Applied\n\
                **Context**: {} ({})\n\
                **Outcome**: {}\n\
                **Nodes requested (used)**: {}\n\
                **Nodes updated**: {}{}\n\n\
                Each updated node's usefulness prior (Beta posterior) and forgetting \
                half-life have been adjusted. Future retrievals will rank these nodes \
                {} when similar situations arise.",
                ctx.display_name, ctx.id,
                outcome_label,
                params.node_ids.len(),
                updated,
                idempotency_note,
                if success { "higher" } else { "lower" },
            );
            ToolCallResult { content: vec![ContentItem::text(text)], isError: None }
        }
        Err(e) => error_result(format!("Feedback failed: {e}")),
    }
}

