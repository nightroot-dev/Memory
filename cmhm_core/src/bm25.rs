/// BM25 (Okapi BM25) inverted index for lexical retrieval over memory text.
///
/// ## Why BM25 alongside dense retrieval?
/// Dense cosine similarity fails for code identifiers: `cos("Arc", "Arc<Mutex<T>>") ≈ 0`
/// in hash-projection space. BM25 uses exact term matching and always finds
/// `Arc<Mutex<T>>` if it was stored. Hybrid retrieval (BM25 + dense) dominates
/// either approach alone on real agent workloads.
///
/// ## Formula
/// ```text
/// score(q, d) = Σ_t IDF(t) · f(t,d)·(k₁+1) / [f(t,d) + k₁·(1 − b + b·|d|/avgdl)]
/// IDF(t) = ln(1 + (N − n_t + 0.5) / (n_t + 0.5))
/// ```
/// Constants: k₁ = 1.5, b = 0.75 (middle of recommended ranges).
///
/// ## Design
/// - Index is append-only: new terms merged on `insert()`.
/// - avgdl updated incrementally — no full re-scan on each insert.
/// - Tokenization: lowercase, split on non-alphanumeric, filter stopwords.
///   This keeps the index small and avoids noise from punctuation.
/// - Storage: `HashMap<String, PostingList>` where each posting is (node_id, tf).
///   Compact: most real workloads have < 50k unique terms.
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ─── Constants ────────────────────────────────────────────────────────────────

/// BM25 term-frequency saturation. Higher = less saturation. Range 1.2–2.0.
/// 1.5 is the middle of the recommended range for short technical text.
const K1: f32 = 1.5;

/// BM25 document-length normalization. 0 = no normalization, 1 = full.
/// 0.75 is standard for mixed prose+code.
const B: f32 = 0.75;

// ─── Types ────────────────────────────────────────────────────────────────────

/// (node_id, within-document term frequency)
type Posting = (u64, f32);

/// Per-term posting list.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PostingList {
    /// Sorted by node_id for cache efficiency on intersection.
    postings: Vec<Posting>,
    /// Number of documents that contain this term (n_t).
    doc_freq: u32,
}

impl PostingList {
    fn insert(&mut self, node_id: u64, tf: f32) {
        // Keep sorted by node_id for binary search
        match self.postings.binary_search_by_key(&node_id, |&(id, _)| id) {
            Ok(i)  => self.postings[i] = (node_id, tf),   // update existing
            Err(i) => {
                self.postings.insert(i, (node_id, tf));
                self.doc_freq += 1;
            }
        }
    }

    fn remove(&mut self, node_id: u64) {
        if let Ok(i) = self.postings.binary_search_by_key(&node_id, |&(id, _)| id) {
            self.postings.remove(i);
            self.doc_freq = self.doc_freq.saturating_sub(1);
        }
    }

    #[allow(dead_code)] // L-05: part of PostingList API; query path accesses postings directly
    fn tf(&self, node_id: u64) -> Option<f32> {
        self.postings.binary_search_by_key(&node_id, |&(id, _)| id)
            .ok()
            .map(|i| self.postings[i].1)
    }
}

// ─── BM25 Index ──────────────────────────────────────────────────────────────

/// Incrementally maintained BM25 inverted index.
///
/// All operations are O(|text|) for insert and O(|query|·|posting|) for score.
/// For typical workloads (< 100k nodes, < 50k unique terms), this fits in < 20 MB.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Bm25Index {
    /// Inverted index: term → posting list
    index: HashMap<String, PostingList>,
    /// Per-document length (number of tokens after tokenization)
    doc_lengths: HashMap<u64, u32>,
    /// H-10 fix: reverse index node_id → distinct terms for O(terms/doc) removal.
    /// Without this, remove() scans all posting lists — O(unique_terms_in_index).
    #[serde(default)]
    node_terms: HashMap<u64, Vec<String>>,
    /// Running sum of all document lengths (for avgdl)
    total_length: u64,
    /// Number of indexed documents
    num_docs: u64,
}

impl Bm25Index {
    pub fn new() -> Self {
        Self::default()
    }

