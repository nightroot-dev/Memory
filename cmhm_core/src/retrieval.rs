use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;
use rand::Rng;
use crate::node::{NodeType};
use crate::node_store::{NodeStore, cosine};
use crate::edge_store::EdgeStore;
use crate::dense_core::DenseCore;
use crate::EMBED_DIM;

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
///    - 2c: Score each eligible candidate by relevance:
///          `query_weight * cos(q, node) + context_weight * cos(dense_ctx, node)`
///    - 2d: Seed = highest-scored candidate. Exact score ties are broken by
///          node authority (tie-break only — never additive, never a top-K
///          override; E7 fail-safe).
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
    // Deprecated compatibility argument. Authority no longer contributes to seed
    // ordering; contradicted provenance is excluded from seed eligibility below.
    _authority_weight: f32,
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
) -> RetrievalResult {
    // Step 1: Dense context
    let dense_ctx = dense_core.query(query);

    // Helper closure for empty results (avoids repeating all fields)
    macro_rules! empty_result {
        ($explanation:expr) => {
            RetrievalResult {
                causal_chain:         vec![],
                dense_context:        dense_ctx,
                explanation:          $explanation.to_string(),
                seed_id:              None,
                seed_node_type:       None,
                seed_authority:       0.5,
                seed_provenance:      String::new(),
                conflict_detected:    false,
                conflict_candidates:  vec![],
                abstained:            false,
                raw_nn_candidates:    vec![],
                post_filter_candidates: vec![],
                scored_candidates:    vec![],
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
    // BM25 lexical candidates fused via RRF when query_text is provided.
    // Empty query_text (legacy/vector paths) skips fusion — dense-only behavior unchanged.
    //
    // R1 FIX: Snapshot dense cosines BEFORE fusion so the scorer receives a real cosine
    // value (∈ [−1,1]) for every candidate, not the RRF rank score (~0.016).
    // BM25-only candidates that have no dense hit receive dense_cos = 0.0.
    // rrf_fuse() output is used only for pool membership and ordering; the (id, rrf_score)
    // tuple's score is immediately replaced by the original dense cosine below.
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
        let bm25_candidates: Vec<(u64, f32)> = {
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
        if bm25_candidates.is_empty() {
            raw_candidates
        } else {
            // Fuse for pool membership. Each candidate keeps its REAL dense cosine (computed on
            // demand for BM25-only hits instead of 0.0) plus the identifier boost below, then
            // the pool is re-sorted by that score.
            // Identifier channel: boost wanted-type nodes that contain an explicit identifier from
            // the query. NO injection: rrf_fuse truncates to the 20-cap first and the boost only
            // reorders inside that pool, so pool membership and truncation are unchanged.
            let boosts = bm25.identifier_boost(query_text, |id| match seed_type {
                Some(t) => node_store.get(id).map(|n| n.node_type == t).unwrap_or(false),
                None => true,
            });
            let mut fused: Vec<(u64, f32)> = crate::bm25::rrf_fuse(&raw_candidates, &bm25_candidates, 20)
                .into_iter()
                .map(|(id, _rrf_score)| {
                    let cos = dense_cosines.get(&id).copied().unwrap_or_else(|| {
                        node_store.get(id)
                            .filter(|n| n.model_id == model_id && n.embedding.len() == query.len())
                            .map(|n| cosine(query, &n.embedding))
                            .unwrap_or(0.0)
                    });
                    (id, cos + boosts.get(&id).copied().unwrap_or(0.0))
                })
                .collect();
            fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
            fused
        }
    };
    if raw_candidates.is_empty() {
        return empty_result!("No memories yet.");
    }
    // Stage 0: RAW_NN diagnostic snapshot (passive, unmutated)
    let raw_nn_candidates: Vec<u64> = raw_candidates.iter().map(|(id, _)| *id).collect();

    // Step 2a: Filter out superseded candidates.
    // Superseded nodes are excluded entirely before scoring — no score comparison performed.
    let candidates: Vec<(u64, f32)> = raw_candidates.into_iter()
        .filter(|(id, _)| {
            node_store.get(*id)
                .map_or(false, |n| n.superseded_by.is_none())
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
            seed_node_type:       None,
            seed_authority:       0.5,
            seed_provenance:      String::new(),
            conflict_detected:    false,
            conflict_candidates:  vec![],
            abstained:            false,
            raw_nn_candidates,
            post_filter_candidates,
            scored_candidates:    vec![],
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

    // Step 2c: Relevance scoring of the eligible post-filter pool.
    //
    // Nodes explicitly marked "contradicted" remain in `candidates` so conflict
    // detection above can see them, but they are structurally ineligible to be
    // returned as a seed. This makes the safety posture fail-closed rather than
    // relying on an authority bonus to make unsafe results unlikely.
    // score = query_weight * cos(q, node)
    //       + context_weight * cos(dense_ctx, node)
    //       + BANDIT_WEIGHT * node.thompson_draw(rng)  <- H-03 fix: bandit exploration
    //
    // The Thompson draw theta ∈ (0,1) is sampled from Beta(alpha, beta).
    // New/untested nodes draw from Beta(1,1) = Uniform — exploration bonus.
    // Weight 0.10 keeps exploration bounded relative to cosine similarity.
    const BANDIT_WEIGHT: f32 = 0.10;
    let mut scored: Vec<(u64, f32)> = candidates.iter().filter_map(|(id, cos)| {
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
                query_weight * cos
                    + context_weight * dense_cos
                    + BANDIT_WEIGHT * bandit_draw
            }
            None => query_weight * cos,
        };
        Some((*id, score))
    }).collect();

    // Sort descending by score. Exact ties (equal relevance) are broken by
    // node authority — a tie-break, NOT a weight: it can never move a
    // lower-relevance node above a higher-relevance one, it only settles
    // exact equals. Reviewer condition on the S-01 amendment: this preserves
    // the E7 fail-safe (authoritative 0.85 must beat migrated neutral 0.50 at
    // equal cosine) without reintroducing authority into the score itself.
    // Node id is the final key so ordering stays fully deterministic.
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

    // Stage 2: SCORED diagnostic snapshot (passive, unmutated).
    let scored_candidates: Vec<(u64, f32)> = scored.clone();

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
            seed_node_type:      None,
            seed_authority:      0.0,
            seed_provenance:     String::new(),
            conflict_detected,
            conflict_candidates,
            abstained:           true,
            raw_nn_candidates,
            post_filter_candidates,
            scored_candidates,
        };
    }

    // Step 2d: Seed selection (highest scored node).
    let (seed_id, _seed_score) = scored[0];
    let seed_node      = node_store.get(seed_id);
    let seed_node_type = seed_node.map(|n| n.node_type);
    let seed_authority = seed_node.map(|n| n.authority).unwrap_or(0.5);
    let seed_provenance = seed_node.map(|n| n.provenance.clone()).unwrap_or_default();

    // Step 2f: Abstain if conflict detected and abstain_on_conflict is set.
    if conflict_detected && abstain_on_conflict && detect_abstain {
        return RetrievalResult {
            causal_chain:        vec![],
            dense_context:       dense_ctx,
            explanation:         "Abstained: conflicting memories detected; retrieval refused."
                .to_string(),
            seed_id:             Some(seed_id),
            seed_node_type,
            seed_authority,
            seed_provenance,
            conflict_detected:   true,
            conflict_candidates,
            abstained:           true,
            raw_nn_candidates,
            post_filter_candidates,
            scored_candidates,
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
    let chain_steps = if let Some(terminal) = best_terminal {
        terminal.path.iter().filter_map(|(src, tgt)| {
            edge_store.get(*src, *tgt).map(|edge| {
                ChainStep {
                    from_node: *src,
                    to_node: *tgt,
                    mean_weight: edge.weight(),
                    evidence: edge.alpha + edge.beta,
                    variance: edge.variance(),
                    is_intervention: edge.is_intervention,
                    is_curious: edge.variance() > curiosity_threshold,
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
        seed_node_type,
        seed_authority,
        seed_provenance,
        conflict_detected,
        conflict_candidates,
        abstained: false,
        raw_nn_candidates,
        post_filter_candidates,
        scored_candidates,
    }

}

fn format_explanation(chain: &[ChainStep], _node_store: &NodeStore) -> String {
    if chain.is_empty() {
        return "No causal path found from seed.".to_string();
    }
    let mut parts = Vec::new();
    for step in chain {
        let confidence_pct = (step.mean_weight * 100.0) as u32;
        let causal_marker = if step.is_intervention { " [intervention]" } else { "" };
        let curious_marker = if step.is_curious { " [uncertain]" } else { "" };
        parts.push(format!(
            "Node({}) →{}{} Node({}) (confidence: {}%, evidence: {:.1})",
            step.from_node, causal_marker, curious_marker,
            step.to_node, confidence_pct, step.evidence
        ));
    }
    parts.join(" → ")
}
