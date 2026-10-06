use serde::{Deserialize, Serialize};
use crate::node_store::NodeStore;
use crate::edge_store::EdgeStore;
use crate::dense_core::DenseCore;
use crate::edge::{Edge, EdgeType, EvidenceType};
use crate::episode_log::EpisodeLog;
use crate::projection::ProjectionRegistry;

/// Consolidation Engine — periodic global maintenance on the graph.
///
/// Triggered when any of:
///   - 24 hours since last consolidation
///   - Node count > 8,000
///   - Cumulative |Δreinforce| > 50
///
/// Executes atomically on the single worker thread (no locks needed).
///
/// Step order (per spec):
///   1. Update cohort centroids (mini-batch k-means)
///   2. Propagate episode log
///   3. Superstition filter (Wilson CI on ATE)
///   4. Prune dead edges
///   5. Cull dead nodes + bridging
///   6. Merge near-identical nodes
///   7. Skill compilation

const NUM_COHORTS: usize = 512;
/// Cohort dimension must always equal node embedding dimension.
use crate::EMBED_DIM as COHORT_DIM;
const COHORT_MOMENTUM: f32 = 0.8;
const CREDIT_PROP_ETA: f32 = 0.1;
const _EDGE_DEATH_WEIGHT: f32 = 0.04;
const NODE_DEATH_SCORE: f32 = 0.12;
const NODE_DEATH_AGE_DAYS: f64 = 14.0;
const _EDGE_DEATH_AGE_DAYS: f64 = 7.0;
const SKILL_MIN_EVIDENCE: f32 = 50.0;
const SKILL_MIN_SUCCESS_RATE: f32 = 0.8;
const MERGE_COSINE: f32 = 0.95;
const BRIDGE_COSINE: f32 = 0.6;

/// Cohort centroids for the superstition filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cohorts {
    /// NUM_COHORTS centroids, dimension matching active embedder (default 128)
    pub centroids: Vec<Vec<f32>>,
}

impl Cohorts {
    pub fn new() -> Self {
        Self::with_dim(COHORT_DIM)
    }

    pub fn with_dim(dim: usize) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let centroids = (0..NUM_COHORTS)
            .map(|_| {
                let mut c = vec![0.0f32; dim];
                for v in c.iter_mut() { *v = rng.gen::<f32>() * 0.01; }
                c
            })
            .collect();
        Self { centroids }
    }

    /// Dimensionality of the centroids in this cohort bank.
    pub fn dim(&self) -> usize {
        self.centroids.first().map(|c| c.len()).unwrap_or(COHORT_DIM)
    }

    /// Ensure the cohort bank matches the target embedding dimension.
    ///
    /// If centroids are empty or dimension differs from target_dim (e.g. after an embedder
    /// transition or when processing 384-dim ONNX nodes), reinitializes centroids to match.
    pub fn ensure_dim(&mut self, target_dim: usize) {
        if target_dim > 0 && (self.centroids.is_empty() || self.dim() != target_dim) {
            *self = Self::with_dim(target_dim);
        }
    }

    /// Hard cluster label for an embedding (argmax of soft assignment).
    pub fn label(&self, e: &[f32]) -> usize {
        self.centroids.iter().enumerate()
            .map(|(i, c)| (i, dist_sq(e, c)))
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Mini-batch k-means update step (momentum 0.8).
    pub fn update(&mut self, samples: &[&[f32]]) {
        if samples.is_empty() { return; }
        let dim = self.dim();
        for sample in samples {
            if sample.len() != dim { continue; }
            let closest = self.label(sample);
            let c = &mut self.centroids[closest];
            for i in 0..dim {
                c[i] = COHORT_MOMENTUM * c[i] + (1.0 - COHORT_MOMENTUM) * sample[i];
            }
        }
    }
}

fn dist_sq(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum()
}

