use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;
use rand::Rng;
use crate::node::{NodeType};
use crate::node_store::{NodeStore, cosine};
/// B1b read-only join types. [`EpisodeStatus`] is re-exported here so callers
/// match on resolution status via `retrieval::EpisodeStatus`.
pub use crate::learn_ledger::{EpisodeStatus, LearnLedger, MetaFilter, SeedEpisode};
use crate::learn_ledger::{BlockedProcess, Disposition};
use crate::edge_store::EdgeStore;
use crate::dense_core::DenseCore;
use crate::EMBED_DIM;

/// Phase 5: Stage 1 keeps at most this many candidates for Stage 2 reranking.
pub const STAGE1_CANDIDATE_POOL: usize = 25;

/// Phase 5: how Stage 1 fuses lexical and dense evidence.
///
/// The classifier is deliberately deterministic and cheap: exact symbols,
/// error codes, file extensions and CLI flags must not be diluted by a small
/// bi-encoder, while natural-language symptom descriptions keep a semantic
/// majority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    /// Exact-symbol / error-shaped query: 0.75 BM25, 0.25 dense.
    LexicalDominant,
    /// Natural-language symptom: 0.40 BM25, 0.60 dense.
    SemanticBalanced,
}

impl QueryMode {
    /// Classify a query. Empty text is semantic (fusion is skipped anyway).
    pub fn classify(query_text: &str) -> Self {
        let q = query_text.trim();
        if q.is_empty() {
            return Self::SemanticBalanced;
        }
        if has_error_code(q)
            || has_file_extension(q)
            || has_cli_flag(q)
            || has_snake_case(q)
            || has_camel_case(q)
            || has_all_caps_symbol(q)
        {
            Self::LexicalDominant
        } else {
            Self::SemanticBalanced
        }
    }

    pub fn bm25_weight(self) -> f32 {
        match self {
            Self::LexicalDominant => 0.75,
            Self::SemanticBalanced => 0.40,
        }
    }

    pub fn dense_weight(self) -> f32 {
        match self {
            Self::LexicalDominant => 0.25,
            Self::SemanticBalanced => 0.60,
        }
    }
}

/// Trim surrounding prose punctuation without eating the dots in file names.
fn trim_query_token(tok: &str) -> &str {
    tok.trim_matches(|c: char| {
        matches!(
            c,
            '"' | '\'' | '`' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
        )
    })
}

/// Tokens split on whitespace and common prose punctuation, but not on `_`,
/// `.`, `/`, `\` or `-` (those carry identifier/path/flag meaning).
fn symbol_tokens(query: &str) -> impl Iterator<Item = &str> {
    query.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | '`' | '<' | '>'
            )
    })
}

const FILE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "py", "sql", "go", "java", "cpp", "cc", "c", "h",
    "hpp", "json", "toml", "yaml", "yml", "md", "sh", "ps1", "css", "html", "lock",
];

fn has_file_extension(query: &str) -> bool {
    query.split_whitespace().any(|raw| {
        let token = trim_query_token(raw).trim_end_matches('.').to_ascii_lowercase();
        FILE_EXTENSIONS.iter().any(|ext| {
            token.len() >= ext.len() + 1
                && token.ends_with(ext)
                && token.as_bytes()[token.len() - ext.len() - 1] == b'.'
        })
    })
}

fn has_cli_flag(query: &str) -> bool {
    query.split_whitespace().any(|raw| {
        let token = trim_query_token(raw);
        if let Some(rest) = token.strip_prefix("--") {
            !rest.is_empty() && rest.chars().any(|c| c.is_ascii_alphanumeric())
        } else if let Some(rest) = token.strip_prefix('-') {
            rest.len() == 1 && rest.chars().all(|c| c.is_ascii_alphabetic())
        } else {
            false
        }
    })
}

fn has_snake_case(query: &str) -> bool {
    symbol_tokens(query).any(|raw| {
        let token = trim_query_token(raw);
        token.contains('_')
            && !token.starts_with('_')
            && !token.ends_with('_')
            && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && token.chars().any(|c| c.is_ascii_alphabetic())
    })
}

fn has_camel_case(query: &str) -> bool {
    symbol_tokens(query).any(|raw| {
        let token = trim_query_token(raw);
        if token.len() < 3
            || !token.chars().all(|c| c.is_ascii_alphanumeric())
            || !token.chars().any(|c| c.is_ascii_lowercase())
        {
            return false;
        }
        let mut previous_lower = false;
        let mut humps = 0;
        for c in token.chars() {
            if previous_lower && c.is_ascii_uppercase() {
                humps += 1;
            }
            previous_lower = c.is_ascii_lowercase();
        }
        humps >= 1
    })
}

fn has_all_caps_symbol(query: &str) -> bool {
    symbol_tokens(query).any(|raw| {
        let token = trim_query_token(raw);
        (token.len() >= 4 || (token.len() >= 3 && token.contains('_')))
            && token.chars().any(|c| c.is_ascii_uppercase())
            && token
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    })
}

