//! B1b acceptance tests (ledger S-15): learn-event ledger + triple/candidate join.
//!
//! TDD gate: written BEFORE implementation — must FAIL on pre-B1b code
//! (missing `learn_ledger` module / `RetrievalResult` fields → compile error,
//! or assertion failures). Implementation must make them pass WITHOUT changing
//! scoring, filtering, or ordering.
//!
//! - T1 triple roundtrip: learn with text → retrieve → seed episode ids/texts match.
//! - T2 top-3 scored-order: candidate_texts == scored top-3 order, no rerank.
//! - T3 dedup +N: identical learns share state node; most-recent triple wins;
//!   shared_count == episodes_sharing - 1.
//! - T4 consolidation graceful: consolidation never breaks episode resolution;
//!   missing nodes resolve gracefully (None text), never panic.
//! - T5 ledgerless/vector-only markers: pre-B1b (empty ledger) → NoTriple;
//!   vector-only learn → Found triple with empty texts. Markers distinct.
//! - T6 thresholds pinned: scoring/config thresholds unchanged by B1b.
//! - EVICT rider: fill past 100k cap → oldest gone with evicted-marker,
//!   index consistent, index never persisted (rebuild on load), index memory counted.

use cmhm_core::authority::CallerRole;
use cmhm_core::config::Config;
use cmhm_core::engine;
use cmhm_core::learn_ledger::{
    LearnLedger, EVICTED_MARKER, MAX_LEARN_EPISODES, NO_TRIPLE_MARKER,
};
use cmhm_core::retrieval::EpisodeStatus;
use cmhm_core::NodeType;

// ── deterministic helpers ─────────────────────────────────────────────

fn emb(seed: u32) -> Vec<f32> {
    let mut v = vec![0.0f32; 128];
    let mut x = seed as u64;
    for f in v.iter_mut() {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *f = ((x >> 33) as f32) / (u32::MAX as f32) - 0.5;
    }
    let norm = v.iter().map(|f| f * f).sum::<f32>().sqrt().max(1e-9);
    v.iter_mut().for_each(|f| *f /= norm);
    v
}

fn test_cfg(dir: &std::path::Path) -> Config {
    let mut cfg = Config::default();
    cfg.data_dir = dir.to_path_buf();
    cfg.projections_dir = dir.join("projections");
    // Keep consolidation out of the way unless T4 explicitly wants it.
    cfg.consolidation_node_trigger = 10_000_000;
    cfg.consolidation_reinforce_trigger = 1e18;
    cfg.consolidation_interval_hours = 1e18;
    cfg
}

fn tmpdir(tag: &str) -> tempfile::TempDir {
    static CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = CTR.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "cmhm_b1b_{tag}_{}_{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    tempfile::tempdir_in(std::env::temp_dir()).unwrap()
}

// ── T1: triple roundtrip ────────────────────────────────────────────────