/// Wilson score confidence interval for a proportion p with n observations.
/// Returns (lower, upper).
pub fn wilson_ci(successes: f64, total: f64) -> (f64, f64) {
    if total == 0.0 {
        return (0.0, 1.0);
    }
    let p = successes / total;
    let z = 1.96_f64;
    let z2 = z * z;
    let denom = 1.0 + z2 / total;
    let centre = (p + z2 / (2.0 * total)) / denom;
    let inner = p * (1.0 - p) / total + z2 / (4.0 * total * total);
    let radius = z * inner.sqrt() / denom;
    ((centre - radius).max(0.0), (centre + radius).min(1.0))
}

/// Run the full consolidation procedure on the core state.
///
/// `bm25_index` is kept in sync: culled/merged node IDs are removed so
/// lexical retrieval never returns dead IDs (retrieval also filters
/// missing IDs defensively, but the index stays small this way).
pub fn run_consolidation(
    node_store: &mut NodeStore,
    edge_store: &mut EdgeStore,
    dense_core: &mut DenseCore,
    episode_log: &mut EpisodeLog,
    cohorts: &mut Cohorts,
    bm25_index: &mut crate::bm25::Bm25Index,
    projections: &ProjectionRegistry,
    now_secs: f64,
    _rng: &mut impl rand::Rng,
) {
    // --- Step 1: Update cohort centroids ---
    //
    // Determine active embedder dimension from node_store (or fall back to COHORT_DIM).
    // Ensure cohort centroids match active dimension (handles hash 128-dim vs MiniLM 384-dim).
    let target_dim = if node_store.embed_dim > 0 {
        node_store.embed_dim
    } else {
        node_store.iter().next().map(|(_, n)| n.embedding.len()).unwrap_or(COHORT_DIM)
    };
    cohorts.ensure_dim(target_dim);

    let sample_count = (node_store.len()).min(2000);
    let samples: Vec<&[f32]> = node_store.iter()
        .take(sample_count)
        .filter(|(_, n)| n.embedding.len() == target_dim)
        .map(|(_, n)| n.embedding.as_slice())
        .collect();
    cohorts.update(&samples);

    // Compute hard labels for all nodes whose dim matches the cohort centroids
    let node_labels: std::collections::HashMap<u64, usize> = node_store.iter()
        .filter(|(_, n)| n.embedding.len() == target_dim)
        .map(|(&id, n)| (id, cohorts.label(&n.embedding)))
        .collect();

    // --- Step 2: Propagate episode log ---
    let entries = episode_log.drain();
    for entry in entries {
        if let Some(edge) = edge_store.get_mut(entry.state_id, entry.action_id) {
            let delta = CREDIT_PROP_ETA * entry.delta;
            if entry.was_success {
                edge.alpha += delta;
            } else {
                edge.beta += delta;
            }
            // Update dense core for this edge.
            // Capture node data into owned values so the immutable borrow of
            // node_store does not conflict with the mutable borrow of dense_core.
            let src_info = node_store.get(entry.state_id)
                .map(|n| (n.embedding.clone(), n.model_id.clone(), n.node_type as u8));
            let tgt_info = node_store.get(entry.action_id)
                .map(|n| (n.embedding.clone(), n.model_id.clone(), n.node_type as u8));

            if let (Some((s_emb, s_mid, s_type)), Some((t_emb, t_mid, t_type))) = (src_info, tgt_info) {
                let slice = DenseCore::slice_index(
                    EdgeType::Sequence as u8,
                    s_type,
                    t_type,
                );
                let w = edge.alpha / (edge.alpha + edge.beta);
                match (projections.project(&s_mid, &s_emb),
                       projections.project(&t_mid, &t_emb))
                {
                    (Ok(sp), Ok(tp)) => {
                        dense_core.update(slice, &sp, &tp, w);
                    }
                    (s_res, t_res) => {
                        tracing::warn!(
                            "consolidation: credit-propagation dense update skipped; \
                             src_mid={} src_dim={} src_ok={} \
                             tgt_mid={} tgt_dim={} tgt_ok={}",
                            s_mid, s_emb.len(), s_res.is_ok(),
                            t_mid, t_emb.len(), t_res.is_ok()
                        );
                    }
                }
            }
        }
    }

    // --- Step 3: Superstition filter (Wilson CI on ATE) ---
    // Build count matrix for intervention edges
    let mut succ = vec![vec![0.0f64; NUM_COHORTS]; NUM_COHORTS];
    let mut fail = vec![vec![0.0f64; NUM_COHORTS]; NUM_COHORTS];

    for (src, tgt, edge) in edge_store.all_edges() {
        if edge.edge_type == EdgeType::CausalHypothesis && edge.is_intervention {
            if let (Some(&ca), Some(&cb)) = (node_labels.get(&src), node_labels.get(&tgt)) {
                succ[ca][cb] += edge.alpha as f64;
                fail[ca][cb] += edge.beta as f64;
            }
        }
    }

    // Determine edges to prune
    let mut to_prune: Vec<(u64, u64)> = Vec::new();
    for (src, tgt, edge) in edge_store.all_edges() {
        if edge.edge_type != EdgeType::CausalHypothesis || !edge.is_intervention {
            continue;
        }
        if let (Some(&ca), Some(&cb)) = (node_labels.get(&src), node_labels.get(&tgt)) {
            let n1 = succ[ca][cb] + fail[ca][cb];
            if n1 < 5.0 {
                continue; // not enough data
            }
            let p_treated = if n1 > 0.0 { succ[ca][cb] / n1 } else { 0.0 };

            let succ_ctrl: f64 = (0..NUM_COHORTS).filter(|&c| c != ca).map(|c| succ[c][cb]).sum();
            let fail_ctrl: f64 = (0..NUM_COHORTS).filter(|&c| c != ca).map(|c| fail[c][cb]).sum();
            let n_ctrl = succ_ctrl + fail_ctrl;
            let p_ctrl = if n_ctrl > 0.0 { succ_ctrl / n_ctrl } else { 0.0 };

            let delta = p_treated - p_ctrl;

            // Wilson CI for each proportion, Newcombe's method for difference
            let (_, hi_t) = wilson_ci(succ[ca][cb], n1);
            let (lo_c, _) = wilson_ci(succ_ctrl, n_ctrl);
            // Upper bound of Δ CI
            let upper = delta + 1.96 * (
                (hi_t * (1.0 - hi_t) / n1)
                + if n_ctrl >= 1.0 {
                    lo_c * (1.0 - lo_c) / n_ctrl
                } else {
                    // L-03 fix: n_ctrl=0 → wilson_ci returns lo_c=0 → variance=0 → CI collapses.
                    // Use max variance 0.25 (p=0.5) as a conservative fallback.
                    // This prevents incorrectly pruning edges with no control-arm data.
                    0.25
                }
            ).sqrt();

            if upper <= 0.0 {
                to_prune.push((src, tgt));
            }
        }
    }

    // --- Step 4: Prune dead edges ---
    let dead_edges: Vec<(u64, u64)> = edge_store.all_edges()
        .into_iter()
        .filter(|(_, _, e)| e.is_dead(now_secs))
        .map(|(s, t, _)| (s, t))
        .collect();

    for (src, tgt) in dead_edges.into_iter().chain(to_prune.into_iter()) {
        // Remove dense core influence before deleting.
        // Capture node data into owned values so any borrowed references
        // don't outlive the mutable dense_core borrow.
        let src_info = node_store.get(src)
            .map(|n| (n.embedding.clone(), n.model_id.clone(), n.node_type as u8));
        let tgt_info = node_store.get(tgt)
            .map(|n| (n.embedding.clone(), n.model_id.clone(), n.node_type as u8));
        let edge_type_byte = edge_store.get(src, tgt).map(|e| e.edge_type as u8);

        if let (Some((s_emb, s_mid, s_type)), Some((t_emb, t_mid, t_type)), Some(et)) =
            (src_info, tgt_info, edge_type_byte)
        {
            let slice = DenseCore::slice_index(et, s_type, t_type);
            match (projections.project(&s_mid, &s_emb),
                   projections.project(&t_mid, &t_emb))
            {
                (Ok(sp), Ok(tp)) => {
                    dense_core.remove_edge(slice, &sp, &tp);
                }
                (s_res, t_res) => {
                    tracing::warn!(
                        "consolidation: dead-edge dense cleanup skipped; \
                         src_mid={} src_dim={} src_ok={} \
                         tgt_mid={} tgt_dim={} tgt_ok={}",
                        s_mid, s_emb.len(), s_res.is_ok(),
                        t_mid, t_emb.len(), t_res.is_ok()
                    );
                }
            }
        }
        edge_store.remove_edge(src, tgt);
    }

    // --- Step 5: Cull dead nodes + bridging ---
    //
    // A node is CULLED only when ALL of the following hold:
    //   1. NOT δ-immune (max_prediction_error <= 0.8)
    //   2. Never retrieved (n_retrievals == 0) — if it was useful once, keep it
    //   3. effective_salience (post-decay) < NODE_DEATH_SCORE
    //   4. Age > NODE_DEATH_AGE_DAYS
    //   5. degree × salience < NODE_DEATH_SCORE (structural importance guard)
    let now_i64 = now_secs as i64;
    let dead_nodes: Vec<u64> = node_store.iter()
        .filter(|(&id, node)| {
            // Guard 1: δ-immune nodes are never culled
            if node.is_delta_immune() { return false; }
            // Guard 2: nodes that were useful in practice are protected
            if node.n_retrievals > 0 { return false; }
            // Guard 3+4+5: standard cull conditions
            let degree   = edge_store.degree(id) as f32;
            let eff_sal  = node.effective_salience(now_i64);
            let age_days = (now_i64 - node.timestamp) as f64 / 86400.0;
            eff_sal * degree < NODE_DEATH_SCORE && age_days > NODE_DEATH_AGE_DAYS
        })
        .map(|(&id, _)| id)
        .collect();

    for node_id in dead_nodes {
        // Collect top-3 incoming and outgoing edges for bridging
        let incoming: Vec<(u64, f32)> = {
            let srcs = edge_store.incoming_sources(node_id);
            let mut v: Vec<(u64, f32)> = srcs.iter()
                .filter_map(|&src| edge_store.get(src, node_id).map(|e| (src, e.weight())))
                .collect();
            v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            v.truncate(3);
            v
        };
        let outgoing: Vec<(u64, f32)> = {
            let outs: Vec<(u64, f32)> = edge_store.outgoing(node_id)
                .iter()
                .map(|o| (o.target, o.edge.weight()))
                .collect();
            let mut v = outs;
            v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            v.truncate(3);
            v
        };

        // Create bridges: in_node → out_node if cosine > 0.6
        for (in_src, w_in) in &incoming {
            for (out_tgt, w_out) in &outgoing {
                if let (Some(in_node), Some(out_node)) = (node_store.get(*in_src), node_store.get(*out_tgt)) {
                    use crate::node_store::cosine;
                    let min_len = in_node.embedding.len().min(out_node.embedding.len());
                    let cos = cosine(&in_node.embedding[..min_len], &out_node.embedding[..min_len]);
                    if cos > BRIDGE_COSINE {
                        let bridge_w = (1.2 * w_in * w_out * cos).min(0.9);
                        let bridge_alpha = bridge_w * 2.0 + 1.0;
                        let bridge_beta = 2.0 - (bridge_w * 2.0) + 1.0;
                        let bridge_edge = Edge::new(
                            bridge_alpha, bridge_beta,
                            now_secs as f32,
                            EdgeType::Sequence,
                            EvidenceType::Inferred,
                            false,
                        );
                        edge_store.upsert(*in_src, *out_tgt, bridge_edge);
                    }
                }
            }
        }

        // Remove all edges and node
        edge_store.remove_node(node_id);
        node_store.remove(node_id);
        // Keep the lexical index in sync — no stale BM25 candidates.
        bm25_index.remove(node_id);
    }

    // --- Step 6: Merge near-identical nodes ---
    // Group by hour, find pairs with cosine > 0.95
    let ids = node_store.all_ids();
    let mut merged: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for i in 0..ids.len() {
        if merged.contains(&ids[i]) { continue; }
        for j in (i + 1)..ids.len() {
            if merged.contains(&ids[j]) { continue; }
            let (na, nb) = match (node_store.get(ids[i]), node_store.get(ids[j])) {
                (Some(a), Some(b)) => (a.clone(), b.clone()),
                _ => continue,
            };
            if na.node_type != nb.node_type { continue; }
            let time_diff = (na.timestamp - nb.timestamp).abs();
            if time_diff > 3600 { continue; }
            use crate::node_store::cosine;
            // Only merge nodes with the same model_id AND same embedding dim (N3)
            if na.model_id != nb.model_id || na.embedding.len() != nb.embedding.len() { continue; }
            let min_len = na.embedding.len();
            if cosine(&na.embedding[..min_len], &nb.embedding[..min_len]) > MERGE_COSINE {
                // Merge: average embedding as Vec<f32>, max salience, latest timestamp
                let avg_raw: Vec<f32> = na.embedding.iter().zip(nb.embedding.iter())
                    .map(|(a, b)| (a + b) * 0.5)
                    .collect();
                let avg = crate::node_store::l2_normalize(&avg_raw);
                let merged_salience = na.salience.max(nb.salience);
                let merged_ts = na.timestamp.max(nb.timestamp);

                // Update surviving node
                if let Some(node) = node_store.get_mut(ids[i]) {
                    node.embedding = avg;
                    node.salience = merged_salience;
                    node.timestamp = merged_ts;
                }

                // Re-point all edges from ids[j] to ids[i]
                let old_out = edge_store.outgoing(ids[j]).to_vec();
                for out in old_out {
                    let new_edge = out.edge.clone();
                    edge_store.upsert(ids[i], out.target, new_edge);
                }
                let old_in = edge_store.incoming_sources(ids[j]);
                for src in old_in {
                    if let Some(edge) = edge_store.get(src, ids[j]).cloned() {
                        edge_store.upsert(src, ids[i], edge);
                    }
                }
                edge_store.remove_node(ids[j]);
                // Adopt the loser's text if the survivor has none (keeps BM25 coverage).
                let loser_text = node_store.get(ids[j]).map(|n| n.text.clone()).unwrap_or_default();
                node_store.remove(ids[j]);
                bm25_index.remove(ids[j]);
                if !loser_text.is_empty() {
                    if let Some(survivor) = node_store.get_mut(ids[i]) {
                        if survivor.text.is_empty() {
                            survivor.text = loser_text.clone();
                        }
                    }
                    if let Some(survivor) = node_store.get(ids[i]) {
                        if !survivor.text.is_empty() {
                            bm25_index.insert(ids[i], &survivor.text.clone());
                        }
                    }
                }
                merged.insert(ids[j]);
            }
        }
    }

    // --- Step 7: Skill compilation ---
    compile_skills(node_store, edge_store, now_secs as i64);
}

