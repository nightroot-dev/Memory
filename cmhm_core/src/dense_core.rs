use serde::{Deserialize, Serialize, Serializer, Deserializer};
use crate::EMBED_DIM;

const SLICES: usize = 8;
const SLICE_LEN: usize = EMBED_DIM * EMBED_DIM;

/// Dense Core — SLICES-slice EMBED_DIM×EMBED_DIM tensor mirror of the sparse graph.
///
/// Provides O(1) approximate causal context vector from a query:
///   c = (1/SLICES) Σ_k C_k * q
///
/// Each slice is updated via the delta rule on every edge change:
///   C_k ← C_k (I - η_c e_i e_i^T) + η_c w (e_j ⊗ e_i)
///
/// This keeps the core self-cleaning without SVD recompression.
/// Memory: SLICES × EMBED_DIM × EMBED_DIM × 4 bytes = 524,288 bytes (~512 KB at 8×128×128)
#[derive(Debug)]
pub struct DenseCore {
    /// SLICES slices of EMBED_DIM×EMBED_DIM f32 matrices, row-major flattened.
    /// Indexed as: core[slice][row * EMBED_DIM + col]
    data: Box<[[f32; SLICE_LEN]; SLICES]>,
}

impl DenseCore {
    pub fn new() -> Self {
        Self {
            data: Box::new([[0.0f32; SLICE_LEN]; SLICES]),
        }
    }

    /// Determine which slice to use for an edge based on edge type + node types.
    /// Hashes (edge_type_u8, src_type_u8, tgt_type_u8) mod SLICES.
    pub fn slice_index(edge_type: u8, src_type: u8, tgt_type: u8) -> usize {
        let h = (edge_type as usize)
            .wrapping_mul(31)
            .wrapping_add(src_type as usize)
            .wrapping_mul(31)
            .wrapping_add(tgt_type as usize);
        h % SLICES
    }

    /// Apply delta rule update for edge i→j with weight w on slice k.
    ///
    /// C_k ← C_k (I - η_c e_i e_i^T) + η_c w (e_j ⊗ e_i)
    ///
    /// Efficient computation:
    ///   For each row r, col c:
    ///     C[r][c] += η_c * (-e_i[c] * (C[r] · e_i) + w * e_j[r] * e_i[c])
    ///
    /// This is the correct row-wise expansion of the outer product update.
    pub fn update(&mut self, slice: usize, e_i: &[f32; EMBED_DIM], e_j: &[f32; EMBED_DIM], w: f32) {
        const ETA_C: f32 = 0.01;
        let k = slice;
        let c = &mut self.data[k];

        // Pre-compute (C_k * e_i): the current "query response" for key e_i
        // c_ei[r] = Σ_col C[r][col] * e_i[col]
        let mut c_ei = [0.0f32; EMBED_DIM];
        for r in 0..EMBED_DIM {
            let mut acc = 0.0f32;
            for col in 0..EMBED_DIM {
                acc += c[r * EMBED_DIM + col] * e_i[col];
            }
            c_ei[r] = acc;
        }

        // Apply update:
        // C[r][col] += η_c * (w * e_j[r] * e_i[col] - c_ei[r] * e_i[col])
        //            = η_c * e_i[col] * (w * e_j[r] - c_ei[r])
        for r in 0..EMBED_DIM {
            let diff = ETA_C * (w * e_j[r] - c_ei[r]);
            for col in 0..EMBED_DIM {
                c[r * EMBED_DIM + col] += diff * e_i[col];
            }
        }
    }

    /// Compute the dense context vector for a query q.
    ///   c = (1/SLICES) Σ_{k=0}^{SLICES-1} C_k q
    pub fn query(&self, q: &[f32; EMBED_DIM]) -> [f32; EMBED_DIM] {
        let mut out = [0.0f32; EMBED_DIM];
        for k in 0..SLICES {
            let c = &self.data[k];
            for r in 0..EMBED_DIM {
                let mut acc = 0.0f32;
                for col in 0..EMBED_DIM {
                    acc += c[r * EMBED_DIM + col] * q[col];
                }
                out[r] += acc;
            }
        }
        // Divide by SLICES (uniform slice weighting)
        let scale = 1.0 / SLICES as f32;
        for v in out.iter_mut() {
            *v *= scale;
        }
        out
    }

    /// Remove an edge's influence from the associative matrix.
    ///
    /// H-04 fix: The previous implementation called `update(w=0)`, which applies the
    /// delta rule with a zero target:
    ///   `C[r][col] += η_c * (0·e_j[r] - c_ei[r]) * e_i[col]`
    ///            `= -η_c * c_ei[r] * e_i[col]`
    ///
    /// This **anti-associates** — it actively pushes the matrix away from the current
    /// association, corrupting context vectors for all nodes that share a subspace with
    /// e_i. After enough consolidation pruning passes the dense context vector accumulates
    /// systematic anti-associations for dead relationships.
    ///
    /// Correct semantics: removing an edge means "stop reinforcing this association."
    /// Dead edges leave a small decaying residual through disuse (subsequent learns with
    /// other targets naturally dilute the old association). This is standard Hebbian
    /// forgetting — not active negation.
    #[allow(unused_variables)]
    pub fn remove_edge(&mut self, slice: usize, e_i: &[f32; EMBED_DIM], e_j: &[f32; EMBED_DIM]) {
        // No-op: do not anti-associate. Residual naturally decays through disuse.
    }
}

/// Manual Serialize: flatten SLICES×EMBED_DIM×EMBED_DIM f32s to Vec<f32>.
impl Serialize for DenseCore {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let flat: Vec<f32> = self.data.iter()
            .flat_map(|sl| sl.iter().copied())
            .collect();
        flat.serialize(s)
    }
}

/// Manual Deserialize: reconstruct from flat Vec<f32>.
impl<'de> Deserialize<'de> for DenseCore {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let flat: Vec<f32> = Vec::deserialize(d)?;
        let expected = SLICES * SLICE_LEN;
        if flat.len() != expected {
            return Err(serde::de::Error::custom(format!(
                "DenseCore expected {} floats, got {}", expected, flat.len()
            )));
        }
        let mut data = Box::new([[0.0f32; SLICE_LEN]; SLICES]);
        for k in 0..SLICES {
            data[k].copy_from_slice(&flat[k * SLICE_LEN..(k + 1) * SLICE_LEN]);
        }
        Ok(Self { data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_and_query() {
        let mut core = DenseCore::new();
        let mut e_i = [0.0f32; EMBED_DIM]; e_i[0] = 1.0;
        let mut e_j = [0.0f32; EMBED_DIM]; e_j[1] = 1.0;

        // Store association e_i → e_j with weight 1.0
        for _ in 0..100 {
            core.update(0, &e_i, &e_j, 1.0);
        }

        // Query with e_i; context should be biased toward e_j direction
        let ctx = core.query(&e_i);
        assert!(ctx[1].abs() > 0.0, "context should encode e_j direction, ctx[1]={}", ctx[1]);
    }

    #[test]
    fn test_slice_index_deterministic() {
        let s1 = DenseCore::slice_index(0, 1, 2);
        let s2 = DenseCore::slice_index(0, 1, 2);
        assert_eq!(s1, s2);
        assert!(s1 < SLICES);
    }
}