#[test]
fn b1b_t1_triple_roundtrip() {
    let tmp = tmpdir("t1");
    // Use our own dir (tmpdir helper above returns an anonymous dir; build cfg from its path).
    let dir = tmp.path().to_path_buf();
    let cfg = test_cfg(&dir);
    let handle = engine::start(cfg).unwrap();

    let s_txt = "b1b t1 state alpha unique zb-t1-s";
    let a_txt = "b1b t1 action beta unique zb-t1-a";
    let o_txt = "b1b t1 outcome gamma unique zb-t1-o";
    let lr = handle
        .learn_with_text_context(
            cmhm_core::text_to_embedding(s_txt).to_vec(),
            cmhm_core::text_to_embedding(a_txt).to_vec(),
            cmhm_core::text_to_embedding(o_txt).to_vec(),
            0.9,
            0.5,
            "hash-128d",
            0.5,
            String::new(),
            None,
            CallerRole::User,
            None,
            None,
            Some((s_txt.to_string(), a_txt.to_string(), o_txt.to_string())),
        )
        .unwrap();

    let r = handle
        .retrieve(
            cmhm_core::text_to_embedding(s_txt).to_vec(),
            "hash-128d",
            NodeType::State,
            s_txt,
        )
        .unwrap();

    assert_eq!(r.seed_id, Some(lr.state_node_id), "T1: seed must be the learned state");
    assert_eq!(r.seed_episode_status, EpisodeStatus::Found, "T1: episode must resolve Found");
    let ep = r
        .seed_episode
        .as_ref()
        .expect("T1: Found status must carry the episode view");
    assert_eq!(ep.seq, 1, "T1: first learn must be seq 1");
    assert_eq!(ep.state_id, lr.state_node_id, "T1: state id must match learn");
    assert_eq!(ep.action_id, lr.action_node_id, "T1: action id must match learn");
    assert_eq!(ep.outcome_id, lr.outcome_node_id, "T1: outcome id must match learn");
    assert_eq!(ep.state_text.as_deref(), Some(s_txt), "T1: state text live-resolved");
    assert_eq!(ep.action_text.as_deref(), Some(a_txt), "T1: action text live-resolved");
    assert_eq!(ep.outcome_text.as_deref(), Some(o_txt), "T1: outcome text live-resolved");
    assert_eq!(ep.shared_count, 0, "T1: unique learn must have no sharers");
    assert!(ep.timestamp > 0, "T1: episode timestamp must be set");
    handle.shutdown().unwrap();
}

// ── T2: top-3 scored-order, no rerank ───────────────────────────────────

#[test]
fn b1b_t2_top3_scored_order_no_rerank() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_cfg(tmp.path());
    let handle = engine::start(cfg).unwrap();

    for i in 0..5 {
        let s = format!("b1b t2 state distinct topic {i} zb-t2-s{i}");
        let a = format!("b1b t2 action distinct {i} zb-t2-a{i}");
        let o = format!("b1b t2 outcome distinct {i} zb-t2-o{i}");
        handle
            .learn_with_text_context(
                cmhm_core::text_to_embedding(&s).to_vec(),
                cmhm_core::text_to_embedding(&a).to_vec(),
                cmhm_core::text_to_embedding(&o).to_vec(),
                0.9,
                0.5,
                "hash-128d",
                0.5,
                String::new(),
                None,
                CallerRole::User,
                None,
                None,
                Some((s, a, o)),
            )
            .unwrap();
    }

    let q = "b1b t2 state distinct topic";
    let r = handle
        .retrieve(
            cmhm_core::text_to_embedding(q).to_vec(),
            "hash-128d",
            NodeType::State,
            q,
        )
        .unwrap();

    assert!(!r.scored_candidates.is_empty(), "T2: need scored candidates");
    let expect_n = r.scored_candidates.len().min(3);
    assert_eq!(
        r.candidate_texts.len(),
        expect_n,
        "T2: candidate_texts must be top-min(3, scored)"
    );
    for (i, cand) in r.candidate_texts.iter().enumerate() {
        assert_eq!(
            cand.node_id, r.scored_candidates[i].0,
            "T2: candidate {i} must equal scored rank {i} (no rerank)"
        );
        assert!(!cand.text.is_empty(), "T2: text-path candidates must carry text");
        assert!(cand.timestamp > 0, "T2: candidate timestamp must be set");
    }
    // Scored order itself must still be descending (join must not have touched it).
    for w in r.scored_candidates.windows(2) {
        assert!(
            w[0].1 + 1e-6 >= w[1].1,
            "T2: scored_candidates must stay descending"
        );
    }
    handle.shutdown().unwrap();
}

// ── T3: dedup +N, most-recent wins ──────────────────────────────────────

