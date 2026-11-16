//! Phase 5: Stage 2 cross-encoder reranking.
//!
//! Stage 1 (BM25 + dense RRF) is intentionally cheap and over-fetches a
//! top-25 pool. This module scores `(query, trigger_signature)` pairs with a
//! fine-tuned cross-encoder, applies sigmoid calibration, and returns the
//! reranked top-k. The model is local-only: a quantized ONNX file plus its
//! tokenizer files, loaded through the `fastembed` user-defined reranker API.
//!
//! The trait is always compiled so callers can depend on it without the
//! `onnx` feature. The real implementation is gated; without the feature (or
//! without a model directory) callers should fall back to Stage 1 scores.

#[cfg(feature = "onnx")]
use std::path::Path;

/// A Stage 2 reranker. Implementations score one query against a batch of
/// candidate documents and return `(candidate_index, calibrated_score)` in
/// `[0, 1]`.
pub trait Stage2Reranker: Send {
    fn rerank(&mut self, query: &str, documents: &[&str]) -> anyhow::Result<Vec<(usize, f32)>>;
}

/// Numerically stable logistic calibration for a raw reranker logit.
pub fn sigmoid(x: f32) -> f32 {
    if x >= 0.0 {
        let z = (-x).exp();
        1.0 / (1.0 + z)
    } else {
        let z = x.exp();
        z / (1.0 + z)
    }
}

/// Rerank `(node_id, text)` candidates and return the top `top_k` as
/// `(node_id, calibrated_score)`, highest first.
///
/// Ties are broken by the original Stage 1 order (stable sort), so the result
/// is deterministic for a deterministic reranker.
pub fn rerank_top_k(
    reranker: &mut dyn Stage2Reranker,
    query: &str,
    candidates: &[(u64, String)],
    top_k: usize,
) -> anyhow::Result<Vec<(u64, f32)>> {
    if candidates.is_empty() || top_k == 0 {
        return Ok(Vec::new());
    }
    let documents: Vec<&str> = candidates.iter().map(|(_, text)| text.as_str()).collect();
    let mut scored = reranker.rerank(query, &documents)?;
    // Reject out-of-range indices defensively: a malformed model must not
    // panic the server or silently map to another candidate.
    scored.retain(|(idx, score)| *idx < candidates.len() && score.is_finite());
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(top_k);
    Ok(scored
        .into_iter()
        .map(|(idx, score)| (candidates[idx].0, score))
        .collect())
}

/// Local cross-encoder reranker backed by a quantized ONNX model.
///
/// Expected directory layout (produced by `tools/train_experience_reranker.py`):
/// - `model_quantized.onnx`
/// - `tokenizer.json`
/// - `tokenizer_config.json`
/// - `special_tokens_map.json`
/// - `config.json`
#[cfg(feature = "onnx")]
pub struct ExperienceReranker {
    inner: std::sync::Mutex<fastembed::TextRerank>,
}

#[cfg(feature = "onnx")]
impl ExperienceReranker {
    /// Load the reranker from `dir`. Missing files are reported by name so a
    /// deployment error is actionable rather than a generic ONNX failure.
    pub fn from_dir(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let dir = dir.as_ref();
        let read = |name: &str| {
            std::fs::read(dir.join(name))
                .map_err(|e| anyhow::anyhow!("reranker: cannot read {}: {e}", dir.join(name).display()))
        };

        let model_path = dir.join("model_quantized.onnx");
        if !model_path.is_file() {
            anyhow::bail!(
                "reranker: model file not found at {} (run tools/train_experience_reranker.py)",
                model_path.display()
            );
        }

        let tokenizer_files = fastembed::TokenizerFiles {
            tokenizer_file: read("tokenizer.json")?,
            config_file: read("config.json")?,
            special_tokens_map_file: read("special_tokens_map.json")?,
            tokenizer_config_file: read("tokenizer_config.json")?,
        };
        let model = fastembed::UserDefinedRerankingModel::new(
            fastembed::OnnxSource::File(model_path),
            tokenizer_files,
        );
        // Let ORT use all available cores; capping intra-op threads made the
        // held-out bench ~2.5x slower with no benefit for a local server.
        let options = fastembed::RerankInitOptionsUserDefined::new().with_max_length(256);
        let inner = fastembed::TextRerank::try_new_from_user_defined(model, options)
            .map_err(|e| anyhow::anyhow!("reranker: init failed: {e}"))?;
        Ok(Self {
            inner: std::sync::Mutex::new(inner),
        })
    }
}

#[cfg(feature = "onnx")]
impl Stage2Reranker for ExperienceReranker {
    fn rerank(&mut self, query: &str, documents: &[&str]) -> anyhow::Result<Vec<(usize, f32)>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }
        // Recover from a poisoned mutex: the ONNX weights are read-only, so a
        // prior panic does not invalidate the session (same posture as the
        // embedder's M-10 recovery).
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let results = guard
            .rerank(query, documents.to_vec(), false, Some(32))
            .map_err(|e| anyhow::anyhow!("reranker: inference failed: {e}"))?;
        Ok(results
            .into_iter()
            .map(|result| (result.index, sigmoid(result.score)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockReranker {
        scores: Vec<f32>,
    }

    impl Stage2Reranker for MockReranker {
        fn rerank(
            &mut self,
            _query: &str,
            documents: &[&str],
        ) -> anyhow::Result<Vec<(usize, f32)>> {
            assert_eq!(documents.len(), self.scores.len());
            Ok(self.scores.iter().copied().enumerate().collect())
        }
    }

    #[test]
    fn sigmoid_is_bounded_and_centered() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(10.0) > 0.999);
        assert!(sigmoid(-10.0) < 0.001);
        assert!(sigmoid(100.0).is_finite());
        assert!(sigmoid(-100.0).is_finite());
    }

    #[test]
    fn rerank_top_k_orders_and_truncates() {
        let candidates = vec![
            (10, "alpha".to_string()),
            (20, "beta".to_string()),
            (30, "gamma".to_string()),
        ];
        let mut reranker = MockReranker {
            scores: vec![0.1, 0.9, 0.5],
        };
        let ranked = rerank_top_k(&mut reranker, "q", &candidates, 2).unwrap();
        assert_eq!(ranked, vec![(20, 0.9), (30, 0.5)]);
    }

    #[test]
    fn rerank_top_k_keeps_stage1_order_on_ties() {
        let candidates = vec![(1, "a".to_string()), (2, "b".to_string())];
        let mut reranker = MockReranker {
            scores: vec![0.5, 0.5],
        };
        let ranked = rerank_top_k(&mut reranker, "q", &candidates, 2).unwrap();
        assert_eq!(ranked, vec![(1, 0.5), (2, 0.5)]);
    }

    #[test]
    fn rerank_top_k_drops_invalid_indices_and_nan() {
        struct BadReranker;
        impl Stage2Reranker for BadReranker {
            fn rerank(
                &mut self,
                _query: &str,
                _documents: &[&str],
            ) -> anyhow::Result<Vec<(usize, f32)>> {
                Ok(vec![(99, 0.9), (0, f32::NAN), (1, 0.4)])
            }
        }
        let candidates = vec![(1, "a".to_string()), (2, "b".to_string())];
        let ranked = rerank_top_k(&mut BadReranker, "q", &candidates, 5).unwrap();
        assert_eq!(ranked, vec![(2, 0.4)]);
    }

    #[test]
    fn empty_candidates_short_circuit() {
        let mut reranker = MockReranker { scores: vec![] };
        assert!(rerank_top_k(&mut reranker, "q", &[], 5).unwrap().is_empty());
    }
}
