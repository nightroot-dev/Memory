/// Embedder trait — the single abstraction separating hash projections from ONNX models.
///
/// ## Design (N20, N21, N22)
///
/// Two feature gates, one invariant:
///   - `hash`  (default, always compiles, zero deps):  FNV-projection, 128-dim
///   - `onnx`  (opt-in,  requires MSVC toolchain):     MiniLM-L6-v2-int8, 384-dim
///
/// Only ONE backend is active per build. If both features are enabled, `onnx` wins.
/// The trait itself is always compiled — it's pure Rust with no deps.
///
/// ## N3 compliance
///
/// Every node stores the `model_id()` of the embedder that created its vector.
/// At retrieval time, nodes are only compared against a query produced by the SAME model.
/// Mixed-geometry vectors (hash 128-dim vs MiniLM 384-dim) would produce silently wrong
/// similarity scores — a class of bug that 158 green tests will never catch.
///
/// ## Adding a new backend
///
/// 1. Implement `Embedder` for your new type.
/// 2. Add a feature flag in `cmhm_core/Cargo.toml`.
/// 3. Update `default_embedder()` at the bottom of this file.
/// 4. That's it. Nothing else in the codebase needs to change.
use anyhow::Result;

// ─── Core Trait ──────────────────────────────────────────────────────────────

/// The embedding contract.
///
/// Implementors must be `Send + Sync` because the embedder is shared between
/// the MCP server thread (for text→vector conversion) and potentially a background
/// thread for batch re-embedding on model upgrade.
pub trait Embedder: Send + Sync {
    /// Stable string identifier for this model.
    ///
    /// Stamped on every node at write time (N3).
    /// Must never change between versions — changing it makes all existing nodes stale.
    /// Use versioned names: "hash-128d", "minilm-l6-v2-int8", etc.
    fn model_id(&self) -> &str;

    /// Output dimension of this embedder.
    fn dim(&self) -> usize;

    /// Embed a batch of texts. Returns one `Vec<f32>` per input, L2-normalized.
    ///
    /// Batch API is mandatory (not optional) because ONNX Runtime has significant
    /// per-call overhead (~0.5 ms). Batching 8 texts costs the same as batching 1.
    /// The single-text convenience method below calls this with a 1-element slice.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Embed a single text. Convenience wrapper over `embed_batch`.
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut batch = self.embed_batch(&[text])?;
        if batch.is_empty() {
            return Err(anyhow::anyhow!("embed_batch returned empty result for single text"));
        }
        Ok(batch.remove(0))
    }
}

// ─── Hash Backend (always compiled) ─────────────────────────────────────────

/// FNV hash-projection embedder.
///
/// Deterministic, zero deps, zero runtime cost. Produces 128-dim unit vectors.
/// Semantically meaningless — two semantically similar texts will map to
/// arbitrary positions in the 128-dim space — but correct for code identifier
/// exact-match retrieval via BM25 (which doesn't use these vectors at all).
///
/// Used as the fallback when `onnx` feature is not enabled.
pub struct HashEmbedder;

impl Embedder for HashEmbedder {
    fn model_id(&self) -> &str { "hash-128d" }

    fn dim(&self) -> usize { 128 }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter()
            .map(|t| crate::text_embed::text_to_embedding(t).to_vec())
            .collect())
    }
}

// ─── ONNX Backend (opt-in, MSVC toolchain required) ─────────────────────────

/// `all-MiniLM-L6-v2` int8-quantized embedder via fastembed + ONNX Runtime.
///
/// ## Requirements
/// - MSVC toolchain (per-project override set: `rustup override set stable-x86_64-pc-windows-msvc`)
/// - `onnx` feature enabled: `cargo build --features onnx`
/// - ~14 MB model downloaded on first run from Hugging Face Hub (cached locally after)
/// - ORT DLL bundled by fastembed (~4 MB, download during build)
///
/// ## Properties
/// - dim = 384 (full MiniLM output, no truncation)
/// - Quantized to int8 — accuracy loss < 1% vs float32, 4× smaller, 2× faster on CPU
/// - ~2 ms per embed on modern CPU (single text), ~10 ms for 32-text batch
///
/// ## Honesty about the tradeoff (D5)
/// This is not "zero dependencies." It adds:
///   - ONNX Runtime (~4 MB DLL bundled by cargo build)
///   - MiniLM-L6-v2 int8 model (~14 MB, downloaded on first run)
///   - Network access required on first run only
///
/// The README must say this. We no longer claim "tiny footprint."
#[cfg(feature = "onnx")]
pub struct OnnxEmbedder {
    model: std::sync::Mutex<fastembed::TextEmbedding>,
}

#[cfg(feature = "onnx")]
impl OnnxEmbedder {
    /// Initialise the embedder. Downloads the model on first call (~14 MB, HuggingFace).
    /// Subsequent calls use the local cache. This is the only network call in CMHM.
    pub fn new() -> Result<Self> {
        let opts = fastembed::InitOptions::new(fastembed::EmbeddingModel::AllMiniLML6V2Q)
            .with_show_download_progress(true);
        let model = fastembed::TextEmbedding::try_new(opts)
            .map_err(|e| anyhow::anyhow!("Failed to init MiniLM embedder: {e}"))?;
        Ok(Self { model: std::sync::Mutex::new(model) })
    }
}