fn has_http_context(lower: &str) -> bool {
    [
        "error", "status", "http", "https", "code", "response", "request", "api",
        "server", "client", "fail", "failed", "unauthorized", "forbidden", "timeout",
        "gateway",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

fn has_error_code(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    const STACK_MARKERS: &[&str] = &[
        "panicked at",
        "stack trace",
        "traceback (most recent call last)",
        "segmentation fault",
        "undefined reference to",
        "cannot find module",
        "module not found",
        "npm err!",
        "nullpointerexception",
        "typeerror:",
        "syntaxerror:",
        "referenceerror:",
    ];
    if STACK_MARKERS.iter().any(|marker| lower.contains(marker)) {
        return true;
    }

    let token_count = query.split_whitespace().count();
    for raw in query.split_whitespace() {
        let token = trim_query_token(raw);
        let upper = token.to_ascii_uppercase();
        if upper.starts_with("TS")
            && upper.len() > 2
            && upper[2..].chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
        if upper.starts_with('E')
            && upper.len() >= 4
            && upper[1..].chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
        if upper.starts_with("CS")
            && upper.len() >= 5
            && upper[2..].chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
        if upper.starts_with("ERR_") || upper.starts_with("ERR!") {
            return true;
        }
        if upper.starts_with("0X")
            && upper.len() >= 6
            && upper[2..].chars().all(|c| c.is_ascii_hexdigit())
        {
            return true;
        }
        if token.len() == 3 && token.chars().all(|c| c.is_ascii_digit()) {
            let status: u16 = token.parse().unwrap_or(0);
            if (400..=599).contains(&status) && (has_http_context(&lower) || token_count <= 2) {
                return true;
            }
        }
    }
    false
}

/// A single step in a retrieved causal chain.
#[derive(Debug, Clone)]
pub struct ChainStep {
    pub from_node: u64,
    pub to_node: u64,
    /// Mean weight µ = α/(α+β)
    pub mean_weight: f32,
    /// Total evidence α+β
    pub evidence: f32,
    /// Uncertainty σ²
    pub variance: f32,
    /// Was this an agent-chosen intervention?
    pub is_intervention: bool,
    /// Is this edge highly uncertain? (flags for optional exploration)
    pub is_curious: bool,
    /// Resolved text of the from_node at chain-assembly time.
    /// `None` = node missing from the store (defensive). `Some` with empty
    /// `text` = the node stores no raw text (vector-path learn) — rendered
    /// explicitly as "no text stored", never dropped silently.
    pub from_text: Option<ChainNodeText>,
    /// Resolved text of the to_node. Same semantics as `from_text`.
    pub to_text: Option<ChainNodeText>,
    /// B7 (S-19, refined): what the learn-event ledger says about this step's
    /// endpoints. Decided at assembly from ledger membership + evidence;
    /// rendering partitions on it. Scoring/traversal never read it.
    pub link_kind: LinkKind,
    /// B7: no rendered chain is a verified causal claim. Always false;
    /// surfaced so callers (and agents) cannot mistake a related observation
    /// for causation.
    pub verified_causation: bool,
}

/// B7 (S-19, refined direction): ledger-membership class of a chain step.
///
/// The learn-event ledger records which nodes were learned together, so it
/// says whether a step connects two nodes of one experience or two different
/// ones — something edge evidence (2.0 both for fresh-legitimate and
/// spurious links) cannot tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// Endpoints share a retained learn episode: always shown.
    SameExperience,
    /// Endpoints have retained episodes but share none, and edge evidence is
    /// prior-only: hidden in human halves with a visible count marker; kept
    /// in JSON with the kind label (lossless machine half).
    CrossExperienceUnverified,
    /// Endpoints share no episode but the edge was reinforced: shown with a
    /// "learned association" label.
    CrossExperienceLearned,
    /// Either endpoint has no retained episode (pre-B1b, evicted, or never
    /// recorded): shown with an "unverified" label. Unknown means visible —
    /// losing data silently is worse than showing a doubtful link.
    Unknown,
}

impl LinkKind {
    /// Machine-readable label for JSON (additive; existing keys untouched).
    pub fn label(self) -> &'static str {
        match self {
            LinkKind::SameExperience => "same_experience",
            LinkKind::CrossExperienceUnverified => "cross_unverified",
            LinkKind::CrossExperienceLearned => "cross_learned",
            LinkKind::Unknown => "unknown",
        }
    }
}

/// B7 (S-19, owner-accepted rider): prior-only evidence tolerance.
///
/// Predicate is `evidence <= 2.0 + PRIOR_ONLY_EVIDENCE_EPS`, NEVER `== 2.0`.
/// Temporal forgetting decays α/β toward a 0.01 floor (`engine.rs`
/// update_causal_edge), NOT the 1.0 prior — an aged never-reinforced edge
/// drifts from 2.0 toward 0.02, so exact equality would miss it; float
/// equality is fragile anyway. Starts at 1e-4; VALIDATED BY TESTS (a freshly
/// reinforced edge with evidence clearly above the band must render).
pub const PRIOR_ONLY_EVIDENCE_EPS: f32 = 1e-4;

/// B7: true when an edge carries no observations beyond the starting prior.
#[inline]
pub fn is_prior_only_evidence(evidence: f32) -> bool {
    evidence <= 2.0 + PRIOR_ONLY_EVIDENCE_EPS
}

/// B7: classify one chain step from ledger membership + edge evidence.
/// Pure function (deterministic; unit-tested directly).
pub fn classify_link(
    ledger: &crate::learn_ledger::LearnLedger,
    from: u64,
    to: u64,
    evidence: f32,
) -> LinkKind {
    match ledger.shares_episode(from, to) {
        Some(true) => LinkKind::SameExperience,
        Some(false) => {
            if is_prior_only_evidence(evidence) {
                LinkKind::CrossExperienceUnverified
            } else {
                LinkKind::CrossExperienceLearned
            }
        }
        None => LinkKind::Unknown,
    }
}

/// B7: visible count marker for hidden cross-experience prior-only steps.
/// Never a silent gap: the count always renders.
#[inline]
pub fn unverified_links_hidden_marker(hidden: usize) -> String {
    let noun = if hidden == 1 { "link" } else { "links" };
    format!("[{hidden} unverified {noun} hidden — prior-only cross-experience steps]")
}

/// Resolved human-readable text for one node, snapshotted at retrieval time.
///
/// B1: retrieval must reproduce what was learned. `text` is the node's stored
/// raw text (what BM25 indexes); `timestamp` is the node's creation time in
/// unix seconds.
#[derive(Debug, Clone)]
pub struct ChainNodeText {
    pub text: String,
    pub timestamp: i64,
}

/// B1b: top-3 scored candidate text, in scored order (no rerank).
///
/// Read-only join: `node_id` follows `scored_candidates` order exactly;
/// `text`/`timestamp` are resolved live from the store at retrieval time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CandidateText {
    pub node_id: u64,
    pub text: String,
    pub timestamp: i64,
    /// Phase 5: indexed trigger signature (empty on legacy nodes).
    #[serde(default)]
    pub trigger_signature: String,
    /// Phase 5: decision payload (never indexed).
    #[serde(default)]
    pub decision_payload: String,
    /// Phase 5: typed disposition.
    #[serde(default)]
    pub disposition: Disposition,
    /// Phase 5: structured blocked-process payload.
    #[serde(default)]
    pub blocked: Option<BlockedProcess>,
}

/// Maximum characters of node text rendered in the human-readable
/// explanation. Longer text is cut with a visible `…[+N chars]` marker; the
/// structured fields always carry the full string (lossless machine half).
pub const TEXT_RENDER_CAP: usize = 120;

/// B1c: render a unix-seconds timestamp as `YYYY-MM-DD` (UTC) for the
/// human-readable halves. Std-only on purpose — no `chrono`/`time`/`jiff`
/// dependency (B1c approval condition). Uses Howard Hinnant's days-to-civil
/// algorithm; negative inputs degrade to their calendar date, never panic.
pub fn fmt_learned_date(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

/// Pre-declared authority-margin threshold for trust-compromised runner-up abstention.
/// Derived from composite scoring mathematics under hash-128d in Protocol v1.9 Section 14.5.
pub const TRUST_COMPROMISE_DELTA: f32 = 0.10;

/// Machine-readable classification of why two candidates triggered conflict detection.
///
/// Score proximity alone is not semantic contradiction — this enum lets callers
/// distinguish the nature of the conflict and act accordingly.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConflictType {
    /// Explicit logical contradiction: candidates share the same non-empty `assertion_key`
    /// or entity identifier with opposing epistemic polarity (+1 vs -1) or incompatible values.
    LogicalContradiction,
    /// Top-2 candidates have near-equal retrieval scores. Neither provenance
    /// nor authority clearly distinguishes them. The engine cannot determine
    /// which record is more reliable without additional context.
    ScoreAmbiguity,
    /// Both candidates have near-equal scores AND opposing provenance signals:
    /// one is `"corrected"` or `"user_explicit"` (high trust) while the other
    /// is `"contradicted"` (low trust). This indicates a deliberate correction
    /// exists alongside the original contradicted claim.
    AuthorityPolarity,
    /// One candidate's `superseded_by` chain intersects the other: a node that
    /// was supposed to be replaced still scores within the conflict threshold of
    /// its successor. Indicates the supersession did not fully remove the old record
    /// from the retrieval candidate pool.
    SupersessionConflict,
}


