//! CMHM_EMBEDDER selection: the model id the MCP schema defaults to must always equal
//! the id of the embedder actually loaded (C-04), and a typo must fail loudly.
//!
//! Needs the ONNX runtime + cached models, and mutates process env, so it is a single
//! sequential test and is `#[ignore]`d.
//!
//! Run: cargo test -p cmhm_core --features onnx --test embedder_select -- --ignored --nocapture
#![cfg(feature = "onnx")]

use cmhm_core::embed::{default_embedder, default_model_id};

#[test]
#[ignore]
fn cmhm_embedder_selection() {
    // Unset -> MiniLM (the default).
    std::env::remove_var("CMHM_EMBEDDER");
    assert_eq!(default_model_id(), "minilm-l6-v2-int8");
    assert_eq!(default_embedder().unwrap().model_id(), "minilm-l6-v2-int8");

    // bge -> BGE-base, ids agree.
    std::env::set_var("CMHM_EMBEDDER", "bge");
    assert_eq!(default_model_id(), "bge-base-en-v1.5");
    let bge = default_embedder().unwrap();
    assert_eq!(bge.model_id(), default_model_id());
    assert_eq!(bge.dim(), 768);

    // Typo -> hard error, never a silent fallback to another model.
    std::env::set_var("CMHM_EMBEDDER", "bgee");
    assert!(default_embedder().is_err(), "unknown CMHM_EMBEDDER must be rejected");

    std::env::remove_var("CMHM_EMBEDDER");
}
