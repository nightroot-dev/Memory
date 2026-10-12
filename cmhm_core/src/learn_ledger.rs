//! B1b learn-event ledger (S-15, option A).
//!
//! Records every `process_learn` call as a minimal 5-field
//! [`LearnEpisode`] (`seq`, `state_id`, `action_id`, `outcome_id`, `timestamp`).
//! Text is NEVER duplicated here — retrieval resolves node text live from
//! `NodeStore` at query time, so the ledger stays small and cannot drift from
//! the store.
//!
//! Design constraints (owner riders):
//! - Cap [`MAX_LEARN_EPISODES`] (100k) entries, oldest-first eviction.
//! - Evicted entries resolve to [`EVICTED_MARKER`], DISTINCT from the pre-B1b
//!   [`NO_TRIPLE_MARKER`].
//! - Eviction removes the evicted seq from the node→episode index. The index is
//!   `#[serde(skip)]` — NEVER persisted — and is rebuilt via [`rebuild_index`]
//!   on snapshot load. [`memory_bytes`] counts the index toward the budget.
//! - Dedup-shared nodes: the index maps a node to EVERY episode that contains
//!   it; resolution returns the most-recent triple, with `shared_count = n-1`
//!   rendered as "(+N other episodes share this memory)".
//!
//! KNOWN LIMITATION (open S-05, do NOT fix here): WAL replay is not idempotent,
//! so a crash/restart replays already-snapshotted learns through `process_learn`
//! and appends DUPLICATE ledger entries exactly as it duplicates nodes. No
//! release ships between B1b and R1 by reviewer rule; R1 owns idempotency.
//!
//! R1 (S-05) FIXED at lifecycle: snapshot-side replay watermark
//! (`CoreState::last_applied_wal_seq`, snapshot v5) + startup skip of
//! already-applied WAL entries + replay skip-before-append in `process_learn`
//! (deduped replays don't append). Mid-life checkpoints AND the terminal
//! shutdown snapshot stamp the watermark and RETAIN the WAL (the WAL stays
//! the recovery path if the snapshot later proves corrupt/incompatible —
//! E7 quarantine rebuilds from it); the startup compaction point deletes the
//! WAL and resets the watermark to 0 after verified load+replay. Live learns
//! always append (T3 most-recent-wins preserved).

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

/// Maximum retained learn episodes. Oldest-first eviction past this cap.
pub const MAX_LEARN_EPISODES: usize = 100_000;

/// Marker for nodes that predate the ledger (learned before B1b, migrated v3).
pub const NO_TRIPLE_MARKER: &str = "no triple recorded (learned before B1b)";

/// Marker for entries evicted past the cap. DISTINCT from [`NO_TRIPLE_MARKER`].
pub const EVICTED_MARKER: &str = "episode record evicted";

/// Human marker for dedup-shared nodes: "(+N other episodes share this memory)".
pub fn shared_marker(n: usize) -> String {
    format!("(+{n} other episodes share this memory)")
}

/// Resolution outcome for an episode lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpisodeStatus {
    /// Episode retained in the ledger.
    Found,
    /// Sequence evicted past the cap (or a node whose last episode was evicted).
    Evicted,
    /// Never recorded: pre-B1b node, or an unknown sequence number.
    NoTriple,
}

/// Minimal learn-event record: IDs + timestamp only. NO text (rider 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnEpisode {
    /// Monotonic learn sequence, starting at 1. Never reused.
    pub seq: u64,
    pub state_id: u64,
    pub action_id: u64,
    pub outcome_id: u64,
    /// Unix seconds of the learn (same `now` stamped on the three nodes).
    pub timestamp: i64,
}

/// Resolved triple view: ledger IDs + text resolved live from `NodeStore`.
///
/// Per-endpoint text is `None` when the node is missing (culled/merged by
/// consolidation — graceful, never a panic), `Some("")` for vector-only learns
/// (triple exists, no text), and `Some(text)` otherwise.
#[derive(Debug, Clone)]
pub struct SeedEpisode {
    pub seq: u64,
    pub state_id: u64,
    pub action_id: u64,
    pub outcome_id: u64,
    pub timestamp: i64,
    pub state_text: Option<String>,
    pub action_text: Option<String>,
    pub outcome_text: Option<String>,
    /// Other episodes containing the seed node (`index_len - 1`).
    pub shared_count: usize,
}

