//! Wire protocol: newline-delimited JSON, one request per line.
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedRequest {
    pub embed: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedResponse {
    pub model: String,
    pub dim: usize,
    pub embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}