#[cfg(feature = "onnx")]
impl Embedder for OnnxEmbedder {
    fn model_id(&self) -> &str { "minilm-l6-v2-int8" }

    fn dim(&self) -> usize { 384 }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        let mut guard = self.model.lock()
            .map_err(|e| anyhow::anyhow!("OnnxEmbedder lock poisoned: {e}"))?;
        // fastembed returns f32 embeddings, already L2-normalised
        guard
            .embed(texts.to_vec(), None)
            .map_err(|e| anyhow::anyhow!("embed_batch failed: {e}"))
    }
}

// ─── Factory ─────────────────────────────────────────────────────────────────

/// Construct the default embedder for this build configuration.
///
/// Selection order (first matching feature wins):
///   1. `onnx` feature → `OnnxEmbedder` (MiniLM-L6-v2-int8, 384-dim)
///   2. `hash` feature (default) → `HashEmbedder` (hash-128d, 128-dim)
///
/// Returns `Box<dyn Embedder>` so callers hold a trait object — the concrete type
/// is an implementation detail that changes with the feature flag.
pub fn default_embedder() -> Result<Box<dyn Embedder>> {
    #[cfg(feature = "onnx")]
    {
        return Ok(Box::new(OnnxEmbedder::new()?));
    }

    #[cfg(not(feature = "onnx"))]
    {
        Ok(Box::new(HashEmbedder))
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // --- HashEmbedder --------------------------------------------------------

    #[test]
    fn hash_embedder_model_id_stable() {
        let e = HashEmbedder;
        assert_eq!(e.model_id(), "hash-128d", "model_id must be stable across versions");
    }

    #[test]
    fn hash_embedder_dim_correct() {
        let e = HashEmbedder;
        assert_eq!(e.dim(), 128);
    }

    #[test]
    fn hash_embedder_embed_returns_correct_dim() {
        let e = HashEmbedder;
        let v = e.embed("cargo build failed linker error").unwrap();
        assert_eq!(v.len(), 128, "output must be 128-dim");
    }

    #[test]
    fn hash_embedder_output_is_unit_norm() {
        let e = HashEmbedder;
        let v = e.embed("nvidia provider bench-agent-k k2.6 opencode config").unwrap();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "output must be L2-normalized, got norm={norm:.6}");
    }

    #[test]
    fn hash_embedder_deterministic() {
        let e = HashEmbedder;
        let a = e.embed("the quick brown fox").unwrap();
        let b = e.embed("the quick brown fox").unwrap();
        assert_eq!(a, b, "same text must produce identical embedding");
    }

    #[test]
    fn hash_embedder_different_texts_different_embeddings() {
        let e = HashEmbedder;
        let a = e.embed("cargo build failed").unwrap();
        let b = e.embed("nvidia provider missing").unwrap();
        // Different texts should not produce identical vectors (would be a hash collision)
        let same = a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-8);
        assert!(!same, "different texts should produce different embeddings");
    }

    #[test]
    fn hash_embedder_batch_matches_single() {
        let e = HashEmbedder;
        let texts = ["cargo", "nvidia", "opencode"];
        let batch = e.embed_batch(&texts).unwrap();
        assert_eq!(batch.len(), 3);
        for (i, t) in texts.iter().enumerate() {
            let single = e.embed(t).unwrap();
            assert_eq!(batch[i], single, "batch[{i}] must equal single embed of '{t}'");
        }
    }

    #[test]
    fn hash_embedder_empty_batch() {
        let e = HashEmbedder;
        let result = e.embed_batch(&[]).unwrap();
        assert!(result.is_empty(), "empty batch must return empty vec");
    }

    #[test]
    fn hash_embedder_empty_string() {
        let e = HashEmbedder;
        // Empty string should not panic — output is a valid (possibly zero) unit vector
        let v = e.embed("").unwrap();
        assert_eq!(v.len(), 128);
        // All zeros after normalization of a zero vector is acceptable
    }

    // --- Factory ----------------------------------------------------------------

    #[test]
    fn default_embedder_constructs() {
        let e = default_embedder();
        assert!(e.is_ok(), "default_embedder() must succeed: {:?}", e.err());
    }

    #[test]
    fn default_embedder_model_id_nonempty() {
        let e = default_embedder().unwrap();
        assert!(!e.model_id().is_empty());
    }

    #[test]
    fn default_embedder_embed_works() {
        let e = default_embedder().unwrap();
        let v = e.embed("test text").unwrap();
        assert_eq!(v.len(), e.dim());
    }

    // --- OnnxEmbedder (only compiled when onnx feature is on) ----------------

    #[cfg(feature = "onnx")]
    #[test]
    fn onnx_embedder_model_id_stable() {
        // This test requires the model download (~14 MB). Only runs in CI with network.
        let e = OnnxEmbedder::new().expect("OnnxEmbedder init failed");
        assert_eq!(e.model_id(), "minilm-l6-v2-int8");
        assert_eq!(e.dim(), 384);
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn onnx_embedder_output_unit_norm() {
        let e = OnnxEmbedder::new().expect("OnnxEmbedder init failed");
        let v = e.embed("cargo build failed").unwrap();
        assert_eq!(v.len(), 384);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "MiniLM output must be unit-norm, got {norm:.6}");
    }
}