/// B6b: caller-supplied typed metadata for one learn episode. Stored ONCE in
/// the ledger's sidecar table keyed by episode seq — never duplicated into
/// node text (texts stay verbatim). All fields optional at the API level;
/// an absent table entry means "unclassified", never a default kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnMeta {
    /// Recommended closed set: fact|failure|preference|config. The MCP
    /// boundary validates; core stores what it is given.
    pub kind: String,
    /// Free-form subject, e.g. "db_pool_size".
    pub subject: String,
    /// Free-form tags; capped at [`MAX_META_TAGS`] per episode.
    pub tags: Vec<String>,
}

/// B6b: per-episode tag cap (aligned with `extract_tags(..., 10)`).
pub const MAX_META_TAGS: usize = 10;

/// B6b: query-side filter over episode metadata. All present clauses must
/// match (AND); `kinds` matches any-of. An empty filter (no kinds, no
/// subject, no tags) is INACTIVE and changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MetaFilter {
    pub kinds: Vec<String>,
    pub subject: Option<String>,
    pub tags: Vec<String>,
}

impl MetaFilter {
    /// True when no clause is set — retrieval must behave exactly as unfiltered.
    pub fn is_inactive(&self) -> bool {
        self.kinds.is_empty() && self.subject.is_none() && self.tags.is_empty()
    }

    /// True when `meta` satisfies every present clause. Tag clause needs ALL
    /// listed tags present on the episode.
    pub fn matches(&self, meta: &LearnMeta) -> bool {
        if !self.kinds.is_empty() && !self.kinds.iter().any(|k| k == &meta.kind) {
            return false;
        }
        if let Some(s) = &self.subject {
            if s != &meta.subject {
                return false;
            }
        }
        if !self.tags.iter().all(|t| meta.tags.contains(t)) {
            return false;
        }
        true
    }
}
/// Learn-event ledger with a transient node→episode index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnLedger {
    /// Retained episodes, oldest-first. Contiguous seqs once eviction starts.
    episodes: VecDeque<LearnEpisode>,
    /// Next sequence to assign. Starts at 1.
    pub next_seq: u64,
    /// Highest seq evicted so far. 0 when nothing has been evicted.
    evicted_up_to: u64,
    /// Transient index: node id → seqs of retained episodes containing it,
    /// oldest-first. NEVER persisted ([`rebuild_index`] restores it on load).
    #[serde(skip)]
    node_to_episodes: HashMap<u64, Vec<u64>>,
    /// Transient set: nodes whose last retained episode was evicted.
    /// In-memory only (not persisted): after a snapshot reload these nodes
    /// resolve as [`EpisodeStatus::NoTriple`] rather than `Evicted`. Bounded by
    /// construction (entries re-added to the index drop out of this set), and
    /// counted in [`memory_bytes`].
    #[serde(skip)]
    evicted_nodes: HashSet<u64>,
    /// B6b: typed metadata by episode seq. PERSISTED (unlike the index above).
    /// Entries die with their episode: the append-time eviction loop removes
    /// the evicted seq's meta, so the table never outgrows the ledger.
    /// Absent entry = unclassified episode (pre-B6b learns, WAL-replayed tail
    /// learns whose meta record never arrived — documented, never backfilled).
    pub meta: HashMap<u64, LearnMeta>,
}

impl Default for LearnLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl LearnLedger {
    pub fn new() -> Self {
        Self {
            episodes: VecDeque::new(),
            next_seq: 1,
            evicted_up_to: 0,
            node_to_episodes: HashMap::new(),
            evicted_nodes: HashSet::new(),
            meta: HashMap::new(),
        }
    }

    /// B6b: attach typed metadata to a RETAINED episode. Returns false (and
    /// stores nothing) when the seq is unknown or already evicted — the
    /// caller surfaces that instead of failing silently. Tags are capped at
    /// [`MAX_META_TAGS`] (extras dropped from the tail, documented).
    pub fn record_meta(&mut self, seq: u64, mut meta: LearnMeta) -> bool {
        if self.get(seq).is_none() {
            return false;
        }
        meta.tags.truncate(MAX_META_TAGS);
        self.meta.insert(seq, meta);
        true
    }