#[test]
fn b1b_t3_dedup_shared_most_recent_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_cfg(tmp.path());
    let handle = engine::start(cfg).unwrap();

    // Identical embeddings + low surprise/eta → birth gate dedups onto one state node.
    let s = emb(9001);
    let a = emb(9002);
    let o = emb(9003);
    let s_txt = "b1b t3 repeated state";
    let mut ids = vec![];
    for k in 0..3 {
        let lr = handle
            .learn_with_text_context(
                s.clone(),
                a.clone(),
                o.clone(),
                0.1,
                0.1,
                "hash-128d",
                0.5,
                String::new(),
                None,
                CallerRole::User,
                None,
                None,
                if k == 0 {
                    Some((
                        s_txt.to_string(),
                        "b1b t3 action".to_string(),
                        "b1b t3 outcome".to_string(),
                    ))
                } else {
                    None
                },
            )
            .unwrap();
        ids.push((lr.state_node_id, lr.action_node_id, lr.outcome_node_id));
    }
    assert_eq!(ids[0].0, ids[1].0, "T3: repeated state must dedup to one node");
    assert_eq!(ids[1].0, ids[2].0, "T3: third repeat must share the node");

    let r = handle.retrieve(s.clone(), "hash-128d", NodeType::State, "").unwrap();
    assert_eq!(r.seed_id, Some(ids[0].0), "T3: seed must be the shared node");
    assert_eq!(r.seed_episode_status, EpisodeStatus::Found);
    let ep = r.seed_episode.as_ref().expect("T3: Found must carry view");
    assert_eq!(ep.seq, 3, "T3: most-recent triple wins (seq 3 of 3)");
    assert_eq!(ep.shared_count, 2, "T3: 3 episodes share the node → +2 others");
    handle.shutdown().unwrap();
}

// ── T4: consolidation graceful ──────────────────────────────────────────

#[test]
fn b1b_t4_consolidation_graceful() {
    // Engine level: consolidation runs but never breaks episode resolution.
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_cfg(tmp.path());
    cfg.consolidation_node_trigger = 1; // force consolidation to run on next command
    let handle = engine::start(cfg).unwrap();
    let s_txt = "b1b t4 state consolidation";
    let lr = handle
        .learn_with_text_context(
            cmhm_core::text_to_embedding(s_txt).to_vec(),
            cmhm_core::text_to_embedding("b1b t4 action").to_vec(),
            cmhm_core::text_to_embedding("b1b t4 outcome").to_vec(),
            0.9,
            0.5,
            "hash-128d",
            0.5,
            String::new(),
            None,
            CallerRole::User,
            None,
            None,
            Some((
                s_txt.to_string(),
                "b1b t4 action".to_string(),
                "b1b t4 outcome".to_string(),
            )),
        )
        .unwrap();
    // This retrieve triggers the consolidation check; fresh nodes are never culled
    // (14-day age guard), so the triple must survive intact — no panic, Found.
    let r = handle
        .retrieve(
            cmhm_core::text_to_embedding(s_txt).to_vec(),
            "hash-128d",
            NodeType::State,
            s_txt,
        )
        .unwrap();
    assert_eq!(r.seed_id, Some(lr.state_node_id));
    assert_eq!(r.seed_episode_status, EpisodeStatus::Found);
    assert!(r.seed_episode.is_some(), "T4: ledger must survive consolidation");

    // Unit level: an episode referencing a culled (missing) node resolves
    // gracefully — None text for the missing endpoint, never a panic.
    let mut ledger = LearnLedger::new();
    let seq = ledger.append(111, 222, 333, 1_700_000_000);
    assert_eq!(seq, 1);
    // Resolve against an EMPTY store: all three endpoints missing.
    let empty_store = cmhm_core::node_store::NodeStore::new();
    let view = ledger.resolve_node_view(111, &empty_store);
    assert!(view.is_some(), "T4: view must exist even with missing nodes");
    let view = view.unwrap();
    assert_eq!(view.state_text, None, "T4: missing state node → None text");
    assert_eq!(view.action_text, None, "T4: missing action node → None text");
    assert_eq!(view.outcome_text, None, "T4: missing outcome node → None text");
    handle.shutdown().unwrap();
}

// ── T5: ledgerless vs vector-only markers ───────────────────────────────

