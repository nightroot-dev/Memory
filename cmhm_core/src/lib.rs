// Deny panic-inducing patterns in production code.
// Tests are exempt (#[cfg(test)]) so existing test assertions are unaffected.
// This fires at *compile time*, not runtime — the library cannot kill the host process.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

//! # CMHM Core — Causal-Matrix Hybrid Memory Engine
//!
//! A causal world model with a full Context Runtime.
//! Each project/conversation gets an isolated `CoreState`
//! managed by the `ContextRuntime` orchestrator.
//!
//! ## Quick Start (with Context Runtime)
//! ```rust,no_run
//! use cmhm_core::{Config, EMBED_DIM, context::runtime::ContextRuntime, node::NodeType};
//!
//! let cfg = Config::default();
//! let mut rt = ContextRuntime::new(cfg, "doc-session".to_string()).unwrap();
//!
//! // Learn into the default context
//! let state_emb   = vec![0.0f32; EMBED_DIM];
//! let action_emb  = vec![0.0f32; EMBED_DIM];
//! let outcome_emb = vec![0.0f32; EMBED_DIM];
//! rt.learn(None, state_emb, action_emb, outcome_emb, 0.3, 0.5, "default",
//!          0.5, String::new(), None).unwrap();
//!
//! // Retrieve from a named context
//! let query = vec![0.0f32; EMBED_DIM];
//! let (result, _ctx) = rt.retrieve(None, query, "default", NodeType::State).unwrap();
//! println!("{}", result.explanation);
//! ```


pub mod node;
pub mod edge;
pub mod node_store;
pub mod edge_store;
pub mod working_memory;
pub mod episode_log;
pub mod salience;
pub mod dense_core;
pub mod retrieval;
pub mod projection;
pub mod consolidation;
pub mod persistence;
pub mod config;
pub mod core_state;
pub mod engine;
pub mod serde_helpers;
pub mod context;
pub mod text_embed;
pub mod bm25;
pub mod scrub;
pub mod embed;
pub mod authority;
pub mod migration;

// Re-export the most commonly used types at crate root
pub use config::Config;
pub use engine::{start, EngineHandle, LearnResult, SchemaMismatchError, SupersessionError};
pub use retrieval::{RetrievalResult, ChainStep, ConflictCandidate, ConflictType};

pub use node::NodeType;
pub use edge::{EdgeType, EvidenceType};
pub use context::runtime::ContextRuntime;
pub use context::{ContextDescriptor, ContextId, ContextType, ContextState};
pub use text_embed::{text_to_embedding, extract_tags};
pub use embed::{Embedder, HashEmbedder, default_embedder};
pub use authority::{clamp_authority, authenticate_and_clamp, CallerRole};



/// The embedding dimensionality used throughout CMHM.
///
/// Every node embedding, projection matrix output, and dense-core vector
/// is exactly this many `f32` values. Changing this constant requires
/// regenerating all projection matrices and snapshots.
pub const EMBED_DIM: usize = 128;
