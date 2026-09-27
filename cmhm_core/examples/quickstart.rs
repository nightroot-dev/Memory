//! Quickstart: learn two causal chains, retrieve the relevant one.
//!
//! Run:
//!   cargo run --example quickstart -p cmhm_core
//!
//! Uses the built-in hash embedder (no ONNX required).
//! Data lives in a tempdir so each run starts clean.

use cmhm_core::authority::CallerRole;
use cmhm_core::engine;
use cmhm_core::config::Config;
use cmhm_core::text_to_embedding;
use cmhm_core::NodeType;
use tempfile::tempdir;

fn main() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = tmp.path().join("projections");

    println!("[quickstart] starting engine in {}", tmp.path().display());
    let handle = engine::start(cfg)?;

    let events = [
        (
            "database connection pool exhausted under load",
            "increased pool size to 100",
            "connection timeouts stopped",
        ),
        (
            "kubernetes pod entered crash loop after config change",
            "rolled back the deployment",
            "pods stabilized within a minute",
        ),
    ];

    for (state, action, outcome) in &events {
        let r = handle.learn_with_text_context(
            text_to_embedding(state).to_vec(),
            text_to_embedding(action).to_vec(),
            text_to_embedding(outcome).to_vec(),
            0.2, 0.5, "hash-128d",
            0.5, String::new(), None,
            CallerRole::User, None, None,
            Some((state.to_string(), action.to_string(), outcome.to_string())),
        )?;
        println!(
            "[quickstart] learned: state={} action={} outcome={}",
            r.state_node_id, r.action_node_id, r.outcome_node_id
        );
    }

    let query_text = "connections are failing again";
    let q_emb = text_to_embedding(query_text).to_vec();
    let result = handle.retrieve(q_emb, "hash-128d", NodeType::State, query_text)?;

    println!();
    println!("[quickstart] query: {query_text}");
    println!("[quickstart] seed node id: {:?}", result.seed_id);
    println!("[quickstart] candidate pool: {:?}", result.post_filter_candidates);
    println!("[quickstart] causal chain: {}", result.explanation);

    handle.shutdown()?;
    println!();
    println!("[quickstart] done");
    Ok(())
}