#[test]
fn b1b_t5_ledgerless_vs_vector_only_markers() {
    // Markers must be distinct strings (rider).
    assert_ne!(NO_TRIPLE_MARKER, EVICTED_MARKER, "T5: markers must be distinct");
    assert!(
        NO_TRIPLE_MARKER.contains("no triple recorded"),
        "T5: pre-B1b marker text pinned"
    );
    assert!(
        EVICTED_MARKER.contains("evicted"),
        "T5: evicted marker text pinned"
    );

    // Ledgerless (pre-B1b): nodes exist, ledger empty → NoTriple, no episode,
    // but candidate texts still resolve (read-only join is independent of ledger).
    let mut store = cmhm_core::node_store::NodeStore::new();
    let n = cmhm_core::node::Node::new(
        vec![1.0f32; 128],
        "hash-128d".to_string(),
        0.5,
        1_700_000_000,
        0.5,
        NodeType::State,
        0.5,
        String::new(),
    )
    .with_text("pre-b1b legacy state text");
    let nid = store.insert(n);
    let ledger = LearnLedger::new();
    assert_eq!(ledger.len(), 0);
    let (status, ep, shared) = ledger.resolve_node(nid);
    assert_eq!(status, EpisodeStatus::NoTriple, "T5: empty ledger → NoTriple");
    assert!(ep.is_none());
    assert_eq!(shared, 0);

    // Vector-only learn via engine: triple EXISTS (Found) but texts are empty.
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_cfg(tmp.path());
    let handle = engine::start(cfg).unwrap();
    let lr = handle
        .learn_with_context(
            emb(7001),
            emb(7002),
            emb(7003),
            0.9,
            0.5,
            "hash-128d",
            0.5,
            String::new(),
            None,
            CallerRole::User,
            None,
            None,
        )
        .unwrap();
    let r = handle.retrieve(emb(7001), "hash-128d", NodeType::State, "").unwrap();
    assert_eq!(r.seed_id, Some(lr.state_node_id));
    assert_eq!(
        r.seed_episode_status,
        EpisodeStatus::Found,
        "T5: vector-only learn still records a triple"
    );
    let ep = r.seed_episode.as_ref().expect("T5: Found must carry view");
    assert_eq!(ep.state_text, Some(String::new()), "T5: vector-only state text is empty");
    assert_eq!(ep.action_text, Some(String::new()));
    assert_eq!(ep.outcome_text, Some(String::new()));
    handle.shutdown().unwrap();
}

// ── T6: thresholds pinned ───────────────────────────────────────────────

#[test]
fn b1b_t6_thresholds_pinned() {
    let cfg = Config::default();
    assert!((cfg.retrieval_authority_weight - 0.25).abs() < 1e-6, "authority α=0.25 pinned");
    assert!((cfg.retrieval_query_weight - 0.7).abs() < 1e-6, "query weight 0.7 pinned");
    assert!((cfg.retrieval_context_weight - 0.3).abs() < 1e-6, "context weight 0.3 pinned");
    assert!((cfg.retrieval_curiosity_threshold - 0.1).abs() < 1e-6, "curiosity 0.1 pinned");
    assert!((cfg.conflict_detection_threshold - 0.0).abs() < 1e-6, "conflict gap 0.0 pinned");
    assert_eq!(cfg.abstain_on_conflict, true, "abstain_on_conflict pinned true");
    assert_eq!(cfg.retrieval_seed_pool_size, 20, "seed pool 20 pinned");
    assert!((cfg.gate_threshold - 0.35).abs() < 1e-6, "gate 0.35 pinned");
    assert!((cfg.gate_kappa - 2.0).abs() < 1e-6, "kappa 2.0 pinned");
    assert_eq!(
        cmhm_core::retrieval::TEXT_RENDER_CAP, 120,
        "TEXT_RENDER_CAP pinned"
    );
    assert!(
        (cmhm_core::retrieval::TRUST_COMPROMISE_DELTA - 0.10).abs() < 1e-6,
        "TRUST_COMPROMISE_DELTA pinned"
    );
    assert_eq!(MAX_LEARN_EPISODES, 100_000, "ledger cap 100k pinned");
}

