//! Context Router: probabilistic context selection.
//!
//! score(c) = alpha*Sq + beta*Sh + gamma*St + delta*Sa
//!
//! Sq = keyword/tag overlap with query hint  (semantic match)
//! Sh = normalised access frequency          (history)
//! St = recency score, exponential decay     (temporal)
//! Sa = 1 if Active/Idle else 0              (active bonus)
//!
//! All four weights and the decay half-life come from Config.
//! Nothing in this module is hardcoded.

use crate::config::Config;
use crate::context::{ContextId, ContextMeta, now_secs, fsm};

// --- RouterWeights -----------------------------------------------------------

/// Scoring weights, loaded entirely from Config. No defaults live here.
pub struct RouterWeights {
    pub alpha:              f32,   // semantic overlap
    pub beta:               f32,   // access frequency
    pub gamma:              f32,   // recency
    pub delta:              f32,   // active-context bonus
    pub half_life_secs:     f32,   // recency decay half-life
}

impl RouterWeights {
    /// Build from Config. The only way to construct RouterWeights.
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            alpha:          cfg.router_alpha,
            beta:           cfg.router_beta,
            gamma:          cfg.router_gamma,
            delta:          cfg.router_delta,
            half_life_secs: cfg.router_recency_half_life_secs,
        }
    }
}

// --- Core scoring function ---------------------------------------------------

/// Compute a routing score for a single context.
///
/// `query_hint` is free-text: keywords, the first ~80 chars of a prompt, etc.
/// `max_access_count` = highest access_count across all contexts (for Sh normalisation).
///
/// Returns score in [0.0, 1.0] (weights must sum to 1.0 for this guarantee).
pub fn score(
    meta: &ContextMeta,
    query_hint: &str,
    weights: &RouterWeights,
    max_access_count: u64,
) -> f32 {
    let now = now_secs();

    // Sq: semantic overlap (tag/name/description keyword match, case-insensitive)
    let sq = semantic_score(meta, query_hint);

    // Sh: normalised access frequency  (0 when no context has been accessed yet)
    let sh = if max_access_count == 0 {
        0.0f32
    } else {
        (meta.access_count as f32) / (max_access_count as f32)
    };

    // St: temporal recency using exponential decay with configurable half-life
    //   St = exp(-ln(2)/half_life * age_seconds)
    //   age = 0 → St = 1.0;  age = half_life → St = 0.5
    let age_secs = (now - meta.last_accessed).max(0) as f32;
    let lambda = if weights.half_life_secs > 0.0 {
        core::f32::consts::LN_2 / weights.half_life_secs
    } else {
        0.0
    };
    let st = (-lambda * age_secs).exp();

    // Sa: active-context bonus — delegates to FSM's single definition of "serviceable"
    let sa = if fsm::is_serviceable(meta.state) { 1.0f32 } else { 0.0f32 };

    weights.alpha * sq + weights.beta * sh + weights.gamma * st + weights.delta * sa
}

/// Select the single best context from a slice.
/// Returns `None` if the slice is empty.
pub fn best_match<'a>(
    contexts: &'a [&'a ContextMeta],
    query_hint: &str,
    weights: &RouterWeights,
) -> Option<&'a ContextMeta> {
    if contexts.is_empty() { return None; }
    let max_ac = contexts.iter().map(|m| m.access_count).max().unwrap_or(1);
    contexts.iter()
        .map(|m| (*m, score(m, query_hint, weights, max_ac)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(m, _)| m)
}

/// Return top-K (context, score) pairs, sorted descending by score.
pub fn top_k<'a>(
    contexts: &'a [&'a ContextMeta],
    query_hint: &str,
    weights: &RouterWeights,
    k: usize,
) -> Vec<(&'a ContextMeta, f32)> {
    if contexts.is_empty() { return vec![]; }
    let max_ac = contexts.iter().map(|m| m.access_count).max().unwrap_or(1);
    let mut scored: Vec<_> = contexts.iter()
        .map(|m| (*m, score(m, query_hint, weights, max_ac)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(k).collect()
}

/// Score a (id, meta) pair — useful when building scored lists for MCP responses.
#[allow(dead_code)]
pub fn score_with_id(
    id: &ContextId,
    meta: &ContextMeta,
    query_hint: &str,
    weights: &RouterWeights,
    max_access_count: u64,
) -> (ContextId, f32) {
    (id.clone(), score(meta, query_hint, weights, max_access_count))
}

// --- Internal helpers --------------------------------------------------------

fn semantic_score(meta: &ContextMeta, query_hint: &str) -> f32 {
    if query_hint.is_empty() { return 0.0; }

    let query_lower = query_hint.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
    if query_words.is_empty() { return 0.0; }

    // Build a single searchable string from all context metadata text
    let mut haystack = String::with_capacity(256);
    haystack.push_str(&meta.display_name.to_lowercase());
    haystack.push(' ');
    haystack.push_str(&meta.description.to_lowercase());
    for tag in &meta.tags {
        haystack.push(' ');
        haystack.push_str(&tag.to_lowercase());
    }

    let matched = query_words.iter()
        .filter(|w| haystack.contains(**w))
        .count();

    matched as f32 / query_words.len() as f32
}