/// A candidate node that participated in conflict detection.
/// Populated in `RetrievalResult::conflict_candidates` when `conflict_detected` is true.
#[derive(Debug, Clone)]
pub struct ConflictCandidate {
    pub node_id:      u64,
    pub score:        f32,
    pub authority:    f32,
    pub provenance:   String,
    /// Machine-readable classification of the conflict type.
    /// All candidates in a single conflict record share the same `conflict_type`.
    pub conflict_type: ConflictType,
}

/// Full retrieval result returned to the caller.
#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub causal_chain: Vec<ChainStep>,
    /// Dense context vector (blended C*q output), EMBED_DIM floats.
    pub dense_context: [f32; EMBED_DIM],
    /// Text summary of the chain (human-readable).
    pub explanation: String,
    /// Node ID selected as the causal-chain seed (None if store was empty).
    /// Observability field for integration tests and diagnostics.
    pub seed_id: Option<u64>,
    /// Resolved text of the seed node (B1). None when no seed is returned.
    pub seed_text: Option<ChainNodeText>,
    /// NodeType of the seed node (None if store was empty).
    /// Allows tests to verify role-scoping invariants without inspecting the engine's
    /// internal node store through a separate round-trip.
    pub seed_node_type: Option<NodeType>,
    /// `node.authority` of the selected seed node. 0.5 if store was empty.
    /// Exposed for diagnostics and MCP response surfacing.
    pub seed_authority: f32,
    /// `node.provenance` of the selected seed node. Empty string if store was empty.
    pub seed_provenance: String,
    /// True when the top-2 scored candidates have near-equal scores AND differ in provenance
    /// or authority, suggesting conflicting memories for this query.
    /// When `abstained` is also true, `causal_chain` is empty and the engine returned no result.
    pub conflict_detected: bool,
    /// Structured top-2 candidates when `conflict_detected` is true.
    /// Contains score, authority, and provenance for each candidate — not just IDs.
    /// Empty when `conflict_detected` is false.
    pub conflict_candidates: Vec<ConflictCandidate>,
    /// True when `conflict_detected` and `abstain_on_conflict` config flag is set.
    /// When true, `causal_chain` is empty — the engine refused to return a result
    /// rather than risk returning a contested record. `conflict_candidates` still
    /// populated with the competing nodes for caller inspection.
    pub abstained: bool,
    /// Passive diagnostic instrumentation: candidate IDs returned by nearest() / nearest_by_type()
    /// before any filtering (Stage 0: RAW_NN).
    pub raw_nn_candidates: Vec<u64>,
    /// Passive diagnostic instrumentation: candidate IDs surviving Step 2a supersession
    /// filtering (Stage 1: POST_FILTER).
    pub post_filter_candidates: Vec<u64>,
    /// Passive diagnostic instrumentation: composite-scored candidates sorted descending
    /// with their scores (Stage 2: SCORED).
    pub scored_candidates: Vec<(u64, f32)>,
    /// B1b read-only join: top-3 of `scored_candidates`, in scored order, with
    /// live-resolved text. Empty when there are no scored candidates.
    pub candidate_texts: Vec<CandidateText>,
    /// B1b read-only join: most-recent learn triple for the seed node
    /// (most-recent wins on dedup-shared nodes). `None` when the seed has no
    /// retained episode — see `seed_episode_status` for why.
    pub seed_episode: Option<SeedEpisode>,
    /// B1b: why `seed_episode` is present or absent. `Found` = triple returned;
    /// `Evicted` = "episode record evicted"; `NoTriple` = pre-B1b
    /// "no triple recorded (learned before B1b)".
    pub seed_episode_status: EpisodeStatus,
}

/// Internal Dijkstra state.
#[derive(Debug)]
struct DijkstraState {
    cost: f32,      // negative log of product of sampled weights (minimize = maximize weight product)
    node_id: u64,
    depth: usize,
    path: Vec<(u64, u64)>, // (src, tgt) pairs
}

impl PartialEq for DijkstraState {
    fn eq(&self, other: &Self) -> bool { self.cost == other.cost }
}
impl Eq for DijkstraState {}
impl PartialOrd for DijkstraState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for DijkstraState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap: lower cost = higher priority → reverse
        other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}