// ── EVICT rider: cap, marker, index consistency, no-persist, budget ─────

#[test]
fn b1b_evict_oldest_first_index_consistent() {
    assert_eq!(MAX_LEARN_EPISODES, 100_000, "rider: cap is exactly 100k");
    let mut ledger = LearnLedger::new();
    // Fill past the cap with distinct node ids (no sharing → index 1:1).
    let over = 5usize;
    for k in 0..(MAX_LEARN_EPISODES + over) {
        let b = 1_000_000u64 + k as u64;
        ledger.append(b, b + 400_000, b + 800_000, 1_700_000_000 + k as i64);
    }
    assert_eq!(ledger.len(), MAX_LEARN_EPISODES, "rider: ledger capped at 100k");
    assert_eq!(ledger.next_seq, MAX_LEARN_EPISODES as u64 + over as u64 + 1);
    // Oldest 5 seqs (1..=5) evicted; seq 6 is the oldest retained.
    assert_eq!(ledger.first_retained_seq(), Some(6));
    assert!(ledger.get(1).is_none(), "rider: oldest entry gone");
    assert!(ledger.get(5).is_none());
    assert!(ledger.get(6).is_some(), "rider: newest-retained boundary present");
    // Evicted seqs resolve to the evicted marker — DISTINCT from pre-B1b marker.
    assert_eq!(ledger.status_for_seq(1), EpisodeStatus::Evicted);
    assert_eq!(ledger.status_for_seq(5), EpisodeStatus::Evicted);
    assert_eq!(ledger.status_for_seq(6), EpisodeStatus::Found);
    assert_eq!(
        ledger.status_for_seq(ledger.next_seq + 1000),
        EpisodeStatus::NoTriple,
        "rider: never-existed seq is NoTriple, not Evicted"
    );
    // Evicted nodes are gone from the index; retained nodes resolve Found.
    let (st_old, _, _) = ledger.resolve_node(1_000_000);
    assert_eq!(st_old, EpisodeStatus::Evicted, "rider: evicted node → evicted-marker");
    let newest_base = 1_000_000u64 + (MAX_LEARN_EPISODES + over - 1) as u64;
    let (st_new, ep_new, sh_new) = ledger.resolve_node(newest_base);
    assert_eq!(st_new, EpisodeStatus::Found);
    assert_eq!(ep_new.unwrap().seq, ledger.next_seq - 1);
    assert_eq!(sh_new, 0);
    // Index consistency: every indexed seq exists; every retained episode indexed.
    ledger.assert_index_consistent();
    // Index is NEVER persisted: round-trip bytes carry episodes only.
    let bytes = bincode::serialize(&ledger).unwrap();
    let re: LearnLedger = bincode::deserialize(&bytes).unwrap();
    assert_eq!(re.len(), ledger.len(), "snapshot must preserve episodes");
    assert_eq!(re.next_seq, ledger.next_seq);
    // Freshly deserialized index is empty until rebuilt (serde skip).
    assert_eq!(re.index_len(), 0, "rider: index must NOT be persisted");
    let mut re = re;
    re.rebuild_index();
    assert_eq!(re.index_len(), ledger.index_len(), "rebuild must restore full index");
    re.assert_index_consistent();
    // Index memory is counted in the budget.
    let mem = ledger.memory_bytes();
    let episodes_only =
        ledger.len() * std::mem::size_of::<cmhm_core::learn_ledger::LearnEpisode>();
    assert!(
        mem > episodes_only,
        "rider: memory budget must include the index (mem={mem} episodes_only={episodes_only})"
    );
}

#[test]
fn b1b_t7_learned_date_is_readable_utc() {
    use cmhm_core::retrieval::fmt_learned_date;
    assert_eq!(fmt_learned_date(0), "1970-01-01");
    assert_eq!(fmt_learned_date(1791746816), "2026-10-11");
    // Pre-epoch degrades gracefully, never panics.
    assert_eq!(fmt_learned_date(-1).len(), 10);
}
