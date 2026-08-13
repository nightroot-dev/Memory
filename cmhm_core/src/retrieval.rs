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
/// 2. Seed selection: top-3 NN (role-scoped when seed_type is Some), mixed score
///    `query_weight * cos(q,node) + context_weight * cos(c,node)
///     + authority_weight * node.emotional_tag
///     + recency_weight * exp(-recency_lambda * (now_secs - node.timestamp))`.
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
    /// Role-scoped seed selection. Some(State) restricts candidates to State nodes.
    /// None falls back to the unscoped nearest() scan (backward-compat path).
    seed_type: Option<NodeType>,
    /// Weight of node.emotional_tag in seed scoring. 0.0 = no authority signal (default).
    authority_weight: f32,
    /// Weight of the exponential recency term in seed scoring. 0.0 = no recency (default).
    recency_weight: f32,
    /// Decay rate for the recency term, s⁻¹. Ignored when recency_weight == 0.
    recency_lambda: f32,
    /// Current Unix timestamp in seconds. Used for recency scoring.
    now_secs: i64,
) -> RetrievalResult {
    // Step 1: Dense context
    let dense_ctx = dense_core.query(query);

    // Step 2: Seed selection — top-3 NN by cosine (same model_id only — N3)
    // When seed_type is Some(t), restrict candidates to nodes of that role.
    let candidates = match seed_type {
        Some(t) => node_store.nearest_by_type(query, model_id, 3, t),
        None    => node_store.nearest(query, model_id, 3),
    };
    if candidates.is_empty() {
        return RetrievalResult {
            causal_chain: vec![],
            dense_context: dense_ctx,
            explanation: "No memories yet.".to_string(),
            seed_id: None,
            seed_node_type: None,
        };
    }

    // Mixed score: query_weight * cos(q, node) + context_weight * cos(dense_ctx, node)
    //            + authority_weight * node.emotional_tag
    //            + recency_weight * exp(-recency_lambda * (now_secs - node.timestamp))
    let seed_id = candidates.iter().max_by(|(id_a, cos_a), (id_b, cos_b)| {
        let score = |id: u64, cos: f32| {
            match node_store.get(id) {
                Some(node) => {
                    // min-len cosine for mixed-dim safety during embedder transition
                    let min_len = dense_ctx.len().min(node.embedding.len());
                    let dense_cos = cosine(&dense_ctx[..min_len], &node.embedding[..min_len]);
                    let recency = if recency_weight > 0.0 {
                        recency_weight * (-recency_lambda * (now_secs - node.timestamp) as f32).exp()
                    } else {
                        0.0
                    };
                    query_weight * cos
                        + context_weight * dense_cos
                        + authority_weight * node.emotional_tag
                        + recency
                }
                None => query_weight * cos,
            }
        };
        let sa = score(*id_a, *cos_a);
        let sb = score(*id_b, *cos_b);
        sa.partial_cmp(&sb).unwrap_or(Ordering::Equal)
    }).map(|(id, _)| *id);

    // If no seed found (candidates empty after node-store filter), return empty result
    let seed_id = match seed_id {
        Some(id) => id,
        None => return RetrievalResult {
            causal_chain: vec![],
            dense_context: dense_ctx,
            explanation: "No valid seed node found.".to_string(),
            seed_id: None,
            seed_node_type: None,
        },
    };
    // Capture the seed's NodeType for observability. Lookup is O(1) via HashMap.
    let seed_node_type = node_store.get(seed_id).map(|n| n.node_type);

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