/// Thompson-sampled Dijkstra retrieval engine.
///
/// Algorithm:
/// 1. Project query + compute dense context.
/// 2. Seed selection: top-8 NN (role-scoped when `seed_type` is `Some`).
///    - 2a: Filter: remove candidates where `node.superseded_by.is_some()`.
///    - 2b: Conflict detection on post-filter pool using persisted node metadata
///          (assertion_key, polarity, provenance, authority). Detection precedes scoring
///          so contradicted D- nodes suppressed by the authority weight are still visible.
///          Signal precedence: LogicalContradiction → SupersessionConflict → ProvenancePair → AdversarialPair.
///    - 2b½: Metadata filter (B6b, only when active): keep candidates whose
///          most-recent episode metadata matches every present clause.
///          Runs after 2b (detection stays fully sighted) and before 2c.
///          Ledgerless candidates are excluded under an active filter.
///          Empty pool → explicit abstention naming the filter.
///    - 2c: Score each eligible candidate. Relevance dominates; authority is a
///          bounded subordinate term (α = 0.25, measured):
///          `query_weight * cos(q, node) + context_weight * cos(dense_ctx, node)
///           + authority_weight * node.authority + BANDIT_WEIGHT * bandit`
///          Nodes marked "contradicted" are ineligible: they stay in the pool
///          for 2b detection but are never scored, so they cannot be seeds.
///    - 2d: Seed = highest-scored candidate. Exact score ties are broken by
///          node authority, then node id for determinism (E7 fail-safe).
///    - 2e/2f: Abstain if conflict detected and `abstain_on_conflict` is set.
///    - `emotional_tag` is affective salience only; it does NOT appear in the score.
///    - No recency term; use `superseded_by` for staleness handling.
/// 3. Dijkstra with Thompson-sampled edge costs, bounded to `max_depth` hops.
/// 4. Assemble path with mean weights, evidence, variance.
pub fn retrieve(
    query: &[f32; EMBED_DIM],
    model_id: &str,
    node_store: &NodeStore,
    edge_store: &EdgeStore,
    dense_core: &DenseCore,
    rng: &mut impl Rng,
    max_depth: usize,
    query_weight: f32,
    context_weight: f32,
    curiosity_threshold: f32,
    // Role-scoped seed selection. Some(State) restricts candidates to State nodes.
    // None falls back to the unscoped nearest() scan (backward-compat library path).
    // MCP tool handlers always pass Some(node_type) — never None.
    seed_type: Option<NodeType>,
    // Bounded authority term in seed scoring (default 0.25). Subordinate to
    // relevance: measured on the phase-3 approval corpus this weight restores
    // baseline recall (G1 42/43, MRR 0.988) while contradicted provenance stays
    // structurally ineligible (G4 0/43) and an irrelevant trusted node cannot
    // displace the relevance winner (property holds to α ≈ 0.5).
    authority_weight: f32,
    // Score gap below which two candidates trigger conflict_detected = true.
    // Deprecated-in-place as of Protocol v1.9: the score-gap proximity check has been
    // deleted from the decision path. Retained for API/schema compatibility.
    _conflict_detection_threshold: f32,
    // When true and conflict_detected, return empty chain + abstained=true instead of
    // returning the contested winner. Callers still receive conflict_candidates.
    abstain_on_conflict: bool,
    query_text: &str,
    bm25: &crate::bm25::Bm25Index,
    // R3: Seed pool size — number of NN candidates to retrieve before scoring.
    // Default 20 (was hardcoded 8). Tunable via Config::retrieval_seed_pool_size.
    seed_pool_size: usize,
    // B13: fused-pool truncation cap (the rrf_fuse pool cap; was hardcoded 20).
    // Tunable via Config::retrieval_fusion_cap; default 20 = unchanged behavior.
    fusion_cap: usize,
    // B1b: learn-event ledger for the read-only episode join. Scoring,
    // filtering, and ordering above never read it; the join below only
    // resolves text/triples for already-ranked candidates.
    ledger: &LearnLedger,
    // B6b: optional metadata filter over episode classification. Applied
    // AFTER conflict detection (Step 2b stays fully sighted — safety first)
    // and BEFORE scoring: a user query constraint, not a safety rule.
    // None or inactive = byte-identical unfiltered behavior.
    meta_filter: Option<&MetaFilter>,
    // Stage 2: optional cross-encoder reranker applied to the top fused
    // candidates before seed/chain selection. None keeps the default Stage 1
    // path byte-identical.
    stage2: Option<&mut dyn crate::reranker::Stage2Reranker>,
) -> RetrievalResult {
    // Step 1: Dense context
    let dense_ctx = dense_core.query(query);

    // Helper closure for empty results (avoids repeating all fields)
    // B1b: empty sites carry an empty join (no candidates, no triple).
    macro_rules! empty_result {
        ($explanation:expr) => {
            RetrievalResult {
                causal_chain:         vec![],
                dense_context:        dense_ctx,
                explanation:          $explanation.to_string(),
                seed_id:              None,
                seed_text:            None,
                seed_node_type:       None,
                seed_authority:       0.5,
                seed_provenance:      String::new(),
                conflict_detected:    false,
                conflict_candidates:  vec![],
                abstained:            false,
                raw_nn_candidates:    vec![],
                post_filter_candidates: vec![],
                scored_candidates:    vec![],
                candidate_texts:      vec![],
                seed_episode:         None,
                seed_episode_status:  EpisodeStatus::NoTriple,
            }
        };
    }

    // Step 2: Seed selection — top-8 NN by cosine (same model_id only — N3)
    // When seed_type is Some(t), restrict candidates to nodes of that role.
    // We request 8 to have a meaningful pool for conflict detection and supersession filtering.
    let raw_candidates = match seed_type {
        Some(t) => node_store.nearest_by_type(query, model_id, seed_pool_size, t),
        None    => node_store.nearest(query, model_id, seed_pool_size),
    };
    // BM25 lexical candidates fused with the dense arm when query_text is provided.
    // Empty query_text (legacy/vector paths) skips fusion — dense-only behavior unchanged.
    //
    // R1 FIX: Snapshot dense cosines BEFORE fusion so the scorer receives a real cosine
    // value (∈ [−1,1]) for every candidate, not the RRF rank score (~0.016).
    // BM25-only candidates that have no dense hit receive a computed cosine (or 0.0).
    //
    // Phase 5: the classifier picks the BM25/dense mix per query. Stage 1 returns a
    // weighted relevance score (BM25 min-max normalized, dense cosine clamped to
    // [0,1]) plus the exact-identifier boost, then truncates to STAGE1_CANDIDATE_POOL
    // for the Stage 2 reranker. The final scorer below consumes this relevance
    // directly for text queries; the dense-only path keeps the legacy query_weight
    // scaling.
    let query_mode = QueryMode::classify(query_text);
    let stage1_bm25_weight = query_mode.bm25_weight();
    let stage1_dense_weight = query_mode.dense_weight();
    let dense_cosines: std::collections::HashMap<u64, f32> =
        raw_candidates.iter().map(|&(id, cos)| (id, cos)).collect();

    let raw_candidates = if query_text.is_empty() {
        raw_candidates
    } else {
        // Type is pool MEMBERSHIP, not ranking: the dense arm is already type-scoped
        // (nearest_by_type), so the BM25 arm must be too, otherwise a lexically matching
        // Action/Outcome node enters the pool and can be returned as the seed of a State
        // request. Over-fetch because the index holds state, action and outcome texts and the
        // filter drops the other types; the list handed to fusion is still capped at 50.
        let bm25_scored: Vec<(u64, f32)> = {
            let all = bm25.query(query_text, 150);
            match seed_type {
                Some(t) => all
                    .into_iter()
                    .filter(|(id, _)| node_store.get(*id).map(|n| n.node_type == t).unwrap_or(false))
                    .take(50)
                    .collect(),
                None => all.into_iter().take(50).collect(),
            }
        };
        if bm25_scored.is_empty() {
            raw_candidates
        } else {
            // Min-max normalize BM25 scores across this query's candidates so the
            // lexical weight is comparable to a cosine. A single candidate is 1.0.
            let (mut bm25_min, mut bm25_max) = (f32::INFINITY, f32::NEG_INFINITY);
            for &(_, score) in &bm25_scored {
                bm25_min = bm25_min.min(score);
                bm25_max = bm25_max.max(score);
            }
            let bm25_range = bm25_max - bm25_min;
            let bm25_norm: std::collections::HashMap<u64, f32> = bm25_scored
                .iter()
                .map(|&(id, score)| {
                    // A lone candidate carries all the lexical evidence: 1.0, not 0/0.
                    let norm = if bm25_range.abs() < 1e-6 {
                        1.0
                    } else {
                        (score - bm25_min) / bm25_range
                    };
                    (id, norm)
                })
                .collect();

            // Fuse for pool membership. RRF ranks decide who enters the pool; the
            // weighted Stage 1 score below decides the order inside it.
            let bm25_candidates: Vec<(u64, f32)> =
                bm25_scored.iter().map(|&(id, _)| (id, 0.0)).collect();
            let boosts = bm25.identifier_boost(query_text, |id| match seed_type {
                Some(t) => node_store.get(id).map(|n| n.node_type == t).unwrap_or(false),
                None => true,
            });
            let mut fused: Vec<(u64, f32)> =
                crate::bm25::rrf_fuse(&raw_candidates, &bm25_candidates, fusion_cap)
                    .into_iter()
                    .map(|(id, _rrf_score)| {
                        let cos = dense_cosines.get(&id).copied().unwrap_or_else(|| {
                            node_store.get(id)
                                .filter(|n| n.model_id == model_id && n.embedding.len() == query.len())
                                .map(|n| cosine(query, &n.embedding))
                                .unwrap_or(0.0)
                        });
                        let dense_component = cos.clamp(0.0, 1.0);
                        let lexical_component = bm25_norm.get(&id).copied().unwrap_or(0.0);
                        let stage1 = stage1_dense_weight * dense_component
                            + stage1_bm25_weight * lexical_component
                            + boosts.get(&id).copied().unwrap_or(0.0);
                        (id, stage1)
                    })
                    .collect();
            fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
            fused.truncate(STAGE1_CANDIDATE_POOL);
            fused
        }
    };
    if raw_candidates.is_empty() {
        return empty_result!("No memories yet.");
    }
    // Stage 0: RAW_NN diagnostic snapshot (passive, unmutated)
    let raw_nn_candidates: Vec<u64> = raw_candidates.iter().map(|(id, _)| *id).collect();

    // Step 2a: Filter out superseded candidates, and (when the retrieval is
    // role-scoped) any node whose NodeType differs from the requested seed type.
    //
    // The role filter matters most on the fused path: BM25 indexes action and
    // outcome text (expansion), so a query phrased like "how did I fix X"
    // can lexically match an Action node. Role-scoped retrievals must never
    // seed it — lexical evidence may inform the pool, not override the role
    // contract. The dense path was already role-scoped pre-fusion; this
    // closes the BM25 bypass.
    let candidates: Vec<(u64, f32)> = raw_candidates.into_iter()
        .filter(|(id, _)| {
            let keep_role = match seed_type {
                Some(t) => node_store.get(*id).map_or(false, |n| n.node_type == t),
                None    => true,
            };
            keep_role && node_store.get(*id).map_or(false, |n| n.superseded_by.is_none())
        })
        .collect();

    // Stage 1: POST_FILTER diagnostic snapshot (passive, unmutated)
    let post_filter_candidates: Vec<u64> = candidates.iter().map(|(id, _)| *id).collect();

    if candidates.is_empty() {
        return RetrievalResult {
            causal_chain:         vec![],
            dense_context:        dense_ctx,
            explanation:          "All candidate nodes are superseded. No active seed available.".to_string(),
            seed_id:              None,
            seed_text:            None,
            seed_node_type:       None,
            seed_authority:       0.5,
            seed_provenance:      String::new(),
            conflict_detected:    false,
            conflict_candidates:  vec![],
            abstained:            false,
            raw_nn_candidates,
            post_filter_candidates,
            scored_candidates:    vec![],
            candidate_texts:      vec![],
            seed_episode:         None,
            seed_episode_status:  EpisodeStatus::NoTriple,
        };
    }

    // Step 2b: Conflict detection on post-filter pool (POOL-BASED — v2.0 §14.7 item 7).
    //
    // Operates on `candidates` (post-supersession-filter pool), NOT on scored top-2.
    // This breaks the circular dependency where the authority weight (0.8 * clamped_authority)
    // suppresses contradicted D- nodes below scored top-2, making detection blind to them.
    //
    // Signal precedence (first match governs conflict_type):
    //   1. LogicalContradiction: any pair in pool with identical non-empty assertion_key + opposing polarity
    //   2. SupersessionConflict: defensive — any superseded node that survived Step 2a
    //   3. ProvenancePair: any pair with one HIGH_TRUST + one LOW_TRUST provenance
    //      — fires unconditionally (v2.0: TrustCompromised clamped-gap check DELETED)
    //   4. AdversarialPair: any pair, both HIGH_TRUST, pos_prov == "corrected", neg_auth > pos_auth
    //
    // TRUST_COMPROMISE_DELTA is retained as a named deprecated-in-place constant (not read here).
    // Scores are not available yet; ConflictCandidate::score will be backfilled after Step 2c.

    const HIGH_TRUST: &[&str] = &["corrected", "user_explicit"];
    const LOW_TRUST:  &[&str] = &["contradicted"];

    // Collect pool node metadata once — avoid redundant node_store lookups in the scan.
    struct PoolEntry<'a> {
        id:            u64,
        prov:          &'a str,
        auth:          f32,
        key:           Option<&'a str>,
        pol:           Option<i8>,
        // superseded_by removed (L-06): Step 2a already filters superseded nodes before
        // pool_meta is built, so this field was always None here. Removed with M-06 dead check.
    }
    let pool_meta: Vec<PoolEntry<'_>> = candidates.iter().filter_map(|(id, _)| {
        node_store.get(*id).map(|n| PoolEntry {
            id:            *id,
            prov:          n.provenance.as_str(),
            auth:          n.authority,
            key:           n.assertion_key.as_deref(),
            pol:           n.polarity,
        })
    }).collect();

    let (detect_conflict_type, detect_pair, detect_abstain): (Option<ConflictType>, Option<(u64, u64)>, bool) = 'detect: {
        // 1. LogicalContradiction — scan all pairs for same non-empty key + opposing polarity
        {
            for i in 0..pool_meta.len() {
                for j in (i + 1)..pool_meta.len() {
                    let a = &pool_meta[i];
                    let b = &pool_meta[j];
                    if let (Some(ka), Some(kb), Some(pa), Some(pb)) = (a.key, b.key, a.pol, b.pol) {
                        if ka == kb && (pa as i16) * (pb as i16) < 0 {
                            break 'detect (Some(ConflictType::LogicalContradiction), Some((a.id, b.id)), true);
                        }
                    }
                }
            }
        }
        // 2. SupersessionConflict — M-06 note: this check can never fire.
        //    Step 2a (line ~254) already filters out every node where superseded_by.is_some().
        //    pool_meta is built from candidates (post-2a), so e.superseded_by is always None here.
        //    The ConflictType::SupersessionConflict variant is kept for API compatibility.
        // 3. ProvenancePair: HIGH_TRUST + LOW_TRUST in pool — guarded by assertion_key conflict (R4).
        //    R4 INTENT: Suppress false abstention when competing nodes are about DIFFERENT topics.
        //    RULE: Fire unless both nodes carry non-empty assertion_keys that are explicitly different.
        //    - node A has key "db_pool_size", node B has no key → FIRE (topic unknown, safety first)
        //    - node A has key "db_pool_size", node B has key "db_pool_size" → FIRE (same topic)
        //    - node A has key "db_pool_size", node B has key "tls_ciphers" → SUPPRESS (different topics)
        {
            for i in 0..pool_meta.len() {
                for j in (i + 1)..pool_meta.len() {
                    let a = &pool_meta[i];
                    let b = &pool_meta[j];
                    let a_high = HIGH_TRUST.contains(&a.prov);
                    let b_low  = LOW_TRUST.contains(&b.prov);
                    let b_high = HIGH_TRUST.contains(&b.prov);
                    let a_low  = LOW_TRUST.contains(&a.prov);
                    if (a_high && b_low) || (b_high && a_low) {
                        // R4: Suppress ONLY if both nodes have explicit non-empty, different keys.
                        let key_conflict = a.key.is_some() && b.key.is_some()
                            && !a.key.unwrap_or("").is_empty()
                            && !b.key.unwrap_or("").is_empty()
                            && a.key != b.key;
                        if !key_conflict {
                            break 'detect (Some(ConflictType::AuthorityPolarity), Some((a.id, b.id)), true);
                        }
                    }
                }
            }
        }
        // 4. AdversarialPair: both HIGH_TRUST, one "corrected" + one "user_explicit", neg_auth > pos_auth.
        //    No key guard — adversarial attacks may deliberately use a different key to evade detection.
        //    The corrected+user_explicit HIGH+HIGH pattern is always suspicious regardless of keys.
        {
            for i in 0..pool_meta.len() {
                for j in (i + 1)..pool_meta.len() {
                    let a = &pool_meta[i];
                    let b = &pool_meta[j];
                    if !HIGH_TRUST.contains(&a.prov) || !HIGH_TRUST.contains(&b.prov) {
                        continue;
                    }
                    let is_adv = (a.prov == "corrected" && b.prov == "user_explicit" && b.auth > a.auth)
                        || (b.prov == "corrected" && a.prov == "user_explicit" && a.auth > b.auth);
                    if is_adv {
                        break 'detect (Some(ConflictType::AuthorityPolarity), Some((a.id, b.id)), true);
                    }
                }
            }
        }
        (None, None, false)
    };

    let conflict_detected = detect_conflict_type.is_some();

    // B6b (S-19 refined note): metadata filter over episode classification.
    // Runs AFTER conflict detection (2b stays fully sighted) and BEFORE
    // scoring (2c): this is a user query constraint, not a safety rule, so it
    // must not blind detection — but it must shape what competes.
    // Semantics: a candidate survives iff its MOST-RECENT episode carries
    // metadata matching every present clause. Ledgerless candidates (pre-B6b,
    // evicted, WAL-replayed tail without a meta record) are EXCLUDED under an
    // active filter — documented: a filter cannot match what was never
    // classified. Inactive/None filter = no-op (unfiltered behavior identical).
    let mut candidates = candidates;
    if let Some(f) = meta_filter {
        if !f.is_inactive() {
            candidates.retain(|(id, _)| {
                match ledger.resolve_node(*id) {
                    (EpisodeStatus::Found, Some(ep), _) => ledger
                        .meta_for_seq(ep.seq)
                        .map(|m| f.matches(m))
                        .unwrap_or(false),
                    _ => false,
                }
            });
            if candidates.is_empty() {
                // Filter matched nothing: explicit empty result (never index
                // scored[0]). Conflict state is preserved as detected — the
                // pair is named with 0.0 scores (scoring never ran).
                let conflict_candidates: Vec<ConflictCandidate> =
                    if let (Some(ctype), Some((id_a, id_b))) = (&detect_conflict_type, detect_pair) {
                        [id_a, id_b].iter().map(|id| {
                            let node = node_store.get(*id);
                            ConflictCandidate {
                                node_id:      *id,
                                score:        0.0,
                                authority:    node.map(|n| n.authority).unwrap_or(0.5),
                                provenance:   node.map(|n| n.provenance.clone()).unwrap_or_default(),
                                conflict_type: ctype.clone(),
                            }
                        }).collect()
                    } else {
                        vec![]
                    };
                return RetrievalResult {
                    causal_chain:        vec![],
                    dense_context:       dense_ctx,
                    explanation:         "No memories match the metadata filter."
                        .to_string(),
                    seed_id:             None,
                    seed_text:           None,
                    seed_node_type:      None,
                    seed_authority:      0.0,
                    seed_provenance:     String::new(),
                    conflict_detected,
                    conflict_candidates,
                    abstained:           true,
                    raw_nn_candidates,
                    post_filter_candidates,
                    scored_candidates:    vec![],
                    candidate_texts:      vec![],
                    seed_episode:         None,
                    seed_episode_status:  EpisodeStatus::NoTriple,
                };
            }
        }
    }

    // Step 2c: Relevance-primary scoring of the eligible post-filter pool.
    //
    // Nodes explicitly marked "contradicted" remain in `candidates` so conflict
    // detection above can see them, but they are structurally ineligible to be
    // returned as a seed. This makes the safety posture fail-closed rather than
    // relying on scoring to make unsafe returns unlikely.
    //
    // Phase 5: for text queries, `cos` entering this loop is already the
    // weighted Stage 1 relevance (BM25/dense mix + exact-identifier boost), so
    // it is used directly. For dense-only queries it is the raw cosine and
    // keeps the legacy query_weight scaling.
    // score = relevance
    //       + context_weight * cos(dense_ctx, node)
    //       + authority_weight * node.authority   <- bounded, α=0.25; see gate sweep
    //       + BANDIT_WEIGHT * node.thompson_draw(rng)  <- H-03 fix: bandit exploration
    //
    // The Thompson draw theta ∈ (0,1) is sampled from Beta(alpha, beta).
    // New/untested nodes draw from Beta(1,1) = Uniform — exploration bonus.
    // Weight 0.10 keeps exploration bounded relative to cosine similarity.
    const BANDIT_WEIGHT: f32 = 0.10;
    let mut scored: Vec<(u64, f32)> = candidates.iter().filter_map(|(id, cos)| {
        let relevance = if query_text.is_empty() {
            query_weight * cos
        } else {
            *cos
        };
        let score = match node_store.get(*id) {
            Some(node) => {
                if node.provenance == "contradicted" {
                    return None;
                }
                // min-len cosine for mixed-dim safety during embedder transition
                let min_len = dense_ctx.len().min(node.embedding.len());
                let dense_cos = cosine(&dense_ctx[..min_len], &node.embedding[..min_len]);
                // H-03 fix: bandit exploration. Only apply when node has actual feedback
                // (alpha>1 or beta>1). Prior-only Beta(1,1) nodes get 0 — no signal yet.
                let bandit_draw = if node.bandit_alpha > 1.0 || node.bandit_beta > 1.0 {
                    node.thompson_draw(rng)
                } else {
                    0.0 // uniform prior = no exploitation signal
                };
                relevance
                    + context_weight * dense_cos
                    + authority_weight * node.authority
                    + BANDIT_WEIGHT * bandit_draw
            }
            None => relevance,
        };
        Some((*id, score))
    }).collect();

    // Sort descending by score. Exact ties (equal relevance) are broken by
    // node authority — it can only settle equals, never move a lower-relevance
    // node above a higher-relevance one (E7 fail-safe). Node id is the final
    // key so ordering stays fully deterministic.
    scored.sort_by(|(id_a, sa), (id_b, sb)| {
        sb.partial_cmp(sa)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                let auth_a = node_store.get(*id_a).map(|n| n.authority).unwrap_or(0.0);
                let auth_b = node_store.get(*id_b).map(|n| n.authority).unwrap_or(0.0);
                auth_b.partial_cmp(&auth_a).unwrap_or(Ordering::Equal)
            })
            .then_with(|| id_a.cmp(id_b))
    });

    // Stage 2 (opt-in): rerank the fused head with the cross-encoder. This is
    // the only mutation of `scored` after sorting; every downstream consumer
    // (candidate_texts, conflict scores, seed, chain) sees the reranked order.
    // The reranked head carries calibrated probabilities in [0, 1]; the
    // untouched tail keeps Stage 1 fused scores, so the two blocks are not
    // numerically comparable.
    if let Some(reranker) = stage2 {
        const STAGE2_POOL: usize = 25;
        let pool: Vec<(u64, String)> = scored
            .iter()
            .take(STAGE2_POOL)
            .filter_map(|(id, _)| {
                node_store.get(*id).map(|node| {
                    let text = if node.trigger_signature.is_empty() {
                        node.text.clone()
                    } else {
                        node.trigger_signature.clone()
                    };
                    (*id, text)
                })
            })
            .collect();
        if !pool.is_empty() {
            match crate::reranker::rerank_top_k(reranker, query_text, &pool, pool.len()) {
                Ok(ranked) => {
                    let reranked: std::collections::HashSet<u64> =
                        ranked.iter().map(|(id, _)| *id).collect();
                    let mut next = ranked;
                    for (id, score) in scored.iter().take(STAGE2_POOL) {
                        if !reranked.contains(id) {
                            next.push((*id, *score));
                        }
                    }
                    next.extend(scored.iter().skip(STAGE2_POOL).copied());
                    scored = next;
                }
                Err(err) => {
                    tracing::warn!("stage2 reranker failed; keeping Stage 1 order: {err}");
                }
            }
        }
    }

    // Stage 2: SCORED diagnostic snapshot (passive, unmutated).
    let scored_candidates: Vec<(u64, f32)> = scored.clone();

    // B1b read-only join (candidate half): top-3 of `scored` in scored order.
    // `scored` is final here — this maps, never sorts, filters, or rescores.
    let candidate_texts: Vec<CandidateText> = scored.iter().take(3).map(|(id, _)| {
        let n = node_store.get(*id);
        CandidateText {
            node_id: *id,
            text: n.map(|x| x.text.clone()).unwrap_or_default(),
            timestamp: n.map(|x| x.timestamp).unwrap_or(0),
            trigger_signature: n.map(|x| x.trigger_signature.clone()).unwrap_or_default(),
            decision_payload: n.map(|x| x.decision_payload.clone()).unwrap_or_default(),
            disposition: n.map(|x| x.disposition).unwrap_or_default(),
            blocked: n.and_then(|x| x.blocked.clone()),
        }
    }).collect();

    // Step 2e (moved before the empty check): assemble conflict_candidates
    // from the detected pair so BOTH return paths identify contradicted
    // candidates. Backfill scores from the scored output; filtered-out nodes
    // get 0.0 (they never competed, but callers still see who they are).
    let conflict_candidates: Vec<ConflictCandidate> = if let (Some(ctype), Some((id_a, id_b))) = (&detect_conflict_type, detect_pair) {
        let make_cand = |id: u64| {
            let score = scored.iter()
                .find(|(sid, _)| *sid == id)
                .map(|(_, s)| *s)
                .unwrap_or(0.0);
            let node = node_store.get(id);
            ConflictCandidate {
                node_id:      id,
                score,
                authority:    node.map(|n| n.authority).unwrap_or(0.5),
                provenance:   node.map(|n| n.provenance.clone()).unwrap_or_default(),
                conflict_type: ctype.clone(),
            }
        };
        let mut cands = vec![make_cand(id_a), make_cand(id_b)];
        cands.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        cands
    } else {
        vec![]
    };

    // No safe seed exists when every post-filter candidate is explicitly
    // contradicted. Return an explicit empty abstention instead of indexing
    // `scored[0]` and panicking.
    if scored.is_empty() {
        return RetrievalResult {
            causal_chain:        vec![],
            dense_context:       dense_ctx,
            explanation:         "Abstained: no eligible memory remains after excluding contradicted candidates."
                .to_string(),
            seed_id:             None,
            seed_text:           None,
            seed_node_type:      None,
            seed_authority:      0.0,
            seed_provenance:     String::new(),
            conflict_detected,
            conflict_candidates,
            abstained:           true,
            raw_nn_candidates,
            post_filter_candidates,
            scored_candidates,
            candidate_texts:      vec![],
            seed_episode:         None,
            seed_episode_status:  EpisodeStatus::NoTriple,
        };
    }

    // Step 2d: Seed selection (highest scored node).
    let (seed_id, _seed_score) = scored[0];
    let seed_node      = node_store.get(seed_id);
    let seed_node_type = seed_node.map(|n| n.node_type);
    let seed_authority = seed_node.map(|n| n.authority).unwrap_or(0.5);
    let seed_provenance = seed_node.map(|n| n.provenance.clone()).unwrap_or_default();
    // B1: resolved seed text, snapshotted from the store at selection time.
    let seed_text = seed_node.map(|n| ChainNodeText {
        text: n.text.clone(),
        timestamp: n.timestamp,
    });
    // B1b read-only join (episode half): most-recent triple for the seed node.
    // Resolved AFTER scoring/selection; never feeds back into ranks.
    let seed_episode_status = ledger.resolve_node(seed_id).0;
    let seed_episode = ledger.resolve_node_view(seed_id, node_store);

    // Step 2f: Abstain if conflict detected and abstain_on_conflict is set.
    if conflict_detected && abstain_on_conflict && detect_abstain {
        return RetrievalResult {
            causal_chain:        vec![],
            dense_context:       dense_ctx,
            explanation:         "Abstained: conflicting memories detected; retrieval refused."
                .to_string(),
            seed_id:             Some(seed_id),
            seed_text,
            seed_node_type,
            seed_authority,
            seed_provenance,
            conflict_detected:   true,
            conflict_candidates,
            abstained:           true,
            raw_nn_candidates,
            post_filter_candidates,
            scored_candidates,
            candidate_texts,
            seed_episode,
            seed_episode_status,
        };
    }



    // Step 3: Thompson-sampled Dijkstra (bounded depth)
    let mut heap = BinaryHeap::new();
    heap.push(DijkstraState {
        cost: 0.0,
        node_id: seed_id,
        depth: 0,
        path: vec![],
    });

    let mut visited: HashMap<u64, f32> = HashMap::new();
    let mut best_terminal: Option<DijkstraState> = None;

    while let Some(state) = heap.pop() {
        if visited.contains_key(&state.node_id) {
            continue;
        }
        visited.insert(state.node_id, state.cost);

        // Track best terminal (deepest or lowest cost with path)
        if !state.path.is_empty() {
            match &best_terminal {
                None => best_terminal = Some(DijkstraState { path: state.path.clone(), ..state }),
                Some(best) if state.cost < best.cost => {
                    best_terminal = Some(DijkstraState { path: state.path.clone(), ..state });
                }
                _ => {}
            }
        }

        if state.depth >= max_depth {
            continue;
        }

        for out_edge in edge_store.outgoing(state.node_id) {
            if visited.contains_key(&out_edge.target) {
                continue;
            }
            // Thompson sample
            let w_tilde = out_edge.edge.sample(rng);
            let edge_cost = -(w_tilde + 1e-6_f32).ln();
            let new_cost = state.cost + edge_cost;

            let mut new_path = state.path.clone();
            new_path.push((state.node_id, out_edge.target));

            heap.push(DijkstraState {
                cost: new_cost,
                node_id: out_edge.target,
                depth: state.depth + 1,
                path: new_path,
            });
        }
    }

    // Step 4: Assemble result
    let resolve_text = |id: u64| -> Option<ChainNodeText> {
        node_store.get(id).map(|n| ChainNodeText {
            text: n.text.clone(),
            timestamp: n.timestamp,
        })
    };
    let chain_steps = if let Some(terminal) = best_terminal {
        terminal.path.iter().filter_map(|(src, tgt)| {
            edge_store.get(*src, *tgt).map(|edge| {
                let evidence = edge.alpha + edge.beta;
                ChainStep {
                    from_node: *src,
                    to_node: *tgt,
                    mean_weight: edge.weight(),
                    evidence,
                    variance: edge.variance(),
                    is_intervention: edge.is_intervention,
                    is_curious: edge.variance() > curiosity_threshold,
                    from_text: resolve_text(*src),
                    to_text: resolve_text(*tgt),
                    // B7 (S-19, refined): read-only ledger-membership join.
                    // Same closure site and pattern as resolve_text above;
                    // scoring/selection above never read the ledger.
                    link_kind: classify_link(ledger, *src, *tgt, evidence),
                    verified_causation: false,
                }
            })
        }).collect()
    } else {
        vec![]
    };

    let explanation = format_explanation(&chain_steps, node_store);

    RetrievalResult {
        causal_chain: chain_steps,
        dense_context: dense_ctx,
        explanation,
        seed_id: Some(seed_id),
        seed_text,
        seed_node_type,
        seed_authority,
        seed_provenance,
        conflict_detected,
        conflict_candidates,
        abstained: false,
        raw_nn_candidates,
        post_filter_candidates,
        scored_candidates,
        candidate_texts,
        seed_episode,
        seed_episode_status,
    }

}