fn compile_skills(node_store: &mut NodeStore, edge_store: &mut EdgeStore, now_ts: i64) {
    use crate::node::NodeType;
    let state_ids: Vec<u64> = node_store.iter()
        .filter(|(_, n)| n.node_type == NodeType::State)
        .map(|(&id, _)| id)
        .collect();

    let mut new_skills: Vec<(u64, Vec<f32>, String)> = Vec::new();

    for state_id in state_ids {
        let actions: Vec<u64> = edge_store.outgoing(state_id).iter()
            .filter(|e| e.edge.edge_type == EdgeType::Sequence)
            .map(|e| e.target)
            .collect();

        for action_id in actions {
            let outcomes: Vec<(u64, f32, f32)> = edge_store.outgoing(action_id).iter()
                .filter(|e| e.edge.edge_type == EdgeType::CausalHypothesis && e.edge.is_intervention)
                .map(|e| (e.target, e.edge.alpha, e.edge.beta))
                .collect();

            for (outcome_id, alpha, beta) in outcomes {
                let total_evidence = alpha + beta;
                let success_rate = alpha / total_evidence;
                if total_evidence > SKILL_MIN_EVIDENCE && success_rate > SKILL_MIN_SUCCESS_RATE {
                    // Compile skill: average embedding of state + action + outcome
                    if let (Some(sn), Some(an), Some(on)) = (
                        node_store.get(state_id),
                        node_store.get(action_id),
                        node_store.get(outcome_id),
                    ) {
                        // Only compile skill if all three nodes have the same dim (N3)
                        if sn.embedding.len() == an.embedding.len() && an.embedding.len() == on.embedding.len() {
                            let avg_raw: Vec<f32> = (0..sn.embedding.len())
                                .map(|i| (sn.embedding[i] + an.embedding[i] + on.embedding[i]) / 3.0)
                                .collect();
                            let avg = crate::node_store::l2_normalize(&avg_raw);
                            let mid = sn.model_id.clone();
                            new_skills.push((state_id, avg, mid));
                        }
                    }
                }
            }
        }
    }

    // Insert skill nodes and connect them
    for (state_id, skill_emb, model_id) in new_skills {
        let skill_node = crate::node::Node::new(skill_emb, model_id, 0.9, now_ts, 0.0, NodeType::Skill, 0.9, String::new());

        let skill_id = node_store.insert(skill_node);
        // High-confidence edge from state to skill
        let skill_edge = Edge::new(50.0, 1.0, now_ts as f32, EdgeType::Sequence, EvidenceType::Exact, false);
        edge_store.upsert(state_id, skill_id, skill_edge);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{Node, NodeType};

    #[test]
    fn test_cohorts_dynamic_dimension_384() {
        let mut cohorts = Cohorts::with_dim(384);
        assert_eq!(cohorts.dim(), 384);
        assert_eq!(cohorts.centroids.len(), NUM_COHORTS);
        assert_eq!(cohorts.centroids[0].len(), 384);

        let sample1 = vec![0.5f32; 384];
        let sample2 = vec![-0.5f32; 384];
        let label1 = cohorts.label(&sample1);
        let label2 = cohorts.label(&sample2);
        assert!(label1 < NUM_COHORTS);
        assert!(label2 < NUM_COHORTS);

        let samples = [&sample1[..], &sample2[..]];
        cohorts.update(&samples);
        assert_eq!(cohorts.dim(), 384);
    }

    #[test]
    fn test_cohorts_ensure_dim_transition() {
        let mut cohorts = Cohorts::new();
        assert_eq!(cohorts.dim(), 128);

        cohorts.ensure_dim(384);
        assert_eq!(cohorts.dim(), 384);
        assert_eq!(cohorts.centroids.len(), NUM_COHORTS);
        assert_eq!(cohorts.centroids[0].len(), 384);
    }

    #[test]
    fn test_run_consolidation_updates_384d_nodes() {
        let mut node_store = NodeStore::new();
        let mut edge_store = EdgeStore::new();
        let mut dense_core = DenseCore::new();
        let mut episode_log = EpisodeLog::new();
        let mut cohorts = Cohorts::new(); // starts at 128

        let node1 = Node::new(vec![0.1f32; 384], "minilm-l6-v2-int8".to_string(), 1.0, 100, 0.0, NodeType::State, 0.8, String::new());
        let node2 = Node::new(vec![0.2f32; 384], "minilm-l6-v2-int8".to_string(), 1.0, 100, 0.0, NodeType::State, 0.8, String::new());
        node_store.insert(node1);
        node_store.insert(node2);

        let mut rng = rand::thread_rng();
        let mut bm25_index = crate::bm25::Bm25Index::new();
        let projections = ProjectionRegistry::new();
        run_consolidation(
            &mut node_store,
            &mut edge_store,
            &mut dense_core,
            &mut episode_log,
            &mut cohorts,
            &mut bm25_index,
            &projections,
            200.0,
            &mut rng,
        );

        // Verify cohorts resized to 384 and processed nodes
        assert_eq!(cohorts.dim(), 384);
        assert_eq!(cohorts.centroids.len(), NUM_COHORTS);
        assert_eq!(cohorts.centroids[0].len(), 384);
    }
}
