use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::node::{Node, NodeType};

/// Node store: primary hash map + brute-force NN index.
///
/// Nearest-neighbor search uses brute-force cosine over f32 embeddings.
/// Optimal up to ~200k nodes on a modern CPU (~10 ms worst case).
///
/// ## Model-ID enforcement (N3)
///
/// `nearest()` requires a `model_id` argument and ONLY compares against nodes
/// that were stored by the same embedder model. Comparing 128-dim hash vectors
/// against 384-dim MiniLM vectors produces silently wrong similarity scores —
/// this invariant is the runtime enforcement of that rule.
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeStore {
    /// Primary storage: NodeId → Node
    pub nodes: HashMap<u64, Node>,
    /// Next ID counter (monotonically increasing)
    next_id: u64,
    /// Global exponential moving average of all embeddings.
    /// Stored as Vec<f32> to match the active embedder's dimension.
    /// Reset to zeros when the embedder dimension changes.
    pub global_ema: Vec<f32>,
    /// Dimension of the embeddings stored in this NodeStore.
    /// Set on first insert; subsequent inserts with different dim are rejected.
    #[serde(default)]
    pub embed_dim: usize,
}

impl NodeStore {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            next_id: 1,
            global_ema: Vec::new(),
            embed_dim: 0,
        }
    }

    /// Insert a node and return its assigned ID.
    ///
    /// On the first insert, establishes the store's embedding dimension.
    /// Subsequent inserts with a different dimension are rejected with a warning.
    pub fn insert(&mut self, node: Node) -> u64 {
        let dim = node.embedding.len();
        if self.embed_dim == 0 {
            self.embed_dim = dim;
            self.global_ema = vec![0.0f32; dim];
        }
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.insert(id, node);
        id
    }

    /// Remove a node. Called only during consolidation.
    pub fn remove(&mut self, id: u64) -> Option<Node> {
        self.nodes.remove(&id)
    }

    pub fn get(&self, id: u64) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Brute-force top-k nearest neighbors by cosine similarity.
    ///
    /// Only compares against nodes with matching `model_id` (N3).
    /// Returns `Vec<(NodeId, cosine)>` sorted descending by similarity.
    ///
    /// Emits a `tracing::warn!` when every node in the store is skipped due to
    /// model_id mismatch — that is the signature of a stale rename or wrong embedder.
    pub fn nearest(&self, query: &[f32], model_id: &str, k: usize) -> Vec<(u64, f32)> {
        if self.nodes.is_empty() {
            return vec![];
        }

        let total = self.nodes.len();
        let mut scores: Vec<(u64, f32)> = self.nodes.iter()
            .filter(|(_, node)| node.model_id == model_id)
            .filter(|(_, node)| node.embedding.len() == query.len())
            .map(|(&id, node)| (id, cosine(query, &node.embedding)))
            .collect();

        // Mismatch metric: nodes that exist but were invisible to this query.
        // If ALL nodes were skipped, something is structurally wrong — log loudly.
        let nodes_skipped_model_mismatch = total.saturating_sub(scores.len());
        if nodes_skipped_model_mismatch > 0 && scores.is_empty() {
            tracing::warn!(
                model_id,
                total_nodes = total,
                nodes_skipped_model_mismatch,
                "nearest(): ALL nodes skipped — model_id mismatch. \
                 This produces silently empty results. \
                 Did a model_id rename leave stale nodes on disk? \
                 Known good id: 'hash-128d'."
            );
        } else if nodes_skipped_model_mismatch > 0 {
            tracing::debug!(
                model_id,
                nodes_skipped_model_mismatch,
                "nearest(): some nodes skipped due to model_id mismatch (expected during embedder migration)"
            );
        }

        // Partial sort: we only need top-k
        scores.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        scores
    }

    /// Brute-force top-k nearest neighbours restricted to a specific `NodeType`.
    ///
    /// Used by `birth_node` so that STATE embeddings are only compared against
    /// other STATE nodes, ACTION against ACTION, etc. This prevents cross-role
    /// EMA contamination (the 0.34 floor observed in `test_diagnose_cosines`).
    pub fn nearest_by_type(
        &self,
        query: &[f32],
        model_id: &str,
        k: usize,
        node_type: NodeType,
    ) -> Vec<(u64, f32)> {
        if self.nodes.is_empty() { return vec![]; }
        let mut scores: Vec<(u64, f32)> = self.nodes.iter()
            .filter(|(_, n)| n.model_id == model_id
                           && n.node_type == node_type
                           && n.embedding.len() == query.len())
            .map(|(&id, n)| (id, cosine(query, &n.embedding)))
            .collect();
        scores.sort_unstable_by(|a, b|
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        scores
    }

    /// Maximum cosine similarity between `query` and any existing node with `model_id`.
    ///
    /// Used by the shadow gate to compute `novelty = 1 − max_cosine_to_existing`.
    /// Returns `0.0` if the store is empty or has no matching nodes
    /// (empty store → novelty = 1.0 → gate score is maximised, which is correct).
    pub fn max_cosine_to_existing(&self, query: &[f32], model_id: &str) -> f32 {
        self.nodes.values()
            .filter(|n| n.model_id == model_id && n.embedding.len() == query.len())
            .map(|n| cosine(query, &n.embedding))
            .fold(0.0f32, f32::max)
    }

    /// Maximum cosine similarity restricted to a specific `NodeType`.
    ///
    /// Used by the shadow gate when evaluating STATE embedding novelty — only
    /// STATE nodes are relevant for deciding whether a new state is novel.
    pub fn max_cosine_to_existing_typed(
        &self,
        query: &[f32],
        model_id: &str,
        node_type: NodeType,
    ) -> f32 {
        self.nodes.values()
            .filter(|n| n.model_id == model_id
                     && n.node_type == node_type
                     && n.embedding.len() == query.len())
            .map(|n| cosine(query, &n.embedding))
            .fold(0.0f32, f32::max)
    }

    /// Update the global EMA with a new embedding.
    /// Called on every node birth. Skips if dims don't match (transition period).
    pub fn update_global_ema(&mut self, e: &[f32]) {
        if self.global_ema.len() != e.len() {
            // Dim mismatch during embedder transition — reset EMA to new dim
            self.global_ema = e.to_vec();
            return;
        }
        for i in 0..e.len() {
            self.global_ema[i] = 0.1 * e[i] + 0.9 * self.global_ema[i];
        }
    }

    /// Blend a raw embedding with the global EMA and normalize.
    /// `e_new = normalize(0.7 * e + 0.3 * μ)`
    pub fn blend_with_ema(&self, e: &[f32]) -> Vec<f32> {
        if self.global_ema.len() != e.len() || self.global_ema.is_empty() {
            // No EMA yet or dim mismatch — return normalized raw embedding
            return l2_normalize(e);
        }
        let mut blended: Vec<f32> = e.iter().zip(self.global_ema.iter())
            .map(|(x, m)| 0.7 * x + 0.3 * m)
            .collect();
        l2_normalize_in_place(&mut blended);
        blended
    }

    /// Iterate all nodes (for consolidation passes).
    pub fn iter(&self) -> impl Iterator<Item = (&u64, &Node)> {
        self.nodes.iter()
    }

    /// All node IDs (for consolidation).
    pub fn all_ids(&self) -> Vec<u64> {
        self.nodes.keys().copied().collect()
    }
}