    /// Index a node's text. If the node was previously indexed, the old entry is replaced.
    pub fn insert(&mut self, node_id: u64, text: &str) {
        // Remove old entry if re-indexing.
        // remove() already decrements total_length and num_docs — do not subtract again.
        if self.doc_lengths.contains_key(&node_id) {
            self.remove(node_id);
        }

        let tokens = tokenize(text);
        let doc_len = tokens.len() as u32;

        // Count term frequencies
        let mut tf_map: HashMap<&str, u32> = HashMap::new();
        for tok in &tokens {
            *tf_map.entry(tok.as_str()).or_insert(0) += 1;
        }

        // Insert into posting lists; collect distinct terms for reverse index.
        let mut term_set: Vec<String> = Vec::with_capacity(tf_map.len());
        for (term, raw_tf) in &tf_map {
            let tf = *raw_tf as f32;
            self.index
                .entry((*term).to_string())
                .or_default()
                .insert(node_id, tf);
            term_set.push((*term).to_string());
        }

        // H-10: store reverse mapping for O(terms/doc) removal.
        self.node_terms.insert(node_id, term_set);
        self.doc_lengths.insert(node_id, doc_len);
        self.total_length += doc_len as u64;
        self.num_docs += 1;
    }

    /// Remove a node from the index.
    ///
    /// H-10 fix: uses `node_terms` reverse index for O(terms/doc) removal instead of
    /// scanning all posting lists O(unique_terms_in_index). During consolidation when
    /// many nodes are pruned simultaneously, this prevents O(dead_nodes × vocabulary) spikes.
    pub fn remove(&mut self, node_id: u64) {
        if let Some(&doc_len) = self.doc_lengths.get(&node_id) {
            // Use reverse index to remove only the posting lists this node appears in.
            if let Some(terms) = self.node_terms.remove(&node_id) {
                for term in &terms {
                    if let Some(list) = self.index.get_mut(term) {
                        list.remove(node_id);
                        // Prune empty posting lists to keep memory bounded.
                        // (HashMap entry removal not done here to avoid borrow issues;
                        //  empty lists are filtered at query time.)
                    }
                }
            } else {
                // Fallback for nodes indexed before H-10 (no node_terms entry).
                // This path runs at most once per pre-existing node.
                for list in self.index.values_mut() {
                    list.remove(node_id);
                }
            }
            self.doc_lengths.remove(&node_id);
            self.total_length = self.total_length.saturating_sub(doc_len as u64);
            self.num_docs = self.num_docs.saturating_sub(1);
        }
    }

    /// Score all documents for a query. Returns `(node_id, score)` sorted descending.
    ///
    /// Returns at most `top_k` results. Only documents containing at least one query term
    /// are returned (BM25 is sparse — non-matching docs score 0 and are excluded).
    pub fn query(&self, query_text: &str, top_k: usize) -> Vec<(u64, f32)> {
        if self.num_docs == 0 {
            return vec![];
        }

        let avgdl = self.total_length as f32 / self.num_docs as f32;
        let query_tokens = tokenize(query_text);

        // Deduplicate query terms (IDF is per-term, not per-occurrence)
        let mut seen = std::collections::HashSet::new();
        let unique_terms: Vec<String> = query_tokens.into_iter().filter(|t| seen.insert(t.clone())).collect();

        // Accumulate BM25 scores across candidates
        let mut scores: HashMap<u64, f32> = HashMap::new();

        for term in &unique_terms {
            let list = match self.index.get(term) {
                Some(l) if l.doc_freq > 0 => l,
                _ => continue,
            };

            let n_t = list.doc_freq as f32;
            let n   = self.num_docs as f32;
            let idf = ((1.0 + (n - n_t + 0.5) / (n_t + 0.5)).ln()).max(0.0);

            for &(node_id, tf) in &list.postings {
                let doc_len = *self.doc_lengths.get(&node_id).unwrap_or(&1) as f32;
                let norm    = 1.0 - B + B * doc_len / avgdl;
                let tf_norm = tf * (K1 + 1.0) / (tf + K1 * norm);
                let delta   = idf * tf_norm;
                *scores.entry(node_id).or_insert(0.0) += delta;
            }
        }

        // Sort and return top_k
        let mut results: Vec<(u64, f32)> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }

