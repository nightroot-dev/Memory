//! Degradation tests (M5): sidecar down behavior.
//!
//! Separate binary (own process) because `CMHM_SIDECAR_ADDR` is process-global
//! env. Single test fn, strictly sequential: no intra-file parallelism hazards.

use std::sync::Arc;

use cmhm_gateway::control::{ControlPlane, FileControlPlane, PrincipalKind};
use cmhm_gateway::routes::{router, AppState};
use cmhm_gateway::{Role, TenantId};
use cmhm_core::scope::ScopeId;

async fn setup() -> (
    String,
    reqwest::Client,
    String,
    String,
    String,
    std::path::PathBuf,
    String,
) {
    // (base, client, owner_tok, scope, tenant_id, data_dir, _guard)
    let dir = std::env::temp_dir().join(format!(
        "cmhm_gw_deg_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let mut cp = FileControlPlane::open(&dir.join("control.json")).unwrap();
    let t = cp.create_tenant("deg").unwrap();
    let tenant = TenantId::import(&t.id).unwrap();
    let owner = cp.create_principal(&tenant, PrincipalKind::User).unwrap();
    let scope = ScopeId::generate();
    cp.register_scope(&tenant, &scope, "deg-scope").unwrap();
    let tok = cp.mint_token(&owner.id, vec![], 3600).unwrap();
    cp.grant(&owner.id, scope.as_str(), Role::Owner).unwrap();

    let state = Arc::new(AppState::new(cp, dir.join("data")));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
    (
        format!("http://127.0.0.1:{port}"),
        reqwest::Client::new(),
        tok,
        scope.as_str().to_string(),
        ta_id(&t.id),
        dir.join("data"),
        String::new(),
    )
}

fn ta_id(s: &str) -> String {
    s.to_string()
}

async fn post(
    base: &str,
    client: &reqwest::Client,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> (u16, String) {
    let resp = client
        .post(format!("{base}{path}"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap();
    let code = resp.status().as_u16();
    (code, resp.text().await.unwrap_or_default())
}

#[tokio::test]
async fn sidecar_down_degrades_explicitly() {
    // No sidecar configured: everything local.
    std::env::remove_var("CMHM_SIDECAR_ADDR");
    let (base, client, tok, scope, _tenant, _data, _) = setup().await;

    let (c, _) = post(
        &base,
        &client,
        "/v1/admin/scope_init",
        &tok,
        serde_json::json!({"scope_id": scope, "display_name": scope}),
    )
    .await;
    assert_eq!(c, 200);

    // One memory with a unique token, learned locally.
    let (c, _) = post(
        &base,
        &client,
        "/v1/learn",
        &tok,
        serde_json::json!({
            "scope_id": scope,
            "state": "degraded mode probe l1deg0001 accumulator",
            "action": "probed",
            "outcome": "worked",
        }),
    )
    .await;
    assert_eq!(c, 200);

    // Point at a dead port: learns must 503 explicitly (never silent).
    std::env::set_var("CMHM_SIDECAR_ADDR", "127.0.0.1:9");
    let (c, b) = post(
        &base,
        &client,
        "/v1/learn",
        &tok,
        serde_json::json!({
            "scope_id": scope,
            "state": "anything",
            "action": "x",
            "outcome": "y",
        }),
    )
    .await;
    assert_eq!(c, 503, "dead sidecar learn must be explicit 503, got {c}: {b}");
    assert!(b.contains("sidecar"), "error must name the sidecar: {b}");

    // Retrieval degrades to BM25-only and still finds the lexical hit:
    // single-memory scope, so the seed is deterministic.
    let (c, b) = post(
        &base,
        &client,
        "/v1/retrieve",
        &tok,
        serde_json::json!({"scope_id": scope, "query": "l1deg0001"}),
    )
    .await;
    assert_eq!(c, 200, "degraded retrieve must still answer: {b}");
    assert!(
        b.contains("\"seed_id\":1"),
        "BM25-only path must seed the lexical hit: {b}"
    );

    // Sidecar unconfigured again: local path works.
    std::env::remove_var("CMHM_SIDECAR_ADDR");
    let (c, _) = post(
        &base,
        &client,
        "/v1/learn",
        &tok,
        serde_json::json!({
            "scope_id": scope,
            "state": "after recovery",
            "action": "x",
            "outcome": "y",
        }),
    )
    .await;
    assert_eq!(c, 200);
}
