//! HTTP routes (M2): versioned `/v1` JSON API over the engine.
//!
//! Every mutating/reading route (except `/health`) requires a bearer token.
//! Tenant always comes from the token. Roles derive from the credential:
//! user-kind principals act as `CallerRole::User`, agent/service tokens as
//! `CallerRole::Agent`. Client-supplied authority/provenance/role are never
//! accepted (C-01 structural close at the HTTP layer).
//!
//! Scope → engine-context resolution goes through the control plane's
//! `ScopeRef.context_id` only. Display names are never resolved (H6 lesson).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Extension, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::node::NodeType;
use cmhm_core::{text_to_embedding, ContextRuntime};

use crate::auth::{authenticate, authorize, AuthContext, Role};
use crate::control::{ControlPlane, FileControlPlane};

const MODEL_ID: &str = "hash-128d";

/// Requests per token per minute (fixed window). Generous for tests;
/// production policy lives in M4 rate work.
const RATE_LIMIT_PER_MIN: u64 = 600;

pub struct AppState {
    cp: Mutex<FileControlPlane>,
    runtimes: Mutex<HashMap<String, ContextRuntime>>,
    data_root: PathBuf,
    rate: Mutex<HashMap<String, (u64, Instant)>>,
}

impl AppState {
    pub fn new(cp: FileControlPlane, data_root: PathBuf) -> Self {
        Self {
            cp: Mutex::new(cp),
            runtimes: Mutex::new(HashMap::new()),
            data_root,
            rate: Mutex::new(HashMap::new()),
        }
    }
}

/// Runtime registry keyed by tenant. One ContextRuntime per tenant, lazily
/// created under its own data root: `<root>/tenants/<tenant_id>/`.
fn runtime_for_tenant(
    runtimes: &Mutex<HashMap<String, ContextRuntime>>,
    data_root: &PathBuf,
    tenant_id: &str,
) {
    let mut map = runtimes.lock();
    if !map.contains_key(tenant_id) {
        let dir = data_root.join("tenants").join(tenant_id);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("projections"));
        let mut cfg = Config::default();
        cfg.data_dir = dir.clone();
        cfg.projections_dir = dir.join("projections");
        if let Ok(rt) = ContextRuntime::new(cfg, format!("gw-{tenant_id}")) {
            map.insert(tenant_id.to_string(), rt);
        }
    }
}

#[derive(Clone)]
struct GateState {
    inner: Arc<AppState>,
}

async fn auth_middleware(
    State(gate): State<GateState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> axum::response::Response {
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // Rate limit BEFORE auth so stolen-token hammering is bounded (M4 owns policy).
    {
        let key = format!("{:x}", fnv1a(bearer.as_bytes()));
        let mut rate = gate.inner.rate.lock();
        let now = Instant::now();
        let entry = rate.entry(key).or_insert((0, now));
        if now.duration_since(entry.1).as_secs() >= 60 {
            *entry = (0, now);
        }
        entry.0 += 1;
        if entry.0 > RATE_LIMIT_PER_MIN {
            return (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded").into_response();
        }
    }
    let ctx = {
        let cp = gate.inner.cp.lock();
        match authenticate(&*cp, bearer) {
            Ok(c) => c,
            Err(e) => {
                let msg = e.to_string();
                let code = if msg.contains("E_UNAUTHENTICATED") {
                    StatusCode::UNAUTHORIZED
                } else {
                    StatusCode::BAD_REQUEST
                };
                return (code, msg).into_response();
            }
        }
    };
    req.extensions_mut().insert(ctx);
    next.run(req).await
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Authorize + resolve tenant runtime + bound context id.
///
/// Returns `(auth, role, context_id)`. The tenant comes from the token
/// (inside `authorize`); the context comes from the scope record. The
/// request body supplies neither.
fn scoped(
    gate: &GateState,
    mut ctx: AuthContext,
    scope_id: &str,
    need: Role,
) -> Result<(AuthContext, Role, String), (StatusCode, String)> {
    let (role, context_id) = {
        let cp = gate.inner.cp.lock();
        let role = authorize(&*cp, &mut ctx, scope_id, need).map_err(|e| {
            let msg = e.to_string();
            let code = if msg.contains("E_NOT_FOUND") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::FORBIDDEN
            };
            (code, msg)
        })?;
        let context_id = cp
            .scope(scope_id)
            .map(|s| s.context_id)
            .filter(|c| !c.is_empty())
            .ok_or((
                StatusCode::NOT_FOUND,
                "scope has no bound engine context (scope_init required)".to_string(),
            ))?;
        (role, context_id)
    };
    runtime_for_tenant(&gate.inner.runtimes, &gate.inner.data_root, &ctx.tenant_id);
    Ok((ctx, role, context_id))
}

fn role_to_caller(ctx: &AuthContext) -> (CallerRole, f32, String) {
    if ctx.is_agent {
        (CallerRole::Agent, 0.5, "inferred".to_string())
    } else {
        (CallerRole::User, 0.85, "user_explicit".to_string())
    }
}

// ─── Schemas ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LearnBody {
    scope_id: String,
    state: String,
    action: String,
    outcome: String,
}

#[derive(Serialize)]
struct LearnResp {
    state_node_id: u64,
    action_node_id: u64,
    outcome_node_id: u64,
}

#[derive(Deserialize)]
struct RetrieveBody {
    scope_id: String,
    query: String,
}

#[derive(Serialize)]
struct RetrieveResp {
    seed_id: Option<u64>,
    explanation: String,
    candidates: Vec<u64>,
}

#[derive(Deserialize)]
struct FeedbackBody {
    scope_id: String,
    node_ids: Vec<u64>,
    success: bool,
}

// ─── Handlers ───────────────────────────────────────────────────────────────

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")}))
}

