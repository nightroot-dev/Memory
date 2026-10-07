//! cmhm_sidecar binary.
//!
//! Usage:
//!   cmhm_sidecar --bind 127.0.0.1:18080 [--model hash]
//!
//! Model selection: `hash` always works. With `--features onnx`,
//! `CMHM_EMBEDDER=minilm|bge` picks the ONNX model (default minilm).

use cmhm_sidecar::batcher::{Batcher, HashBackend};
use cmhm_sidecar::server::{serve, SidecarConfig};
use cmhm_sidecar::EmbedBackend;

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

fn build_backend() -> anyhow::Result<Box<dyn EmbedBackend>> {
    #[cfg(feature = "onnx")]
    {
        return Ok(Box::new(cmhm_sidecar::batcher::OnnxBackend::new()?));
    }
    #[cfg(not(feature = "onnx"))]
    {
        Ok(Box::new(HashBackend))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let bind = arg_value("--bind", "127.0.0.1:18080");
    let batcher = Batcher::new(build_backend()?);
    tracing::info!(
        "sidecar model={} dim={}",
        batcher.model_id(),
        batcher.dim()
    );
    serve(SidecarConfig { bind }, batcher).await
}
