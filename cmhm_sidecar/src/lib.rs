//! Co-located embedding sidecar (Expansion M5).
//!
//! One process per host, reached over TCP loopback (same-box IPC without
//! platform-specific pipes). Speaks newline-delimited JSON:
//!
//! ```text
//! → {"embed": ["text one", "text two"]}
//! ← {"model": "hash-128d", "dim": 128, "embeddings": [[...], [...]]}
//! ```
//!
//! Throughput design (the single-Mutex lesson):
//! - Requests arriving within [`BATCH_WINDOW_MS`] are embedded in ONE model
//!   call and fanned out, so concurrent users share inference cost.
//! - An LRU cache (`CACHE_CAP` entries) absorbs repeated query text, which
//!   dominates agent workloads.
//! - Backends: [`HashBackend`] always; ONNX (MiniLM/BGE) behind `--features
//!   onnx` via `CMHM_EMBEDDER`. The model is pinned per process; tenants pin
//!   per model_id at the gateway.
//!
//! Degradation contract (callers implement it, proven by their tests):
//! sidecar unreachable → retrieval degrades to BM25-only; learns are
//! rejected explicitly, never stored with a wrong or missing embedding.

pub mod batcher;
pub mod cache;
pub mod client;
pub mod protocol;
pub mod server;

pub use batcher::{Batcher, EmbedBackend};
#[cfg(feature = "onnx")]
pub use batcher::OnnxBackend;
pub use batcher::HashBackend;
pub use client::SidecarClient;
pub use cache::LruCache;
pub use protocol::{EmbedRequest, EmbedResponse};
pub use server::{serve, SidecarConfig};
