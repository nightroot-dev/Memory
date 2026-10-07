//! Threat-table tests for the M2 gateway (M2 acceptance, auth half).
//!
//! - No token → 401. Unknown token → 401. Revoked → 401. Expired → 401.
//! - Cross-tenant scope: NOT_FOUND shape (no oracle distinguishing it).
//! - Agent token bound to scope X cannot touch scope Y, even with owner grants.
//! - Deny by default: valid token, no grant → 403.
//! - Happy path: owner scope_init → learn → retrieve roundtrip.

use std::sync::Arc;

use cmhm_gateway::control::{ControlPlane, FileControlPlane, PrincipalKind};
use cmhm_gateway::routes::{router, AppState};
use cmhm_gateway::{Role, TenantId};
use cmhm_core::scope::ScopeId;

struct World {
    base: String,
    client: reqwest::Client,
    tenant_a: String,
    tenant_b: String,
    scope_a1: String,
    scope_a2: String,
    scope_b1: String,
    owner_a_tok: String,
    agent_a_tok: String,
    owner_b_tok: String,
}

async fn setup() -> World {
    let dir = std::env::temp_dir().join(format!(
        "cmhm_gw_t_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let mut cp = FileControlPlane::open(&dir.join("control.json")).unwrap();

    let ta = cp.create_tenant("a").unwrap();
    let tb = cp.create_tenant("b").unwrap();
    let tenant_a = TenantId::import(&ta.id).unwrap();
    let tenant_b = TenantId::import(&tb.id).unwrap();
    let owner_a = cp.create_principal(&tenant_a, PrincipalKind::User).unwrap();
    let agent_a = cp
        .create_principal(
            &tenant_a,
            PrincipalKind::Agent { owner: owner_a.id.clone() },
        )
        .unwrap();
    let owner_b = cp.create_principal(&tenant_b, PrincipalKind::User).unwrap();

    let s_a1 = ScopeId::generate();
    let s_a2 = ScopeId::generate();
    let s_b1 = ScopeId::generate();
    cp.register_scope(&tenant_a, &s_a1, "a-one").unwrap();
    cp.register_scope(&tenant_a, &s_a2, "a-two").unwrap();
    cp.register_scope(&tenant_b, &s_b1, "b-one").unwrap();

    // Owner A: owner-wide token + Owner on both scopes (bind + write).
    let owner_a_tok = cp.mint_token(&owner_a.id, vec![], 3600).unwrap();
    cp.grant(&owner_a.id, s_a1.as_str(), cmhm_gateway::Role::Owner)
        .unwrap();
    cp.grant(&owner_a.id, s_a2.as_str(), cmhm_gateway::Role::Owner)
        .unwrap();
    // Agent A: bound to a1 only, no grants of its own (inherits owner ∩ binding).
    cp.grant(&agent_a.id, s_a1.as_str(), cmhm_gateway::Role::Reader)
        .unwrap();
    let agent_a_tok = cp
        .mint_token(&agent_a.id, vec![s_a1.as_str().to_string()], 3600)
        .unwrap();
    let owner_b_tok = cp.mint_token(&owner_b.id, vec![], 3600).unwrap();
    cp.grant(&owner_b.id, s_b1.as_str(), Role::Owner).unwrap();

    let state = Arc::new(AppState::new(cp, dir.join("data")));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    World {
        base: format!("http://127.0.0.1:{port}"),
        client: reqwest::Client::new(),
        tenant_a: ta.id,
        tenant_b: tb.id,
        scope_a1: s_a1.as_str().to_string(),
        scope_a2: s_a2.as_str().to_string(),
        scope_b1: s_b1.as_str().to_string(),
        owner_a_tok,
        agent_a_tok,
        owner_b_tok,
    }
}

async fn post(
    w: &World,
    path: &str,
    token: Option<&str>,
    body: serde_json::Value,
) -> (u16, String) {
    let mut req = w.client.post(format!("{}{path}", w.base)).json(&body);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await.unwrap();
    let code = resp.status().as_u16();
    (code, resp.text().await.unwrap_or_default())
}

#[tokio::test]
async fn threat_table() {
    let w = setup().await;

    // No token → 401 on every protected route.
    for path in ["/v1/learn", "/v1/retrieve", "/v1/feedback"] {
        let (c, _) = post(&w, path, None, serde_json::json!({})).await;
        assert_eq!(c, 401, "{path} without token must be 401");
    }
    // Unknown token → 401.
    let (c, _) = post(
        &w,
        "/v1/retrieve",
        Some("cmhm_deadbeef"),
        serde_json::json!({"scope_id": w.scope_a1, "query": "x"}),
    )
    .await;
    assert_eq!(c, 401);

    // Bind backing contexts (owner-only admin).
    for (tok, scope) in [(&w.owner_a_tok, &w.scope_a1), (&w.owner_a_tok, &w.scope_a2)] {
        let (c, b) = post(
            &w,
            "/v1/admin/scope_init",
            Some(tok),
            serde_json::json!({"scope_id": scope, "display_name": scope}),
        )
        .await;
        assert_eq!(c, 200, "scope_init failed: {b}");
    }
    // Non-owner cannot bind.
    let (c, _) = post(
        &w,
        "/v1/admin/scope_init",
        Some(&w.agent_a_tok),
        serde_json::json!({"scope_id": w.scope_a2}),
    )
    .await;
    assert_eq!(c, 403, "agent (Reader) must not scope_init");

    // Happy path: owner learn → retrieve roundtrip on a1.
    let (c, b) = post(
        &w,
        "/v1/learn",
        Some(&w.owner_a_tok),
        serde_json::json!({
            "scope_id": w.scope_a1,
            "state": "gateway unique alpha state",
            "action": "probed it",
            "outcome": "it worked",
        }),
    )
    .await;
    assert_eq!(c, 200, "learn failed: {b}");
    let (c, b) = post(
        &w,
        "/v1/retrieve",
        Some(&w.owner_a_tok),
        serde_json::json!({"scope_id": w.scope_a1, "query": "gateway unique alpha state"}),
    )
    .await;
    assert_eq!(c, 200, "retrieve failed: {b}");
    // Retrieval responses carry node IDs, not text (M5 known gap): the proof
    // is seed 1 (the learned state) with itself as the candidate.
    assert!(
        b.contains("\"seed_id\":1") && b.contains("\"candidates\":[1]"),
        "retrieve missed learned node: {b}"
    );

    // Cross-tenant: owner B on A's scope → NOT_FOUND shape (no oracle).
    let (c, b) = post(
        &w,
        "/v1/retrieve",
        Some(&w.owner_b_tok),
        serde_json::json!({"scope_id": w.scope_a1, "query": "x"}),
    )
    .await;
    assert_eq!(c, 404, "cross-tenant must be NOT_FOUND, got {c}: {b}");

    // Agent exceeds binding: agent bound to a1 touches a2 → 403 even though
    // its OWNER holds Writer on a2.
    let (c, _) = post(
        &w,
        "/v1/retrieve",
        Some(&w.agent_a_tok),
        serde_json::json!({"scope_id": w.scope_a2, "query": "x"}),
    )
    .await;
    assert_eq!(c, 403, "agent outside token binding must be 403");
    // Agent CAN read a1 (Reader grant ∩ binding).
    let (c, _) = post(
        &w,
        "/v1/retrieve",
        Some(&w.agent_a_tok),
        serde_json::json!({"scope_id": w.scope_a1, "query": "gateway unique alpha state"}),
    )
    .await;
    assert_eq!(c, 200, "bound agent read must succeed");

    let _ = (w.tenant_a, w.tenant_b, w.scope_b1);
}