#[axum::debug_handler]
async fn learn(
    State(gate): State<GateState>,
    Extension(ctx): Extension<AuthContext>,
    Json(body): Json<LearnBody>,
) -> Result<Json<LearnResp>, (StatusCode, String)> {
    let (ctx, _role, context_id) = scoped(&gate, ctx, &body.scope_id, Role::Writer)?;
    let (caller, authority, provenance) = role_to_caller(&ctx);
    // Embeddings are server-side (hash model; sidecar is M5).
    let s = text_to_embedding(&body.state).to_vec();
    let a = text_to_embedding(&body.action).to_vec();
    let o = text_to_embedding(&body.outcome).to_vec();
    let mut runtimes = gate.inner.runtimes.lock();
    let rt = runtimes.get_mut(&ctx.tenant_id).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "runtime missing after resolve".to_string(),
    ))?;
    let (res, _) = rt
        .learn_text_with_context(
            Some(&context_id),
            s,
            a,
            o,
            0.5,
            0.5,
            MODEL_ID,
            authority,
            provenance,
            None,
            caller,
            None,
            None,
            Some((body.state, body.action, body.outcome)),
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(LearnResp {
        state_node_id: res.state_node_id,
        action_node_id: res.action_node_id,
        outcome_node_id: res.outcome_node_id,
    }))
}

/// Bind the backing engine context for a scope. Owner-only. The mapping
/// (scope → context UUID) is recorded in the control plane; later calls
/// resolve through it, never through names.
async fn scope_init(
    State(gate): State<GateState>,
    Extension(ctx): Extension<AuthContext>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let scope_id = body
        .get("scope_id")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "scope_id required".to_string()))?;
    let display = body
        .get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or(scope_id);
    let (ctx, _role, _) = scoped_init_path(&gate, ctx, scope_id)?;
    runtime_for_tenant(&gate.inner.runtimes, &gate.inner.data_root, &ctx.tenant_id);
    let mut runtimes = gate.inner.runtimes.lock();
    let rt = runtimes.get_mut(&ctx.tenant_id).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "runtime missing after resolve".to_string(),
    ))?;
    use cmhm_core::ContextType;
    let desc = rt
        .create_context(display, ContextType::Project, None, display, vec![])
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    {
        let mut cp = gate.inner.cp.lock();
        cp.bind_scope_context(scope_id, &desc.id.to_string())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    Ok(Json(serde_json::json!({"context_id": desc.id.to_string()})))
}

/// scope_init authorizes Owner on the scope but must not require a bound
/// context (chicken-and-egg: binding is what this call creates).
fn scoped_init_path(
    gate: &GateState,
    mut ctx: AuthContext,
    scope_id: &str,
    // Owner-only by policy; enforced here rather than in `scoped`.
    // (Kept as a separate function so the two paths never share logic.)
) -> Result<(AuthContext, Role, ()), (StatusCode, String)> {
    let role = {
        let cp = gate.inner.cp.lock();
        authorize(&*cp, &mut ctx, scope_id, Role::Owner).map_err(|e| {
            let msg = e.to_string();
            let code = if msg.contains("E_NOT_FOUND") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::FORBIDDEN
            };
            (code, msg)
        })?
    };
    Ok((ctx, role, ()))
}

async fn retrieve(
    State(gate): State<GateState>,
    Extension(ctx): Extension<AuthContext>,
    Json(body): Json<RetrieveBody>,
) -> Result<Json<RetrieveResp>, (StatusCode, String)> {
    let (ctx, _role, context_id) = scoped(&gate, ctx, &body.scope_id, Role::Reader)?;
    let q = text_to_embedding(&body.query).to_vec();
    let mut runtimes = gate.inner.runtimes.lock();
    let rt = runtimes.get_mut(&ctx.tenant_id).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "runtime missing after resolve".to_string(),
    ))?;
    let (res, _) = rt
        .retrieve(
            Some(&context_id),
            q,
            MODEL_ID,
            NodeType::State,
            &body.query,
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(RetrieveResp {
        seed_id: res.seed_id,
        explanation: res.explanation,
        candidates: res.post_filter_candidates,
    }))
}

async fn feedback(
    State(gate): State<GateState>,
    Extension(ctx): Extension<AuthContext>,
    Json(body): Json<FeedbackBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let (ctx, _role, context_id) = scoped(&gate, ctx, &body.scope_id, Role::Writer)?;
    let mut runtimes = gate.inner.runtimes.lock();
    let rt = runtimes.get_mut(&ctx.tenant_id).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "runtime missing after resolve".to_string(),
    ))?;
    let (updated, _) = rt
        .feedback_nodes(Some(&context_id), body.node_ids, body.success, None)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({"updated": updated})))
}

pub fn router(state: Arc<AppState>) -> Router {
    let gate = GateState { inner: state };
    Router::new()
        .route("/health", get(health))
        .route("/v1/learn", post(learn))
        .route("/v1/retrieve", post(retrieve))
        .route("/v1/feedback", post(feedback))
        .route("/v1/admin/scope_init", post(scope_init))
        .layer(middleware::from_fn_with_state(
            gate.clone(),
            auth_middleware,
        ))
        .with_state(gate)
}