    /// B6b: metadata for one episode, if classified.
    pub fn meta_for_seq(&self, seq: u64) -> Option<&LearnMeta> {
        self.meta.get(&seq)
    }

    /// Number of retained episodes.
    pub fn len(&self) -> usize {
        self.episodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.episodes.is_empty()
    }

    /// Oldest retained seq, or `None` when the ledger is empty.
    pub fn first_retained_seq(&self) -> Option<u64> {
        self.episodes.front().map(|e| e.seq)
    }

    /// Append one learn event. Single append site for `process_learn`
    /// (WAL replay funnels through the same site, so the ledger rebuilds
    /// without a new WAL byte).
    ///
    /// NOTE (S-05): replay duplicates entries exactly as it duplicates nodes.
    /// Documented, not worked around here.
    pub fn append(
        &mut self,
        state_id: u64,
        action_id: u64,
        outcome_id: u64,
        timestamp: i64,
    ) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.episodes.push_back(LearnEpisode {
            seq,
            state_id,
            action_id,
            outcome_id,
            timestamp,
        });
        for id in [state_id, action_id, outcome_id] {
            self.node_to_episodes.entry(id).or_default().push(seq);
            // A re-learned node is retained again — it is not evicted anymore.
            self.evicted_nodes.remove(&id);
        }
        // Oldest-first eviction past the cap.
        while self.episodes.len() > MAX_LEARN_EPISODES {
            if let Some(old) = self.episodes.pop_front() {
                self.evicted_up_to = self.evicted_up_to.max(old.seq);
                // B6b: metadata dies with its episode — the table can never
                // outgrow the ledger.
                self.meta.remove(&old.seq);
                for id in [old.state_id, old.action_id, old.outcome_id] {
                    if let Some(v) = self.node_to_episodes.get_mut(&id) {
                        v.retain(|s| *s != old.seq);
                        if v.is_empty() {
                            self.node_to_episodes.remove(&id);
                            self.evicted_nodes.insert(id);
                        }
                    }
                }
            } else {
                break;
            }
        }
        seq
    }

    /// Fetch a retained episode by seq. `None` for evicted/never-existed seqs.
    pub fn get(&self, seq: u64) -> Option<&LearnEpisode> {
        let first = self.first_retained_seq()?;
        if seq < first || seq >= self.next_seq {
            return None;
        }
        let idx = (seq - first) as usize;
        let ep = self.episodes.get(idx)?;
        debug_assert_eq!(ep.seq, seq, "ledger seq contiguity violated");
        Some(ep)
    }

    /// Status for a raw sequence number (evicted vs never-existed distinction).
    pub fn status_for_seq(&self, seq: u64) -> EpisodeStatus {
        if self.get(seq).is_some() {
            EpisodeStatus::Found
        } else if seq >= 1 && seq <= self.evicted_up_to {
            EpisodeStatus::Evicted
        } else {
            EpisodeStatus::NoTriple
        }
    }

    /// B7: do two nodes share at least one retained episode?
    ///
    /// `Some(true)` = learned together (state/action/outcome of one learn).
    /// `Some(false)` = both have retained episodes but share none — a chain
    /// step between them is a cross-experience link. `None` = unknown (either
    /// side has no retained episodes: pre-B1b, evicted, or never-recorded).
    /// Per S-19, unknown means VISIBLE — losing data silently is worse than
    /// showing a doubtful link.
    pub fn shares_episode(&self, a: u64, b: u64) -> Option<bool> {
        let a_seqs = self.node_to_episodes.get(&a)?;
        if a_seqs.is_empty() {
            return None;
        }
        let b_seqs = self.node_to_episodes.get(&b)?;
        if b_seqs.is_empty() {
            return None;
        }
        Some(a_seqs.iter().any(|s| b_seqs.contains(s)))
    }

    /// Resolve a node to its most-recent retained episode (most-recent wins).
    /// Returns `(status, episode, shared_count)` where `shared_count` is the
    /// number of OTHER retained episodes containing the node.
    pub fn resolve_node(&self, node_id: u64) -> (EpisodeStatus, Option<LearnEpisode>, usize) {
        match self.node_to_episodes.get(&node_id) {
            Some(seqs) if !seqs.is_empty() => {
                let latest = seqs[seqs.len() - 1];
                match self.get(latest) {
                    Some(ep) => (EpisodeStatus::Found, Some(ep.clone()), seqs.len() - 1),
                    // Defensive: index/ledger skew must never panic the join.
                    None => (EpisodeStatus::Evicted, None, 0),
                }
            }
            _ => {
                if self.evicted_nodes.contains(&node_id) {
                    (EpisodeStatus::Evicted, None, 0)
                } else {
                    (EpisodeStatus::NoTriple, None, 0)
                }
            }
        }
    }

    /// Resolve a node to a [`SeedEpisode`] view with text resolved live from
    /// `store`. Returns `None` when there is no retained episode (Evicted or
    /// NoTriple) — callers use [`resolve_node`] for the distinguishing status.
    /// Missing store nodes yield `None` endpoint text (consolidation-graceful).
    pub fn resolve_node_view(
        &self,
        node_id: u64,
        store: &crate::node_store::NodeStore,
    ) -> Option<SeedEpisode> {
        let (status, ep, shared) = self.resolve_node(node_id);
        if status != EpisodeStatus::Found {
            return None;
        }
        let ep = ep?;
        let text_of = |id: u64| store.get(id).map(|n| n.text.clone());
        Some(SeedEpisode {
            seq: ep.seq,
            state_id: ep.state_id,
            action_id: ep.action_id,
            outcome_id: ep.outcome_id,
            timestamp: ep.timestamp,
            state_text: text_of(ep.state_id),
            action_text: text_of(ep.action_id),
            outcome_text: text_of(ep.outcome_id),
            shared_count: shared,
        })
    }

    /// Rebuild the transient index from retained episodes. Called on snapshot
    /// load (the index is never persisted) and after migration.
    pub fn rebuild_index(&mut self) {
        self.node_to_episodes.clear();
        for ep in self.episodes.iter() {
            for id in [ep.state_id, ep.action_id, ep.outcome_id] {
                let v = self.node_to_episodes.entry(id).or_default();
                if !v.contains(&ep.seq) {
                    v.push(ep.seq);
                }
            }
        }
    }

    /// Number of indexed node keys (for tests + budget accounting).
    pub fn index_len(&self) -> usize {
        self.node_to_episodes.len()
    }

    /// Assert index/ledger consistency. Panics with a diagnostic on skew
    /// (test + debug aid; the retrieval join itself never panics).
    pub fn assert_index_consistent(&self) {
        for (node_id, seqs) in self.node_to_episodes.iter() {
            for seq in seqs {
                assert!(
                    self.get(*seq).is_some(),
                    "ledger index skew: node {node_id} references missing seq {seq}"
                );
            }
        }
        for ep in self.episodes.iter() {
            for id in [ep.state_id, ep.action_id, ep.outcome_id] {
                let seqs = self.node_to_episodes.get(&id).unwrap_or_else(|| {
                    panic!("ledger index skew: episode seq {} node {id} not indexed", ep.seq)
                });
                assert!(
                    seqs.contains(&ep.seq),
                    "ledger index skew: episode seq {} missing from node {id} list",
                    ep.seq
                );
            }
        }
    }

    /// Estimated memory footprint in bytes, INCLUDING the transient index and
    /// evicted set (rider: index memory is counted in the budget).
    ///
    /// Estimate (64-bit): episode = 40 B (5×8 B fields); per indexed node key
    /// ≈ 8 (id) + 24 (Vec overhead) + 8×n_seqs + 16 (HashMap entry); per
    /// evicted node ≈ 24 B; per meta entry ≈ 64 B + string bytes. Backing-store
    /// capacity beyond `len` is not counted (under-estimate favours the budget).
    pub fn memory_bytes(&self) -> usize {
        let episodes = self.episodes.len() * std::mem::size_of::<LearnEpisode>();
        let index: usize = self
            .node_to_episodes
            .iter()
            .map(|(_, v)| 8 + 24 + v.len() * 8 + 16)
            .sum();
        let evicted = self.evicted_nodes.len() * 24;
        let meta: usize = self
            .meta
            .iter()
            .map(|(_, m)| {
                64 + m.kind.len() + m.subject.len() + m.tags.iter().map(|t| t.len()).sum::<usize>()
            })
            .sum();
        episodes + index + evicted + meta + 64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_are_distinct() {
        assert_ne!(NO_TRIPLE_MARKER, EVICTED_MARKER);
    }

    #[test]
    fn append_assigns_monotonic_seqs() {
        let mut l = LearnLedger::new();
        assert_eq!(l.append(1, 2, 3, 100), 1);
        assert_eq!(l.append(4, 5, 6, 101), 2);
        assert_eq!(l.len(), 2);
        assert_eq!(l.next_seq, 3);
    }

    #[test]
    fn shared_count_tracks_dedup() {
        let mut l = LearnLedger::new();
        l.append(7, 8, 9, 100);
        l.append(7, 10, 11, 101);
        let (st, ep, shared) = l.resolve_node(7);
        assert_eq!(st, EpisodeStatus::Found);
        assert_eq!(ep.unwrap().seq, 2);
        assert_eq!(shared, 1);
    }

    fn meta(kind: &str, subject: &str, tags: &[&str]) -> LearnMeta {
        LearnMeta {
            kind: kind.to_string(),
            subject: subject.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn meta_attaches_only_to_retained_episodes() {
        let mut l = LearnLedger::new();
        l.append(1, 2, 3, 100);
        assert!(l.record_meta(1, meta("failure", "db", &["pool"])));
        assert_eq!(l.meta_for_seq(1).unwrap().kind, "failure");
        assert!(!l.record_meta(999, meta("fact", "x", &[])), "unknown seq refuses");
        assert!(l.meta_for_seq(999).is_none());
    }

    #[test]
    fn meta_tags_truncate_at_cap() {
        let mut l = LearnLedger::new();
        l.append(1, 2, 3, 100);
        let tags: Vec<String> = (0..30).map(|i| format!("t{i}")).collect();
        let refs: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
        assert!(l.record_meta(1, meta("fact", "s", &refs)));
        assert_eq!(l.meta_for_seq(1).unwrap().tags.len(), MAX_META_TAGS);
    }

    #[test]
    fn meta_dies_with_evicted_episode() {
        let mut l = LearnLedger::new();
        for k in 0..(MAX_LEARN_EPISODES + 1) {
            let b = 1_000_000u64 + k as u64;
            l.append(b, b + 400_000, b + 800_000, 100);
        }
        // seq 1 evicted with its episode; newest retained keeps meta.
        assert!(!l.record_meta(1, meta("fact", "old", &[])), "evicted seq refuses");
        let newest = l.next_seq - 1;
        assert!(l.record_meta(newest, meta("fact", "new", &[])));
        assert!(l.meta_for_seq(1).is_none(), "evicted seq has no meta");
        assert!(l.meta_for_seq(newest).is_some(), "retained seq keeps its meta");
        // Budget counts the table.
        assert!(l.memory_bytes() > (l.len() * std::mem::size_of::<LearnEpisode>()));
    }

    #[test]
    fn meta_filter_matches_clauses() {
        let m = meta("failure", "db_pool", &["pool", "timeout"]);
        let any = MetaFilter::default();
        assert!(any.is_inactive());
        assert!(any.matches(&m), "inactive filter matches everything");
        assert!(MetaFilter { kinds: vec!["failure".into()], ..Default::default() }.matches(&m));
        assert!(!MetaFilter { kinds: vec!["fact".into()], ..Default::default() }.matches(&m));
        assert!(MetaFilter { kinds: vec!["fact".into(), "failure".into()], ..Default::default() }.matches(&m));
        assert!(MetaFilter { subject: Some("db_pool".into()), ..Default::default() }.matches(&m));
        assert!(!MetaFilter { subject: Some("other".into()), ..Default::default() }.matches(&m));
        assert!(MetaFilter { tags: vec!["pool".into(), "timeout".into()], ..Default::default() }.matches(&m));
        assert!(!MetaFilter { tags: vec!["pool".into(), "missing".into()], ..Default::default() }.matches(&m));
    }
}