    /// Minimum document frequency among query terms present in the index.
    ///
    /// Returns None if no query term appears in the index at all.
    /// Used by the retrieval fusion gate: a query whose every matching
    /// term is common (high df) gets no discriminative signal from BM25
    /// and should stay dense-only. A query with at least one rare
    /// (low-df) matching term carries genuine lexical evidence.
    pub fn rarest_query_df(&self, query_text: &str) -> Option<u32> {
        let mut seen = std::collections::HashSet::new();
        let mut best: Option<u32> = None;
        for t in tokenize(query_text) {
            if !seen.insert(t.clone()) {
                continue;
            }
            if let Some(list) = self.index.get(&t) {
                if list.doc_freq > 0 {
                    best = Some(best.map_or(list.doc_freq, |b| b.min(list.doc_freq)));
                }
            }
        }
        best
    }

    /// Number of indexed documents.
    pub fn len(&self) -> u64 {
        self.num_docs
    }

    pub fn is_empty(&self) -> bool {
        self.num_docs == 0
    }
}

// ─── Tokenizer ────────────────────────────────────────────────────────────────

/// Tokenize text for BM25 indexing.
///
/// Strategy: lowercase → split on non-alphanumeric boundaries → filter stopwords + short tokens.
/// Preserves identifiers like `Arc`, `Mutex`, `unwrap` as full tokens.
/// Strips common English stopwords to reduce index noise.
fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in lower.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else {
            if current.len() >= 2 && !is_stopword(&current) {
                tokens.push(current.clone());
            }
            current.clear();
        }
    }
    if current.len() >= 2 && !is_stopword(&current) {
        tokens.push(current);
    }

    tokens
}

/// Common English stopwords to filter from BM25 index.
/// Kept minimal — we want to keep technical words like "from", "to", "use" in code contexts.
fn is_stopword(word: &str) -> bool {
    matches!(word,
        "the" | "and" | "for" | "was" | "but" | "are" | "has" | "had"
        | "not" | "with" | "this" | "that" | "have" | "from" | "they"
        | "been" | "its" | "into" | "than" | "when" | "who" | "will"
    )
}

// ─── RRF Fusion ──────────────────────────────────────────────────────────────

/// Reciprocal Rank Fusion — combine dense and BM25 ranked lists without score calibration.
///
/// Formula: `RRF(d) = Σ_r  1 / (k + rank_r(d))`
/// k = 60 is the standard value. It smooths out rank differences at the top.
///
/// ## Why RRF first, learned reranker later?
/// RRF is parameter-free — it requires zero training data and is provably competitive.
/// A learned reranker only surpasses RRF after ~1k labeled examples. Ship RRF now,
/// switch to learned reranker when you have evidence it's worth the complexity.
pub const RRF_K: f32 = 60.0;

