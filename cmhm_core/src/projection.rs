use serde::{Deserialize, Serialize};
use crate::node_store::{l2_normalize, has_nan};
use crate::EMBED_DIM;

/// Cross-model linear projection.
///
/// Maps embeddings from a specific model's space into the shared EMBED_DIM CMHM space:
///   e_proj = normalize(P * e_raw + b)
///
/// P is an EMBED_DIM×D matrix (D = input dimension, e.g., EMBED_DIM for identity, 768 for typical models).
/// For the identity case (D=EMBED_DIM), P = I and b = 0.
///
/// Projection matrices are loaded from binary files at startup.
/// File format: [D: u32 LE][P: D*EMBED_DIM f32 LE][b: EMBED_DIM f32 LE]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projection {
    /// Input dimension (number of columns in P).
    pub input_dim: usize,
    /// Matrix P flattened row-major: EMBED_DIM rows × input_dim cols
    pub matrix: Vec<f32>,
    /// Bias vector, EMBED_DIM floats.
    #[serde(with = "crate::serde_helpers::f32_128")]
    pub bias: [f32; EMBED_DIM],
}

impl Projection {
    /// Identity projection for EMBED_DIM input (no-op except normalize).
    pub fn identity() -> Self {
        let mut matrix = vec![0.0f32; EMBED_DIM * EMBED_DIM];
        for i in 0..EMBED_DIM {
            matrix[i * EMBED_DIM + i] = 1.0;
        }
        Self { input_dim: EMBED_DIM, matrix, bias: [0.0f32; EMBED_DIM] }
    }

    /// Apply projection: out = normalize(P * e + b)
    /// Input e must have length == self.input_dim.
    pub fn project(&self, e: &[f32]) -> Result<[f32; EMBED_DIM], ProjectionError> {
        if e.len() != self.input_dim {
            return Err(ProjectionError::DimMismatch {
                expected: self.input_dim,
                got: e.len(),
            });
        }

        let mut out = [0.0f32; EMBED_DIM];
        // out = P * e + b
        for row in 0..EMBED_DIM {
            let mut acc = self.bias[row];
            for col in 0..self.input_dim {
                acc += self.matrix[row * self.input_dim + col] * e[col];
            }
            out[row] = acc;
        }

        if has_nan(&out) {
            return Err(ProjectionError::NanOutput);
        }

        let norm_vec = l2_normalize(&out);
        let mut result = [0.0f32; EMBED_DIM];
        result.copy_from_slice(&norm_vec[..EMBED_DIM]);
        Ok(result)
    }

    /// Load from binary file format.
    /// Format: [input_dim: u32 LE][matrix: input_dim*EMBED_DIM f32 LE][bias: EMBED_DIM f32 LE]
    pub fn from_bytes(data: &[u8]) -> Result<Self, ProjectionError> {
        if data.len() < 4 {
            return Err(ProjectionError::InvalidFile);
        }
        let input_dim = u32::from_le_bytes(
            data[0..4].try_into().map_err(|_| ProjectionError::InvalidFile)?
        ) as usize;
        let matrix_bytes = input_dim * EMBED_DIM * 4;
        let bias_bytes   = EMBED_DIM * 4;
        if data.len() < 4 + matrix_bytes + bias_bytes {
            return Err(ProjectionError::InvalidFile);
        }

        let mut matrix = vec![0.0f32; input_dim * EMBED_DIM];
        for (i, chunk) in data[4..4 + matrix_bytes].chunks(4).enumerate() {
            matrix[i] = f32::from_le_bytes(
                chunk.try_into().map_err(|_| ProjectionError::InvalidFile)?
            );
        }
        let mut bias = [0.0f32; EMBED_DIM];
        for (i, chunk) in data[4 + matrix_bytes..4 + matrix_bytes + bias_bytes].chunks(4).enumerate() {
            bias[i] = f32::from_le_bytes(
                chunk.try_into().map_err(|_| ProjectionError::InvalidFile)?
            );
        }

        Ok(Self { input_dim, matrix, bias })
    }
}

/// Cross-model projection registry.
/// Keyed by model_id string (e.g., "model-a", "claude", "openai").
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProjectionRegistry {
    projections: std::collections::HashMap<String, Projection>,
}

impl ProjectionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a projection for a model.
    pub fn register(&mut self, model_id: String, proj: Projection) {
        self.projections.insert(model_id, proj);
    }

    /// Project an embedding using the model's projection, or identity if not found.
    pub fn project(&self, model_id: &str, e: &[f32]) -> Result<[f32; EMBED_DIM], ProjectionError> {
        match self.projections.get(model_id) {
            Some(proj) => proj.project(e),
            None => {
                // Identity fallback: expect exactly EMBED_DIM dims
                if e.len() != EMBED_DIM {
                    return Err(ProjectionError::NoProjectionFound {
                        model_id: model_id.to_string(),
                    });
                }
                let mut arr = [0.0f32; EMBED_DIM];
                arr.copy_from_slice(e);
                if has_nan(&arr) {
                    return Err(ProjectionError::NanOutput);
                }
                let norm_vec = l2_normalize(&arr);
                let mut result = [0.0f32; EMBED_DIM];
                result.copy_from_slice(&norm_vec[..EMBED_DIM]);
                Ok(result)
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("Dimension mismatch: expected {expected}, got {got}")]
    DimMismatch { expected: usize, got: usize },
    #[error("NaN in projection output — bad embedding or ill-conditioned matrix")]
    NanOutput,
    #[error("Invalid projection file format")]
    InvalidFile,
    #[error("No projection found for model '{model_id}' and input is not {EMBED_DIM}-dim")]
    NoProjectionFound { model_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_projection() {
        let proj = Projection::identity();
        let mut e = [0.0f32; EMBED_DIM];
        e[0] = 3.0; e[1] = 4.0; // norm = 5
        let out = proj.project(&e).unwrap();
        // After identity + normalize: e[0]=0.6, e[1]=0.8
        assert!((out[0] - 0.6).abs() < 1e-5, "e[0] should be 0.6, got {}", out[0]);
        assert!((out[1] - 0.8).abs() < 1e-5);
    }

    #[test]
    fn test_dim_mismatch_error() {
        let proj = Projection::identity();
        let e = vec![1.0f32; 64]; // wrong dim
        assert!(proj.project(&e).is_err());
    }
}
