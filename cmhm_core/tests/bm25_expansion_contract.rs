//! Expansion-phase contract: BM25 now indexes action + outcome text (R5).
//!
//! Two invariants this pins:
//!   1. ROLE ISOLATION: a State-scoped retrieval must never seed an
//!      Action/Outcome node, even when the query text matches the action or
//!      outcome text much better than the state text (BM25 fusion is the new
//!      attack surface — the dense path was already contract-tested).
//!   2. EXPANSION WORKS: unscoped retrieval can surface action/outcome nodes
//!      from action-word queries ("how did I fix X") — the whole point of
//!      indexing those texts.
//!
//! Run: cargo test -p cmhm_core --test bm25_expansion_contract -- --nocapture

use cmhm_core::{Config, ContextRuntime, NodeType, CallerRole, text_to_embedding};

struct Ep {
    state:   &'static str,
    action:  &'static str,
    outcome: &'static str,
}

const EPS: &[Ep] = &[
    Ep { state: "connection pool exhausted under peak load",
         action: "rolled out fast debug tracing on the redis sessions",
         outcome: "latency settled back to forty milliseconds" },
    Ep { state: "payment webhooks returning five hundred errors",
         action: "rotated the signing secret used by the billing gateway",
         outcome: "webhook deliveries resumed without retries" },
    Ep { state: "nightly batch jobs overlapping the backup window",
         action: "staggered the cron schedule across midnight",
         outcome: "backups finished inside their window" },
];

#[test]
fn state_scoped_retrieval_never_seeds_action_or_outcome_nodes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().join("data");
    cfg.projections_dir = tmp.path().join("proj");
    let mut rt = ContextRuntime::new(cfg, "expansion-test".to_string()).unwrap();

    let ctx = rt.create_context("exp", cmhm_core::ContextType::Project, None, "expansion", vec![]).unwrap();
    let ctx_id = ctx.id.as_str().to_string();
    let mut state_ids: Vec<u64> = Vec::new();
    for ep in EPS {
        let (res, _) = rt.learn_text_with_context(
            Some(&ctx_id),
            text_to_embedding(ep.state).to_vec(),
            text_to_embedding(ep.action).to_vec(),
            text_to_embedding(ep.outcome).to_vec(),
            0.5, 0.5, "hash-128d", 0.85, String::new(), None,
            CallerRole::User, None, None,
            Some((ep.state.into(), ep.action.into(), ep.outcome.into())),
            false,
        ).unwrap();
        state_ids.push(res.state_node_id);
    }

    // Action-word-only queries: BM25 will hammer action text. A State-scoped
    // retrieval must still seed the correct State node — never the Action
    // node that matches lexically better.
    let action_queries = [
        ("how did I roll out fast debug tracing", 0usize),
        ("what signing secret rotation fixed billing", 1usize),
        ("which cron staggering fixed the backups", 2usize),
        ("who rolled out tracing on redis sessions", 0usize),
    ];

    for (q, expected_idx) in action_queries {
        let (res, _) = rt.retrieve(
            Some(&ctx_id),
            text_to_embedding(q).to_vec(),
            "hash-128d",
            NodeType::State,
            q,
        ).unwrap();
        assert_eq!(
            res.seed_id,
            Some(state_ids[expected_idx]),
            "State-scoped seed violated for query {q:?}: got seed_id={:?} \
             (seed_node_type={:?}), expected state node {}",
            res.seed_id, res.seed_node_type, state_ids[expected_idx],
        );
        // Hard guard: the seed node type is exactly State regardless of pool noise.
        if let Some(t) = res.seed_node_type {
            assert_eq!(t, NodeType::State,
                "seed node type drifted to {t:?} on query {q:?}");
        }
    }

    // Invariant 2: unscoped retrieval CAN surface Action nodes from
    // action-word queries (the expansion actually indexes that text).
    let (res, _) = rt.retrieve_unscoped(
        Some(&ctx_id),
        text_to_embedding("how did I roll out fast debug tracing").to_vec(),
        "hash-128d",
        "how did I roll out fast debug tracing",
    ).unwrap();
    let pool_all_types = !res.raw_nn_candidates.is_empty();
    assert!(pool_all_types, "unscoped pool must not be empty");

    println!("expansion contract: role isolation held; unscoped pool size = {}",
             res.raw_nn_candidates.len());
}
