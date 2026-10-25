//! Deterministic text → embedding conversion.
//!
//! Converts arbitrary plain text into a normalised [`EMBED_DIM`]-dimensional
//! float32 vector using FNV-1a hash projection — no ML runtime required.
//!
//! ## Properties
//! - **Deterministic**: same text always yields the same vector.
//! - **Zero dependencies**: uses only the standard library + crate constants.
//! - **Cosine-compatible**: output is L2-normalised for direct cosine comparison.
//! - **Partial-match aware**: character 3-grams give non-zero similarity to
//!   morphological variants (e.g. "config" ~ "configure").
//!
//! ## Intended use
//! Feed the output directly into [`ContextRuntime::retrieve`] or
//! [`ContextRuntime::learn`] as the embedding vector. Pass `"text-hash"` as the
//! `model_id` — the projection registry will use the identity fallback (the
//! vector is already in the CMHM space).

use crate::EMBED_DIM;

// ─── FNV-1a 64-bit hash ───────────────────────────────────────────────────────

#[inline(always)]
fn fnv1a(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME:  u64 = 1099511628211;
    let mut h = FNV_OFFSET;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Convert arbitrary plain text to a deterministic [`EMBED_DIM`]-dim embedding.
///
/// Algorithm:
/// 1. Lowercase + tokenise on non-alphanumeric boundaries.
/// 2. **Word hashes** — each word scatters its FNV-1a hash across 4 dimensions
///    with decreasing weights (1.0, 0.7, 0.5, 0.3). Short words (< 2 chars)
///    are skipped to reduce stop-word noise.
/// 3. **Character 3-gram hashes** — each overlapping trigram adds a lighter
///    signal (0.15) for partial / morphological matches.
/// 4. **L2-normalise** the accumulated vector.
///
/// # Example
/// ```
/// use cmhm_core::text_to_embedding;
/// let v = text_to_embedding("nvidia provider model catalog");
/// assert!((v.iter().map(|x| x*x).sum::<f32>().sqrt() - 1.0).abs() < 1e-5);
/// ```
pub fn text_to_embedding(text: &str) -> [f32; EMBED_DIM] {
    let mut vec = [0.0f32; EMBED_DIM];
    let lower = text.to_lowercase();

    // Word-level hashes: 4 scatter points per word
    for word in lower.split(|c: char| !c.is_alphanumeric()) {
        if word.len() < 2 { continue; }
        let h = fnv1a(word.as_bytes());
        vec[(h as usize)                            % EMBED_DIM] += 1.0;
        vec[((h >> 17) as usize)                    % EMBED_DIM] += 0.7;
        vec[((h ^ 0xdead_beef_u64) as usize)        % EMBED_DIM] += 0.5;
        vec[(h.wrapping_mul(2_654_435_761) as usize) % EMBED_DIM] += 0.3;
    }

    // Character 3-gram hashes: partial/morphological match signal
    let bytes = lower.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        let h = fnv1a(&bytes[i..i + 3]);
        vec[(h as usize) % EMBED_DIM] += 0.15;
    }

    // L2-normalise in-place
    let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 {
        for v in vec.iter_mut() { *v /= norm; }
    }

    vec
}

/// Extract keyword tags from text for context routing improvement.
///
/// Returns lowercase tokens that are:
/// - ≥ 4 characters (filters single-char stop words and short words)
/// - Not in a hard-coded English stop-word list
/// - Deduplicated, insertion-ordered
/// - Capped at `max_tags`
///
/// # Example
/// ```
/// use cmhm_core::extract_tags;
/// let tags = extract_tags("opencode config nvidia provider model catalog modelx", 5);
/// assert!(tags.contains(&"opencode".to_string()));
/// assert!(!tags.contains(&"and".to_string()));
/// ```
pub fn extract_tags(text: &str, max_tags: usize) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "this", "that", "with", "from", "have", "been", "were", "they",
        "them", "their", "then", "when", "what", "which", "will", "into",
        "also", "some", "more", "than", "your", "about", "after", "before",
        "could", "would", "should", "there", "these", "those", "where",
        "here", "each", "such", "both", "only", "even", "just", "like",
        "make", "made", "done", "does", "said", "want", "need", "used",
        // M-01: "using" kept (useful for "using X library"). "call" and "added" deduped.
        // "file" and "path" REMOVED — highly discriminative for coding assistant routing.
        "using", "call", "adds", "added",
    ];

    let mut seen  = std::collections::HashSet::new();
    let mut tags  = Vec::new();

    for word in text.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if word.len() < 4                  { continue; }
        if STOP_WORDS.contains(&word)      { continue; }
        if seen.contains(word)             { continue; }
        seen.insert(word.to_string());
        tags.push(word.to_string());
        if tags.len() >= max_tags          { break; }
    }

    tags
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_is_unit_length() {
        let v = text_to_embedding("hello world test embedding");
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "norm was {norm}");
    }

    #[test]
    fn embedding_is_deterministic() {
        let v1 = text_to_embedding("opencode config nvidia");
        let v2 = text_to_embedding("opencode config nvidia");
        assert_eq!(v1, v2);
    }

    #[test]
    fn different_texts_differ() {
        let v1 = text_to_embedding("nvidia gpu driver");
        let v2 = text_to_embedding("python flask web server");
        // cosine similarity should be low (not identical)
        let cos: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        assert!(cos < 0.9, "unrelated texts too similar: {cos}");
    }

    #[test]
    fn similar_texts_are_closer() {
        let v1 = text_to_embedding("nvidia provider model");
        let v2 = text_to_embedding("nvidia provider models catalog");
        let v3 = text_to_embedding("python flask server route");
        let cos12: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        let cos13: f32 = v1.iter().zip(v3.iter()).map(|(a, b)| a * b).sum();
        assert!(cos12 > cos13, "similar pair ({cos12:.3}) not closer than dissimilar ({cos13:.3})");
    }

    #[test]
    fn extract_tags_filters_stop_words() {
        let tags = extract_tags("this should filter stop words like with from", 10);
        assert!(!tags.contains(&"this".to_string()));
        assert!(!tags.contains(&"with".to_string()));
        assert!(!tags.contains(&"from".to_string()));
    }

    #[test]
    fn extract_tags_caps_at_max() {
        let tags = extract_tags("alpha beta gamma delta epsilon zeta theta", 3);
        assert_eq!(tags.len(), 3);
    }

    #[test]
    fn extract_tags_deduplicates() {
        let tags = extract_tags("nvidia nvidia provider nvidia", 10);
        assert_eq!(tags.iter().filter(|t| *t == "nvidia").count(), 1);
    }

    #[test]
    fn empty_text_returns_zero_vec() {
        let v = text_to_embedding("");
        assert!(v.iter().all(|x| *x == 0.0));
    }
}
