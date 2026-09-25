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

    /// Load all `*.bin` files in `dir` and register each as a projection
    /// keyed by its filename stem.
    ///
    /// Returns the number of projections successfully loaded. Files that
    /// fail to read or parse are logged as warnings and skipped — a
    /// corrupt file must not prevent engine startup.
    ///
    /// Missing or unreadable directory is treated as "no projections"
    /// and returns 0 (not an error).
    pub fn load_from_dir(&mut self, dir: &std::path::Path) -> usize {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return 0,
        };
        let mut loaded = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("bin") {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => continue,
            };
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(
                        "projection {}: read failed: {e}",
                        path.display()
                    );
                    continue;
                }
            };
            match Projection::from_bytes(&bytes) {
                Ok(proj) => {
                    tracing::info!(
                        model_id = %stem,
                        input_dim = proj.input_dim,
                        "loaded projection from {}",
                        path.display()
                    );
                    self.projections.insert(stem, proj);
                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        "projection {}: parse failed: {e}",
                        path.display()
                    );
                }
            }
        }
        loaded
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

    /// Fail-closed contract: 384-dim input with no registered projection
    /// must Err(NoProjectionFound), never silently truncate.
    #[test]
    fn projection_registry_fails_closed_on_dim_mismatch() {
        let reg = ProjectionRegistry::new();
        let e = vec![0.0f32; 384];
        match reg.project("model_with_no_projection", &e) {
            Err(ProjectionError::NoProjectionFound { model_id }) => {
                assert_eq!(model_id, "model_with_no_projection");
            }
            other => panic!("expected NoProjectionFound, got {:?}", other),
        }
    }

    /// Identity path: 128-dim input with no registered projection succeeds.
    #[test]
    fn projection_registry_identity_for_128_dim() {
        let reg = ProjectionRegistry::new();
        let e = vec![0.1f32; 128];
        let out = reg.project("hash-128d", &e).expect("identity path must succeed");
        assert_eq!(out.len(), 128);
    }

    /// Registered matrix path: fake 384→128 projection (identity on the
    /// first 128 dims) yields 128-dim unit-norm output.
    #[test]
    fn projection_registry_uses_registered_matrix() {
        let mut matrix = vec![0.0f32; EMBED_DIM * 384];
        for row in 0..EMBED_DIM {
            matrix[row * 384 + row] = 1.0;
        }
        let proj = Projection {
            input_dim: 384,
            matrix,
            bias: [0.0f32; EMBED_DIM],
        };
        let mut reg = ProjectionRegistry::new();
        reg.register("fake-384d".to_string(), proj);
        let mut e = vec![0.0f32; 384];
        e[0] = 3.0;
        e[1] = 4.0; // norm = 5 → [0.6, 0.8, ...]
        let out = reg.project("fake-384d", &e).expect("registered projection must succeed");
        assert_eq!(out.len(), 128);
        assert!((out[0] - 0.6).abs() < 1e-5, "e[0] should be 0.6, got {}", out[0]);
        assert!((out[1] - 0.8).abs() < 1e-5, "e[1] should be 0.8, got {}", out[1]);
        let norm = out.iter().map(|f| f * f).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "output must be unit-norm, got {}", norm);
    }
}
