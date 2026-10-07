//! TCP loopback server: newline-delimited JSON embed protocol.
//!
//! One Batcher shared across connections. Malformed lines get an error
//! object, never a dropped connection (callers must always get an answer).

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::batcher::Batcher;
use crate::protocol::{EmbedRequest, EmbedResponse, ErrorResponse};

#[derive(Debug, Clone)]
pub struct SidecarConfig {
    pub bind: String,
}

async fn handle_conn(stream: TcpStream, batcher: Batcher) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let out = match serde_json::from_str::<EmbedRequest>(&line) {
            Ok(req) => match batcher.embed(req.embed).await {
                Ok(vecs) => serde_json::to_string(&EmbedResponse {
                    model: batcher.model_id(),
                    dim: batcher.dim(),
                    embeddings: vecs,
                })
                .unwrap_or_else(|e| {
                    serde_json::to_string(&ErrorResponse {
                        error: format!("encode: {e}"),
                    })
                    .unwrap()
                }),
                Err(e) => serde_json::to_string(&ErrorResponse {
                    error: format!("embed: {e}"),
                })
                .unwrap(),
            },
            Err(e) => serde_json::to_string(&ErrorResponse {
                error: format!("bad request: {e}"),
            })
            .unwrap(),
        };
        writer.write_all(out.as_bytes()).await?;
        writer.write_all(b"\n").await?;
    }
    Ok(())
}

/// Serve forever on `cfg.bind`.
pub async fn serve(cfg: SidecarConfig, batcher: Batcher) -> Result<()> {
    let listener = TcpListener::bind(&cfg.bind).await?;
    serve_on(listener, batcher).await
}

/// Serve on an already-bound listener (tests bind port 0 to avoid races).
pub async fn serve_on(listener: TcpListener, batcher: Batcher) -> Result<()> {
    tracing::info!("sidecar listening");
    loop {
        let (stream, _) = listener.accept().await?;
        let b = batcher.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, b).await {
                tracing::debug!("sidecar conn ended: {e}");
            }
        });
    }
}