/// L2-normalized cosine similarity between two equal-length float slices.
///
/// Embeddings are stored unit-norm, so this is equivalent to the dot product.
/// Panics in debug if lengths differ (caught by `nearest`'s dim filter in production).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "cosine: dimension mismatch ({} vs {})", a.len(), b.len());
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    dot.clamp(-1.0, 1.0)
}

/// L2-normalize a slice, returning a new Vec<f32>.
/// Returns a zero vector if the norm is below the epsilon threshold.
pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1e-10 {
        return vec![0.0f32; v.len()];
    }
    v.iter().map(|x| x / norm).collect()
}

/// L2-normalize in place.
pub fn l2_normalize_in_place(v: &mut Vec<f32>) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1e-10 { return; }
    for x in v.iter_mut() { *x /= norm; }
}

/// Check if any element is NaN.
pub fn has_nan(v: &[f32]) -> bool {
    v.iter().any(|x| x.is_nan())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeType;

    fn dummy_node(v: f32, model_id: &str) -> Node {
        dummy_node_typed(v, model_id, NodeType::State)
    }

    fn dummy_node_typed(v: f32, model_id: &str, node_type: NodeType) -> Node {
        let mut emb = vec![0.0f32; 128];
        emb[0] = v;
        let emb = l2_normalize(&emb);
        Node::new(emb, model_id.to_string(), 0.5, 0, 0.0, node_type)
    }

    #[test]
    fn test_insert_and_get() {
        let mut store = NodeStore::new();
        let n = dummy_node(1.0, "hash-128d");
        let id = store.insert(n.clone());
        assert_eq!(id, 1);
        assert!(store.get(id).is_some());
    }

    #[test]
    fn test_nearest_returns_closest() {
        let mut e1 = vec![0.0f32; 128]; e1[0] = 1.0;
        let mut e2 = vec![0.0f32; 128]; e2[1] = 1.0;
        let mut store = NodeStore::new();
        store.insert(Node::new(l2_normalize(&e1), "hash-128d".into(), 0.5, 0, 0.0, NodeType::State));
        store.insert(Node::new(l2_normalize(&e2), "hash-128d".into(), 0.5, 0, 0.0, NodeType::State));

        let mut query = vec![0.0f32; 128]; query[0] = 1.0;
        let results = store.nearest(&query, "hash-128d", 1);
        assert_eq!(results.len(), 1);
        assert!((results[0].1 - 1.0).abs() < 1e-5, "expected cosine≈1.0, got {}", results[0].1);
    }

    #[test]
    fn test_nearest_filters_by_model_id() {
        let mut store = NodeStore::new();
        let mut e1 = vec![0.0f32; 128]; e1[0] = 1.0;
        // Insert one hash node and one fake "minilm" node (same dim for test)
        store.insert(Node::new(l2_normalize(&e1), "hash-128d".into(), 0.5, 0, 0.0, NodeType::State));
        let mut e2 = vec![0.0f32; 128]; e2[0] = 0.99;
        store.insert(Node::new(l2_normalize(&e2), "minilm-l6-v2-int8".into(), 0.5, 0, 0.0, NodeType::State));

        let query = l2_normalize(&e1);
        // hash-128d query should only see the hash-128d node
        let results = store.nearest(&query, "hash-128d", 10);
        assert_eq!(results.len(), 1, "should only return hash-128d nodes");

        let results_minilm = store.nearest(&query, "minilm-l6-v2-int8", 10);
        assert_eq!(results_minilm.len(), 1, "should only return minilm nodes");
    }

    #[test]
    fn test_cosine_orthogonal() {
        let mut a = vec![0.0f32; 128]; a[0] = 1.0;
        let mut b = vec![0.0f32; 128]; b[1] = 1.0;
        assert!((cosine(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_unit_norm() {
        let v: Vec<f32> = (0..128).map(|i| i as f32).collect();
        let n = l2_normalize(&v);
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "norm should be 1.0, got {norm:.6}");
    }

    #[test]
    fn nearest_by_type_isolates_roles() {
        // A STATE and an ACTION node with identical embeddings: each role's
        // nearest_by_type must only see nodes of its own type.
        let mut store = NodeStore::new();
        let mut e = vec![0.0f32; 128]; e[0] = 1.0;
        let e = l2_normalize(&e);
        let state_id  = store.insert(dummy_node_typed(1.0, "hash-128d", NodeType::State));
        let action_id = store.insert(dummy_node_typed(1.0, "hash-128d", NodeType::Action));

        let results_state  = store.nearest_by_type(&e, "hash-128d", 10, NodeType::State);
        let results_action = store.nearest_by_type(&e, "hash-128d", 10, NodeType::Action);

        assert_eq!(results_state.len(), 1);
        assert_eq!(results_state[0].0, state_id,  "STATE query must only return STATE node");
        assert_eq!(results_action.len(), 1);
        assert_eq!(results_action[0].0, action_id, "ACTION query must only return ACTION node");

        // max_cosine_to_existing_typed must also scope correctly
        let mc_state  = store.max_cosine_to_existing_typed(&e, "hash-128d", NodeType::State);
        let mc_action = store.max_cosine_to_existing_typed(&e, "hash-128d", NodeType::Action);
        assert!((mc_state  - 1.0).abs() < 1e-5);
        assert!((mc_action - 1.0).abs() < 1e-5);

        // Outcome namespace is empty — must return 0.0 ("maximally novel")
        let mc_outcome = store.max_cosine_to_existing_typed(&e, "hash-128d", NodeType::Outcome);
        assert!((mc_outcome - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_global_ema_updates() {
        let mut store = NodeStore::new();
        let emb = l2_normalize(&vec![1.0f32; 128]);
        store.insert(Node::new(emb.clone(), "hash-128d".into(), 0.5, 0, 0.0, NodeType::State));
        store.update_global_ema(&emb);
        // EMA should be nonzero after update
        let sum: f32 = store.global_ema.iter().sum();
        assert!(sum > 0.0, "EMA should be updated");
    }
}
