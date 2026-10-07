//! cmhm_gateway binary: multi-tenant HTTP gateway (Expansion M2).
//!
//! Usage:
//!   cmhm_gateway --data-root ./gw-data --control ./gw-data/control.json --bind 127.0.0.1:8080
//!
//! Tenant/provisioning model: tenants, principals, tokens, grants and scopes
//! are created through the owner-only admin surface (A1 CLI later). This
//! binary serves the data plane.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use cmhm_gateway::control::FileControlPlane;
use cmhm_gateway::routes::{router, AppState};

fn arg_value(name: &str, default: &str) -> String {
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == name {
            return it.next().unwrap_or_else(|| default.to_string());
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return v.to_string();
        }
    }
    default.to_string()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let data_root = PathBuf::from(arg_value("--data-root", "./gw-data"));
    let control_path = PathBuf::from(arg_value(
        "--control",
        &data_root.join("control.json").to_string_lossy().to_string(),
    ));
    let bind: SocketAddr = arg_value("--bind", "127.0.0.1:8080").parse()?;

    std::fs::create_dir_all(&data_root)?;
    let cp = FileControlPlane::open(&control_path)?;
    let state = Arc::new(AppState::new(cp, data_root));

    tracing::info!("cmhm_gateway listening on {bind}");
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, router(state)).await?;
    Ok(())
}
