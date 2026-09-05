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
///    - Filter: remove candidates where `node.superseded_by.is_some()`.
///    - Score each active candidate:
///      `query_weight * cos(q, node) + context_weight * cos(dense_ctx, node)
///       + authority_weight * node.authority`
///    - `emotional_tag` is affective salience only; it does NOT appear in the score.
///    - No recency term; use `superseded_by` for staleness handling.
/// 3. Conflict detection: if top-2 candidates have score gap < `conflict_detection_threshold`
///    AND (differing non-empty provenance OR |authority_A - authority_B| < 0.1),
///    `conflict_detected = true`.
/// 4. Dijkstra with Thompson-sampled edge costs, bounded to `max_depth` hops.
/// 5. Assemble path with mean weights, evidence, variance.
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
    // Weight of node.authority in seed scoring. Production default: 0.8.
    // Uses node.authority (machine-readable trust level), NOT node.emotional_tag.
    authority_weight: f32,
    // Score gap below which two candidates trigger conflict_detected = true.
    conflict_detection_threshold: f32,
    // When true and conflict_detected, return empty chain + abstained=true instead of
    // returning the contested winner. Callers still receive conflict_candidates.
    abstain_on_conflict: bool,
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
        Some(t) => node_store.nearest_by_type(query, model_id, 8, t),
        None    => node_store.nearest(query, model_id, 8),
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

    // Step 2b: Score each active candidate.
    // score = query_weight * cos(q, node)
    //       + context_weight * cos(dense_ctx, node)
    //       + authority_weight * node.authority
    // node.emotional_tag is NOT in the formula — it is affective salience only.
    // No recency term — staleness is handled by supersession (superseded_by field).
    let mut scored: Vec<(u64, f32)> = candidates.iter().map(|(id, cos)| {
        let score = match node_store.get(*id) {
            Some(node) => {
                // min-len cosine for mixed-dim safety during embedder transition
                let min_len = dense_ctx.len().min(node.embedding.len());
                let dense_cos = cosine(&dense_ctx[..min_len], &node.embedding[..min_len]);
                query_weight * cos
                    + context_weight * dense_cos
                    + authority_weight * node.authority
            }
            None => query_weight * cos,
        };
        (*id, score)
    }).collect();

    // Sort descending by score.
    scored.sort_by(|(_, sa), (_, sb)| sb.partial_cmp(sa).unwrap_or(Ordering::Equal));

    // Stage 2: SCORED diagnostic snapshot (passive, unmutated)
    let scored_candidates: Vec<(u64, f32)> = scored.clone();

    // Step 2c: Select seed (highest score).
    let (seed_id, _seed_score) = scored[0];
    let seed_node     = node_store.get(seed_id);
    let seed_node_type = seed_node.map(|n| n.node_type);
    let seed_authority  = seed_node.map(|n| n.authority).unwrap_or(0.5);
    let seed_provenance = seed_node.map(|n| n.provenance.clone()).unwrap_or_default();

    // Step 2d: Conflict detection with semantic classification.
    //
    // Conflict requires BOTH:
    //   (a) score gap < conflict_detection_threshold  (score proximity)
    //   (b) at least one semantic incompatibility class applies
    //
    // Three machine-readable ConflictType classes:
    //   SupersessionConflict — one candidate supersedes the other but still scores within threshold
    //   AuthorityPolarity    — opposing provenance signals ("corrected" vs "contradicted")
    //   ScoreAmbiguity       — provenance differs, authority close — engine cannot decide
    //
    // Score proximity alone (no class match) is NOT a conflict.
    let (conflict_detected, conflict_candidates): (bool, Vec<ConflictCandidate>) = if scored.len() >= 2 {
        let (id_a, score_a) = scored[0];
        let (id_b, score_b) = scored[1];
        let gap = score_a - score_b;

        if gap < conflict_detection_threshold {
            let node_a = node_store.get(id_a);
            let node_b = node_store.get(id_b);
            let prov_a = node_a.map(|n| n.provenance.as_str()).unwrap_or("");
            let prov_b = node_b.map(|n| n.provenance.as_str()).unwrap_or("");
            let auth_a = node_a.map(|n| n.authority).unwrap_or(0.5);
            let auth_b = node_b.map(|n| n.authority).unwrap_or(0.5);
            let super_a = node_a.and_then(|n| n.superseded_by);
            let super_b = node_b.and_then(|n| n.superseded_by);
            let key_a = node_a.and_then(|n| n.assertion_key.as_deref());
            let key_b = node_b.and_then(|n| n.assertion_key.as_deref());
            let pol_a = node_a.and_then(|n| n.polarity);
            let pol_b = node_b.and_then(|n| n.polarity);

            // 1. Explicit logical contradiction: identical non-empty assertion_key with opposing polarity
            let is_logical_contradiction = match (key_a, key_b, pol_a, pol_b) {
                (Some(ka), Some(kb), Some(pa), Some(pb)) if ka == kb && pa * pb < 0 => true,
                _ => false,
            };

            // Classify: logical contradiction takes highest semantic precedence.
            let conflict_type: Option<ConflictType> =
                if is_logical_contradiction {
                    Some(ConflictType::LogicalContradiction)
                } else if super_a == Some(id_b) || super_b == Some(id_a) {
                    // A supersedes B or B supersedes A but both still in candidate pool.
                    Some(ConflictType::SupersessionConflict)
                } else {
                    // Authority polarity: opposing trust signals.
                    const HIGH_TRUST: &[&str] = &["corrected", "user_explicit"];
                    const LOW_TRUST:  &[&str] = &["contradicted"];
                    let a_high = HIGH_TRUST.contains(&prov_a);
                    let b_low  = LOW_TRUST.contains(&prov_b);
                    let b_high = HIGH_TRUST.contains(&prov_b);
                    let a_low  = LOW_TRUST.contains(&prov_a);
                    if (a_high && b_low) || (b_high && a_low) {
                        Some(ConflictType::AuthorityPolarity)
                    } else if !prov_a.is_empty() && !prov_b.is_empty()
                        && prov_a != prov_b
                        && (auth_a - auth_b).abs() < 0.1
                    {
                        // Provenance strings differ and authority is nearly equal —
                        // the engine cannot determine which record is more reliable.
                        Some(ConflictType::ScoreAmbiguity)
                    } else {
                        None // score proximity only, no semantic incompatibility — not a conflict
                    }
                };


            if let Some(ctype) = conflict_type {
                let candidates = vec![
                    ConflictCandidate {
                        node_id:      id_a,
                        score:        score_a,
                        authority:    auth_a,
                        provenance:   node_a.map(|n| n.provenance.clone()).unwrap_or_default(),
                        conflict_type: ctype.clone(),
                    },
                    ConflictCandidate {
                        node_id:      id_b,
                        score:        score_b,
                        authority:    auth_b,
                        provenance:   node_b.map(|n| n.provenance.clone()).unwrap_or_default(),
                        conflict_type: ctype,
                    },
                ];
                (true, candidates)
            } else {
                (false, vec![])
            }
        } else {
            (false, vec![])
        }
    } else {
        (false, vec![])
    };



    // Step 2e: Abstain if conflict_detected and abstain_on_conflict is set.
    // Returns an empty chain with abstained=true and the structured conflict candidates.
    if conflict_detected && abstain_on_conflict {
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