fn format_explanation(chain: &[ChainStep], _node_store: &NodeStore) -> String {
    if chain.is_empty() {
        return "No causal path found from seed.".to_string();
    }
    let render = |node_id: u64, text: &Option<ChainNodeText>| -> String {
        match text {
            None => format!("Node({node_id}) [missing from store]"),
            Some(t) if t.text.is_empty() => {
                format!("Node({node_id}) \"no text stored\" (first recorded {})", fmt_learned_date(t.timestamp))
            }
            Some(t) => {
                let shown = if t.text.chars().count() > TEXT_RENDER_CAP {
                    let cut: String = t.text.chars().take(TEXT_RENDER_CAP).collect();
                    let omitted = t.text.chars().count() - TEXT_RENDER_CAP;
                    format!("{cut}…[+{omitted} chars]")
                } else {
                    t.text.clone()
                };
                format!("Node({node_id}) \"{shown}\" (first recorded {})", fmt_learned_date(t.timestamp))
            }
        }
    };
    let mut parts = Vec::new();
    // B7 (S-19, refined) RENDER-ONLY partition: hide cross-experience
    // prior-only steps with a visible count marker, never a silent gap.
    // Same-experience, learned-association, and unknown steps always render
    // (with their kind labels). Scoring/traversal above is untouched.
    let hidden = chain
        .iter()
        .filter(|s| s.link_kind == LinkKind::CrossExperienceUnverified)
        .count();
    let visible: Vec<&ChainStep> = chain
        .iter()
        .filter(|s| s.link_kind != LinkKind::CrossExperienceUnverified)
        .collect();
    if visible.is_empty() {
        return format!(
            "No verified causal path from seed. {}",
            unverified_links_hidden_marker(hidden)
        );
    }
    for (i, step) in visible.iter().enumerate() {
        let confidence_pct = (step.mean_weight * 100.0) as u32;
        let causal_marker = if step.is_intervention { " [intervention]" } else { "" };
        let curious_marker = if step.is_curious { " [uncertain]" } else { "" };
        // B7: learned associations and unverified links carry their kind label.
        let kind_marker = match step.link_kind {
            LinkKind::CrossExperienceLearned => " [learned association]",
            LinkKind::Unknown => " [unverified link]",
            _ => "",
        };
        // The from_node of step i>0 is the to_node of step i-1 — render it once.
        let head = if i == 0 {
            format!("{} ", render(step.from_node, &step.from_text))
        } else {
            String::new()
        };
        parts.push(format!(
            "{head}→{causal_marker}{curious_marker}{kind_marker} {} (confidence: {}%, evidence: {:.1})",
            render(step.to_node, &step.to_text),
            confidence_pct, step.evidence
        ));
    }
    if hidden > 0 {
        parts.push(unverified_links_hidden_marker(hidden));
    }
    parts.join(" ")
}