/// Fuse two ranked lists using RRF.
///
/// `dense_ranked`: list of (node_id, dense_score) already sorted descending.
/// `bm25_ranked`:  list of (node_id, bm25_score) already sorted descending.
/// Returns fused (node_id, rrf_score) sorted descending, capped at `top_k`.
pub fn rrf_fuse(
    dense_ranked: &[(u64, f32)],
    bm25_ranked:  &[(u64, f32)],
    top_k: usize,
) -> Vec<(u64, f32)> {
    let mut scores: HashMap<u64, f32> = HashMap::new();

    for (rank, &(node_id, _)) in dense_ranked.iter().enumerate() {
        *scores.entry(node_id).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
    }
    for (rank, &(node_id, _)) in bm25_ranked.iter().enumerate() {
        *scores.entry(node_id).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
    }

    let mut fused: Vec<(u64, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused.truncate(top_k);
    fused
}

/// Max additive bonus (cosine scale) for a node that contains an explicit identifier from
/// the query. Split evenly when a few nodes of the wanted type contain the same identifier.
pub const LEX_BOOST_MAX: f32 = 0.7;
/// An identifier present in more wanted-type nodes than this is not specific enough to boost.
const IDENT_MAX_DOCS: usize = 3;

/// Identifier-like raw token: length >= 4 and containing a digit, an underscore, or an
/// interior uppercase letter (camelCase / PascalCase / mixed). IDs, error codes, symbols.
fn is_identifier_like(tok: &str) -> bool {
    tok.chars().count() >= 4
        && (tok.chars().any(|c| c.is_ascii_digit() || c == '_')
            || tok.chars().skip(1).any(|c| c.is_uppercase()))
}

/// True if the raw query contains at least one identifier-like token.
pub fn has_identifier_token(text: &str) -> bool {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_')).any(is_identifier_like)
}

impl Bm25Index {
    /// Node ids whose indexed text contains `term` (a term already in index form).
    fn posting_ids(&self, term: &str) -> Vec<u64> {
        self.index
            .get(term)
            .map(|l| l.postings.iter().map(|&(id, _)| id).collect())
            .unwrap_or_default()
    }

    /// Identifier evidence channel. For every identifier-like token in the query, find the
    /// nodes whose text contains it (exact term, via the SAME tokenizer the index was built
    /// with) and that satisfy `is_wanted` (the requested node type). If only a few qualify
    /// (<= IDENT_MAX_DOCS) each gets `LEX_BOOST_MAX / count`. Independent of BM25's overall
    /// ranking, so ordinary query words can never outvote an explicit identifier, and
    /// action/outcome nodes repeating the identifier cannot dilute the state node.
    pub fn identifier_boost(&self, query_text: &str, is_wanted: impl Fn(u64) -> bool) -> HashMap<u64, f32> {
        let mut out: HashMap<u64, f32> = HashMap::new();
        for raw in query_text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            if !is_identifier_like(raw) {
                continue;
            }
            let terms = tokenize(raw);
            if terms.is_empty() {
                continue;
            }
            let mut ids: Option<Vec<u64>> = None;
            for t in &terms {
                let p = self.posting_ids(t);
                ids = Some(match ids {
                    None => p,
                    Some(prev) => prev.into_iter().filter(|id| p.contains(id)).collect(),
                });
            }
            let wanted: Vec<u64> = ids.unwrap_or_default().into_iter().filter(|&id| is_wanted(id)).collect();
            if wanted.is_empty() || wanted.len() > IDENT_MAX_DOCS {
                continue;
            }
            let b = LEX_BOOST_MAX / wanted.len() as f32;
            for id in wanted {
                let e = out.entry(id).or_insert(0.0);
                if b > *e {
                    *e = b;
                }
            }
        }
        out
    }
}
// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // --- Tokenizer ------------------------------------------------------------

    #[test]
    fn tokenize_preserves_identifiers() {
        let tokens = tokenize("Arc<Mutex<T>> unwrap_or context_route");
        // These identifiers must survive tokenization intact
        assert!(tokens.contains(&"arc".to_string()),      "arc missing");
        assert!(tokens.contains(&"mutex".to_string()),    "mutex missing");
        assert!(tokens.contains(&"unwrap_or".to_string()),"unwrap_or missing");
        assert!(tokens.contains(&"context_route".to_string()), "context_route missing");
    }

    #[test]
    fn tokenize_strips_stopwords() {
        let tokens = tokenize("the quick brown fox and the lazy dog");
        assert!(!tokens.contains(&"the".to_string()), "stopword 'the' should be removed");
        assert!(!tokens.contains(&"and".to_string()), "stopword 'and' should be removed");
        assert!(tokens.contains(&"quick".to_string()));
        assert!(tokens.contains(&"brown".to_string()));
    }

    #[test]
    fn tokenize_lowercases() {
        let tokens = tokenize("Rust CMHM ContextRuntime");
        assert!(tokens.contains(&"rust".to_string()));
        assert!(tokens.contains(&"cmhm".to_string()));
        assert!(tokens.contains(&"contextruntime".to_string()));
    }

    // --- BM25 basic -----------------------------------------------------------

    #[test]
    fn bm25_reindex_preserves_total_length_and_num_docs() {
        let mut idx = Bm25Index::new();
        idx.insert(1, "alpha beta gamma delta");
        idx.insert(2, "epsilon zeta");
        assert_eq!(idx.num_docs, 2);
        let baseline_total = idx.total_length;

        // Re-index node 1 with different-length text: total_length must be
        // recomputed exactly (remove + re-add), never double-subtracted.
        idx.insert(1, "alpha beta gamma delta epsilon zeta eta theta");
        assert_eq!(idx.num_docs, 2, "num_docs must stay exact after re-index");
        assert_eq!(
            idx.total_length,
            (tokenize("alpha beta gamma delta epsilon zeta eta theta").len()
                + tokenize("epsilon zeta").len()) as u64,
            "total_length must equal sum of current doc lengths"
        );

        // Sanity: re-indexing many times never drives total_length to 0.
        for _ in 0..10 {
            idx.insert(1, "alpha beta gamma delta");
        }
        assert_eq!(idx.num_docs, 2);
        assert!(idx.total_length > 0, "avgdl corruption would zero this out");
        assert_eq!(
            idx.total_length,
            (tokenize("alpha beta gamma delta").len() + tokenize("epsilon zeta").len()) as u64
        );
        let _ = baseline_total;
    }

    #[test]
    fn bm25_empty_query_returns_nothing() {
        let mut idx = Bm25Index::new();
        idx.insert(1, "cargo build failed linker error");
        let results = idx.query("", 10);
        assert!(results.is_empty(), "empty query must return nothing");
    }

    #[test]
    fn bm25_no_match_returns_nothing() {
        let mut idx = Bm25Index::new();
        idx.insert(1, "cargo build failed linker error");
        let results = idx.query("nvidia provider modelx", 10);
        assert!(results.is_empty(), "query with no matching terms must return nothing");
    }

    #[test]
    fn bm25_exact_match_top_ranked() {
        let mut idx = Bm25Index::new();
        idx.insert(1, "cargo build failed linker error missing symbol");
        idx.insert(2, "nvidia provider model config modelx");
        idx.insert(3, "opencode config toml provider nvidia");

        let results = idx.query("cargo build failed", 5);
        assert!(!results.is_empty(), "must return at least one result");
        assert_eq!(results[0].0, 1, "exact match doc must be ranked first");
    }

    #[test]
    fn bm25_partial_match_ranked_by_overlap() {
        let mut idx = Bm25Index::new();
        idx.insert(1, "nvidia provider modelx k2 config model active");
        idx.insert(2, "nvidia provider opencode config"); // 2 matching terms
        idx.insert(3, "cargo test failed compilation error"); // 0 matching terms

        let results = idx.query("nvidia provider", 5);
        assert!(results.len() >= 2);
        // Both doc 1 and doc 2 should appear; doc 3 should not
        let ids: Vec<u64> = results.iter().map(|r| r.0).collect();
        assert!(ids.contains(&1), "doc with nvidia+provider should be in results");
        assert!(ids.contains(&2), "doc with nvidia+provider should be in results");
        assert!(!ids.contains(&3), "doc with no matching terms should be absent");
    }

    #[test]
    fn bm25_remove_works() {
        let mut idx = Bm25Index::new();
        idx.insert(1, "cargo build failed linker error");
        idx.insert(2, "cargo test passed");
        idx.remove(1);

        let results = idx.query("cargo build failed", 5);
        let ids: Vec<u64> = results.iter().map(|r| r.0).collect();
        assert!(!ids.contains(&1), "removed doc must not appear in results");
    }

    #[test]
    fn bm25_reinsertion_updates_not_duplicates() {
        let mut idx = Bm25Index::new();
        idx.insert(1, "cargo build failed");
        idx.insert(1, "cargo build succeeded"); // re-index same node
        assert_eq!(idx.len(), 1, "re-indexing same node must not increase doc count");

        let results = idx.query("cargo build", 5);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn bm25_idf_penalizes_common_terms() {
        let mut idx = Bm25Index::new();
        // "cargo" appears in all 3 docs; "linker" only in doc 1
        idx.insert(1, "cargo build failed linker error missing symbol");
        idx.insert(2, "cargo test passed integration");
        idx.insert(3, "cargo check warnings unused imports");

        let results = idx.query("cargo linker", 5);
        // Doc 1 has the rare term "linker" → should rank highest
        assert_eq!(results[0].0, 1, "doc with rare term 'linker' must rank highest");
    }

    // --- RRF fusion -----------------------------------------------------------

    #[test]
    fn rrf_combines_disjoint_lists() {
        // Dense found doc 1 only; BM25 found doc 2 only
        let dense = vec![(1u64, 0.9f32)];
        let bm25  = vec![(2u64, 10.0f32)];
        let fused = rrf_fuse(&dense, &bm25, 5);
        assert_eq!(fused.len(), 2, "both docs must appear in fused list");
    }

    #[test]
    fn rrf_boosts_consensus_candidate() {
        // Doc 1 is top-ranked in both dense and BM25 → should win after fusion
        let dense = vec![(1u64, 0.95f32), (2u64, 0.70f32), (3u64, 0.50f32)];
        let bm25  = vec![(1u64, 15.0f32), (3u64, 8.0f32),  (2u64, 5.0f32)];
        let fused = rrf_fuse(&dense, &bm25, 3);
        assert_eq!(fused[0].0, 1, "doc ranked #1 in both sources must win after RRF");
    }

    #[test]
    fn rrf_top_k_respected() {
        let dense: Vec<(u64, f32)> = (1..=20).map(|i| (i, 1.0 / i as f32)).collect();
        let bm25:  Vec<(u64, f32)> = (1..=20).map(|i| (i, 1.0 / i as f32)).collect();
        let fused = rrf_fuse(&dense, &bm25, 5);
        assert_eq!(fused.len(), 5, "fused result must respect top_k cap");
    }

    #[test]
    fn rrf_empty_lists_handled() {
        let fused = rrf_fuse(&[], &[], 10);
        assert!(fused.is_empty());
    }

    #[test]
    fn identifier_token_detection() {
        assert!(has_identifier_token("what caused zqxrare0007"));
        assert!(has_identifier_token("unwrap_or panics"));
        assert!(has_identifier_token("ContextRuntime init"));
        assert!(!has_identifier_token("users see outdated assets"));
        assert!(!has_identifier_token("CI fails without real cause"));
    }

    fn idx(docs: &[(u64, &str)]) -> Bm25Index {
        let mut b = Bm25Index::new();
        for (id, t) in docs {
            b.insert(*id, t);
        }
        b
    }

    #[test]
    fn identifier_boost_goes_to_node_containing_token_not_bm25_top() {
        // Doc 2 matches the ordinary query word; doc 1 holds the identifier. Only doc 1 is boosted.
        let b = idx(&[(1, "l1rare042 firmware halted"), (2, "health check too strict caused flapping")]);
        let boost = b.identifier_boost("what caused l1rare042", |_| true);
        assert_eq!(boost.len(), 1);
        assert!(boost[&1] > 0.5, "node containing the identifier is boosted");
        assert!(!boost.contains_key(&2), "a node matching only ordinary words is not boosted");
    }

    #[test]
    fn identifier_boost_uses_index_tokenizer() {
        // Mixed case, underscores and punctuation: query and index spell the identifier differently.
        let b = idx(&[(1, "Error_Code_E1042 raised by ContextRuntime::open"), (2, "unrelated note")]);
        for q in ["ERROR_CODE_e1042", "why ContextRuntime fails", "error_code_e1042?"] {
            let boost = b.identifier_boost(q, |_| true);
            assert!(boost.contains_key(&1), "query {q:?} must hit node 1 via the shared tokenizer");
        }
    }

    #[test]
    fn identifier_boost_respects_wanted_type_and_specificity() {
        let b = idx(&[(1, "l1rare7 state text"), (2, "traced l1rare7 action"), (3, "l1rare7 outcome")]);
        // Only node 1 is the requested type: it alone is boosted, at full strength.
        let boost = b.identifier_boost("l1rare7", |id| id == 1);
        assert_eq!(boost.len(), 1);
        assert!((boost[&1] - LEX_BOOST_MAX).abs() < 1e-6);
        // Three wanted nodes share the boost evenly.
        let all = b.identifier_boost("l1rare7", |_| true);
        assert_eq!(all.len(), 3);
        assert!(all.values().all(|&v| (v - LEX_BOOST_MAX / 3.0).abs() < 1e-6));
        // Too many wanted nodes: not specific, so no boost.
        let b4 = idx(&[(1, "l1rare7 a"), (2, "l1rare7 b"), (3, "l1rare7 c"), (4, "l1rare7 d")]);
        assert!(b4.identifier_boost("l1rare7", |_| true).is_empty());
        // Plain natural language: no identifier token, no boost.
        assert!(b.identifier_boost("users see outdated assets", |_| true).is_empty());
    }

    #[test]
    fn rarest_query_df_none_when_no_overlap() {
        let mut idx = Bm25Index::new();
        idx.insert(1, "cargo build failed linker error");
        assert_eq!(idx.rarest_query_df("nvidia provider modelx"), None);
    }

    #[test]
    fn rarest_query_df_finds_rare_term() {
        let mut idx = Bm25Index::new();
        idx.insert(1, "zqxrare0001 firmware bootloader halted");
        idx.insert(2, "service requests failed timeout");
        idx.insert(3, "service requests failed retry");
        // "zqxrare0001" appears in 1 doc; "service" in 2 docs.
        assert_eq!(idx.rarest_query_df("what caused zqxrare0001"), Some(1));
        assert_eq!(idx.rarest_query_df("service requests"), Some(2));
    }
}
