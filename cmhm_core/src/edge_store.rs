use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::edge::{Edge, EvidenceType};

/// An entry in the outgoing adjacency list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutEdge {
    pub target: u64,
    pub edge: Edge,
}

/// An entry in the incoming (reverse) adjacency list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InEdge {
    pub source: u64,
}

/// Edge store: adjacency lists for forward and reverse traversal.
///
/// out_edges[src] = Vec of (target, Edge)
/// in_edges[tgt]  = Vec of source IDs (for bridging and backward credit)
#[derive(Debug, Serialize, Deserialize)]
pub struct EdgeStore {
    pub out_edges: HashMap<u64, Vec<OutEdge>>,
    pub in_edges: HashMap<u64, Vec<InEdge>>,
}

impl EdgeStore {
    pub fn new() -> Self {
        Self {
            out_edges: HashMap::new(),
            in_edges: HashMap::new(),
        }
    }

    /// Insert or update an edge src→tgt.
    /// If the edge already exists, additively updates alpha/beta counts.
    pub fn upsert(&mut self, src: u64, tgt: u64, new_edge: Edge) {
        let out = self.out_edges.entry(src).or_default();
        if let Some(existing) = out.iter_mut().find(|e| e.target == tgt) {
            // Additive update: more evidence from same relationship
            existing.edge.alpha += new_edge.alpha - 1.0; // subtract prior
            existing.edge.beta  += new_edge.beta  - 1.0;
            existing.edge.alpha = existing.edge.alpha.max(0.1);
            existing.edge.beta  = existing.edge.beta.max(0.1);
            // Upgrade evidence type if new evidence is more reliable
            if new_edge.evidence_type == EvidenceType::Exact {
                existing.edge.evidence_type = EvidenceType::Exact;
            }
            if new_edge.is_intervention {
                existing.edge.is_intervention = true;
            }
        } else {
            out.push(OutEdge { target: tgt, edge: new_edge });
            self.in_edges.entry(tgt).or_default().push(InEdge { source: src });
        }
    }

    /// Get mutable reference to the edge src→tgt, if it exists.
    pub fn get_mut(&mut self, src: u64, tgt: u64) -> Option<&mut Edge> {
        self.out_edges.get_mut(&src)?
            .iter_mut()
            .find(|e| e.target == tgt)
            .map(|e| &mut e.edge)
    }

    /// Get immutable reference to the edge src→tgt.
    pub fn get(&self, src: u64, tgt: u64) -> Option<&Edge> {
        self.out_edges.get(&src)?
            .iter()
            .find(|e| e.target == tgt)
            .map(|e| &e.edge)
    }

    /// Outgoing edges from a node.
    pub fn outgoing(&self, src: u64) -> &[OutEdge] {
        self.out_edges.get(&src).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Incoming source IDs for a node.
    pub fn incoming_sources(&self, tgt: u64) -> Vec<u64> {
        self.in_edges.get(&tgt)
            .map(|v| v.iter().map(|e| e.source).collect())
            .unwrap_or_default()
    }

    /// Total degree (in + out) of a node. Used in node death criteria.
    /// C-02 FIX: Was out-degree only — Outcome nodes (leaf, out_degree=0) were always culled
    /// because eff_sal * 0 = 0 < NODE_DEATH_SCORE regardless of salience or age.
    pub fn degree(&self, node_id: u64) -> usize {
        let out = self.out_edges.get(&node_id).map(|v| v.len()).unwrap_or(0);
        let in_ = self.in_edges.get(&node_id).map(|v| v.len()).unwrap_or(0);
        out + in_
    }

    /// Remove a specific edge. Called during consolidation.
    pub fn remove_edge(&mut self, src: u64, tgt: u64) {
        if let Some(out) = self.out_edges.get_mut(&src) {
            out.retain(|e| e.target != tgt);
        }
        if let Some(inc) = self.in_edges.get_mut(&tgt) {
            inc.retain(|e| e.source != src);
        }
    }

    /// Remove ALL edges incident to a node (for node deletion).
    pub fn remove_node(&mut self, node_id: u64) -> Vec<OutEdge> {
        // Remove outgoing
        let outgoing = self.out_edges.remove(&node_id).unwrap_or_default();
        // Clean up reverse entries for each target
        for out in &outgoing {
            if let Some(inc) = self.in_edges.get_mut(&out.target) {
                inc.retain(|e| e.source != node_id);
            }
        }
        // Remove incoming
        if let Some(incoming) = self.in_edges.remove(&node_id) {
            for inc in &incoming {
                if let Some(out) = self.out_edges.get_mut(&inc.source) {
                    out.retain(|e| e.target != node_id);
                }
            }
        }
        outgoing
    }

    /// All edges as (src, tgt, &Edge) triples. Used during consolidation.
    pub fn all_edges(&self) -> Vec<(u64, u64, &Edge)> {
        let mut result = Vec::new();
        for (&src, outs) in &self.out_edges {
            for out in outs {
                result.push((src, out.target, &out.edge));
            }
        }
        result
    }

    /// Total edge count.
    pub fn len(&self) -> usize {
        self.out_edges.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::EdgeType;

    fn test_edge() -> Edge {
        Edge::new(1.0, 1.0, 0.0, EdgeType::Sequence, EvidenceType::Inferred, false)
    }

    #[test]
    fn test_insert_and_retrieve() {
        let mut store = EdgeStore::new();
        store.upsert(1, 2, test_edge());
        assert!(store.get(1, 2).is_some());
        assert!(store.get(2, 1).is_none()); // directed
    }

    #[test]
    fn test_remove_edge() {
        let mut store = EdgeStore::new();
        store.upsert(1, 2, test_edge());
        store.remove_edge(1, 2);
        assert!(store.get(1, 2).is_none());
    }

    #[test]
    fn test_degree() {
        let mut store = EdgeStore::new();
        store.upsert(1, 2, test_edge());
        store.upsert(1, 3, test_edge());
        assert_eq!(store.degree(1), 2);
    }
}