#[cfg(test)]
mod phase5_tests {
    use super::*;

    #[test]
    fn classifier_routes_exact_symbols_to_lexical() {
        for q in [
            "error TS2375: exactOptionalPropertyTypes",
            "E0382 use of moved value",
            "build fails in src/retrieval.rs",
            "cargo test --all-features fails",
            "normalize_trigger_signature returns empty",
            "CoreState::rebuild_bm25_index panics",
            "HTTP 401 from the gateway",
            "status 500",
            "ERR_MODULE_NOT_FOUND",
            "0x80070005 access denied",
        ] {
            assert_eq!(QueryMode::classify(q), QueryMode::LexicalDominant, "query: {q}");
        }
    }

    #[test]
    fn classifier_routes_natural_language_to_semantic() {
        for q in [
            "why does the build keep failing after the refactor",
            "the app forgets my settings when I restart it",
            "I have 401 apples in the crate",
            "how do I make the retry loop stop early",
            "memory usage grows without bound",
        ] {
            assert_eq!(QueryMode::classify(q), QueryMode::SemanticBalanced, "query: {q}");
        }
        assert_eq!(QueryMode::classify("   "), QueryMode::SemanticBalanced);
    }

    #[test]
    fn classifier_weights_are_complementary() {
        for mode in [QueryMode::LexicalDominant, QueryMode::SemanticBalanced] {
            let sum = mode.bm25_weight() + mode.dense_weight();
            assert!((sum - 1.0).abs() < 1e-6, "{mode:?} weights sum to {sum}");
        }
        assert!(QueryMode::LexicalDominant.bm25_weight() > QueryMode::LexicalDominant.dense_weight());
        assert!(QueryMode::SemanticBalanced.dense_weight() > QueryMode::SemanticBalanced.bm25_weight());
    }

    #[test]
    fn classifier_does_not_treat_prose_as_file_extension() {
        assert!(!has_file_extension("this is a sentence."));
        assert!(!has_file_extension("version 1.2.3"));
        assert!(has_file_extension("open main.rs."));
        assert!(has_file_extension("edit the .tsx component"));
    }
}
