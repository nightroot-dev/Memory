use std::sync::mpsc;
use std::path::PathBuf;
use anyhow::Result;
use rand::rngs::SmallRng;
use rand::SeedableRng;

use crate::{
    EMBED_DIM,
    config::Config,
    core_state::{CoreState, now_secs},
    node::{Node, NodeType},
    edge::{Edge, EdgeType, EvidenceType},
    dense_core::DenseCore,
    consolidation::run_consolidation,
    persistence::{WalWriter, read_wal, write_snapshot, read_snapshot, SnapshotLoadError},
    retrieval::{self, RetrievalResult},
};

// ─── Public Command API ────────────────────────────────────────────────────

/// A command sent to the CMHM worker thread.
pub enum Command {
    Retrieve {
        query_emb: Vec<f32>,
        model_id:  String,
        /// Role-scoped seed selection passed through to retrieval::retrieve.
        /// Some(State) restricts seed candidates to State nodes.
        /// None = backward-compat unscoped scan.
         seed_type: Option<NodeType>,
         /// Raw query text for BM25 lexical candidates. Empty = dense-only (legacy/tests).
         query_text: String,
         tx: mpsc::SyncSender<Result<RetrievalResult>>,
    },
    Learn {
        state_emb: Vec<f32>,
        action_emb: Vec<f32>,
        outcome_emb: Vec<f32>,
        prediction_error: f32,
        emotional_tag: f32,
        model_id: String,
        /// Machine-readable source-trust level ∈ [0, 1]. Stored in Node::authority.
        authority: f32,
        /// Caller-supplied source class (e.g. "user_explicit", "corrected", "").
        provenance: String,
        /// Optional ID of an existing node this learn supersedes.
        supersedes: Option<u64>,
        /// Authenticated caller role (User, System, Agent).
        caller_role: crate::authority::CallerRole,
        /// Optional semantic assertion / entity key.
        assertion_key: Option<String>,
        /// Optional epistemic polarity (+1 or -1).
         polarity: Option<i8>,
         /// Optional scrubbed raw text (state/action/outcome) for BM25 indexing.
         raw_text: Option<(String, String, String)>,
        tx: mpsc::SyncSender<Result<LearnResult>>,
    },
    ExplicitEdge {
        state_id: u64,
        action_id: u64,
        outcome_id: u64,
        success: bool,
        tx: mpsc::SyncSender<Result<()>>,
    },
    /// Outcome feedback for a set of retrieved node IDs.
    /// Updates each node's Beta bandit posterior and half-life.
    /// `feedback_key`: base key for deduplication — keyed per (base_key, node_id, success).
    /// When present, the engine skips nodes whose key is already in `feedback_seen`.
    /// When None (unjoined feedback): applied unconditionally with no dedup guarantee.
    FeedbackNodes {
        node_ids: Vec<u64>,
        success: bool,
        feedback_key: Option<String>,
        tx: mpsc::SyncSender<Result<usize>>, // returns number of nodes actually updated
    },
    /// Read-only: max cosine similarity of query against all nodes with model_id.
    /// `node_type`: when Some, restricts search to nodes of that type (role-scoped gate).
    ///              when None, searches all nodes (backward-compat path).
    /// Used by shadow gate (no WAL write, no state mutation).
    MaxCosine {
        query_emb: Vec<f32>,
        model_id:  String,
        node_type: Option<NodeType>,
        tx: mpsc::SyncSender<f32>,
    },
    /// Read-only: number of nodes currently in the store. Used by tests.
    NodeCount {
        tx: mpsc::SyncSender<usize>,
    },
    /// Read-only: Beta posterior (alpha, beta) for a specific node ID.
    /// Returns None if the node ID is not present. Used by tests.
    GetNodeBeta {
        node_id: u64,
        tx: mpsc::SyncSender<Option<(f32, f32)>>,
    },
    /// Read the current adaptive quantile gate value (birth_q30).
    GetBirthQ30 {
        tx: mpsc::SyncSender<f32>,
    },
    /// Inspect neighborhood evidence for ingestion gating: max cosine, nearest node bandit, cohort distance.
    InspectGate {
        query_emb: Vec<f32>,
        model_id: String,
        node_type: NodeType,
        tx: mpsc::SyncSender<GateInspectionResult>,
    },
    Snapshot {
        path: PathBuf,
        tx: mpsc::SyncSender<Result<()>>,
    },
    /// Register a projection matrix for a model_id at runtime.
    /// `matrix` is row-major, shape [EMBED_DIM x input_dim].
    /// `bias` is [f32; EMBED_DIM].
    RegisterProjection {
        model_id: String,
        input_dim: usize,
        matrix: Vec<f32>,
        bias: [f32; crate::EMBED_DIM],
        tx: mpsc::SyncSender<Result<()>>,
    },
    Shutdown {
        tx: mpsc::SyncSender<Result<()>>,
    },
}

/// Explicit, structured error returned when a supersession invariant is violated.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SupersessionError {
    #[error("Supersession target node {0} does not exist in store")]
    TargetNotFound(u64),
    #[error("Cannot supersede node {0} of type {1:?}: only State nodes can be superseded by State nodes")]
    CrossTypeMismatch(u64, NodeType),
    #[error("Node {0} cannot supersede itself (self-loop)")]
    SelfLoop(u64),
    #[error("Node {0} has already been superseded by node {1}")]
    AlreadySuperseded(u64, u64),
}

/// Serialisable record written to WAL for every Learn command.
/// Must contain ALL fields needed to deterministically replay the operation.
/// This is the legacy v1 record (WAL type byte `0x00`). New writes use `WalLearnEntryV2`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct WalLearnEntry {
    state_emb:        Vec<f32>,
    action_emb:       Vec<f32>,
    outcome_emb:      Vec<f32>,
    prediction_error: f32,
    emotional_tag:    f32,
    model_id:         String,
}

/// Serialisable record written to WAL for every Learn command (schema v3+).
///
/// WAL type byte: `0x02`
///
/// Adds `authority`, `provenance`, and `supersedes` from `Command::Learn`.
/// Replays through `process_learn` with the full authority/provenance/supersession semantics.
/// Old `WalLearnEntry` (type `0x00`) records replay with defaults:
/// `authority=0.5, provenance="", supersedes=None`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct WalLearnEntryV2 {
    state_emb:        Vec<f32>,
    action_emb:       Vec<f32>,
    outcome_emb:      Vec<f32>,
    prediction_error: f32,
    emotional_tag:    f32,
    model_id:         String,
    authority:        f32,
    provenance:       String,
    supersedes:       Option<u64>,
    #[serde(default)]
    assertion_key:    Option<String>,
    #[serde(default)]
    polarity:         Option<i8>,
}

/// Serialisable record for text-path learns (WAL type byte `0x04`).
///
/// Same as `WalLearnEntryV2` plus scrubbed `raw_text` for BM25 indexing.
/// Vector-path learns keep writing `WalLearnEntryV2` / `0x03` (bytes unchanged).
/// Old `0x00`–`0x03` records replay with `raw_text = None` (dense-only, as before).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct WalLearnEntryV3 {
    state_emb:        Vec<f32>,
    action_emb:       Vec<f32>,
    outcome_emb:      Vec<f32>,
    prediction_error: f32,
    emotional_tag:    f32,
    model_id:         String,
    authority:        f32,
    provenance:       String,
    supersedes:       Option<u64>,
    #[serde(default)]
    assertion_key:    Option<String>,
    #[serde(default)]
    polarity:         Option<i8>,
    #[serde(default)]
    raw_text:         Option<(String, String, String)>,
}


/// Serialisable record written to WAL for every applied feedback node.
///
/// One record per (retrieve_id, node_id, outcome) triple that was **actually applied**
/// (i.e. not already in `feedback_seen`). Per-node granularity keeps replay idempotent:
/// each record is self-contained and re-checked against `feedback_seen` on replay.
///
/// WAL type byte: `0x01`
///
/// # TODO
/// FNV-1a is used for `feedback_seen` key hashing. It has weak avalanche on structured
/// suffixes (`/{node_id}/{outcome}`). Upgrade to xxh3 (xxhash-rust, no_std, tiny) once
/// that crate is added to the workspace. A collision here silently drops a posterior update.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct WalFeedbackEntry {
    retrieve_id: String,
    node_id:     u64,
    outcome:     bool,
    ts_secs:     f64,
}

/// WAL record type tags.
/// New WAL payloads are prefixed with one of these bytes.
/// Legacy WAL payloads (pre-typed format) have no prefix byte;
/// their first byte will be the low byte of `state_emb.len()` (a u64 LE).
/// For 128-dim embeddings that byte is `0x80` — safely outside [0x00, 0x01],
/// so the `_` (legacy) arm of the replay dispatch fires correctly.
const WAL_TYPE_LEARN:    u8 = 0x00;
const WAL_TYPE_FEEDBACK: u8 = 0x01;
/// Legacy v3 (early): unauthenticated authority + provenance fields.
/// Replayed conservatively with CallerRole::Agent to prevent privilege escalation from pre-fix records.
const WAL_TYPE_LEARN_V2: u8 = 0x02;
/// v3-hardened: verified effective authority + provenance fields.
/// Replayed with CallerRole::System since values were normalized and clamped at write time.
const WAL_TYPE_LEARN_V3: u8 = 0x03;
/// v4: text-path learn — V3 fields plus scrubbed raw_text for BM25 indexing.
/// Replayed with CallerRole::System (same trust basis as V3).
const WAL_TYPE_LEARN_V4: u8 = 0x04;

/// Result of a Learn command.
#[derive(Debug, Clone)]
pub struct LearnResult {
    pub state_node_id: u64,
    pub action_node_id: u64,
    pub outcome_node_id: u64,
    /// Effective authority after role-based clamping — this is what was durably stored.
    pub effective_authority: f32,
    /// Effective provenance after role-based normalization — this is what was durably stored.
    pub effective_provenance: String,
}

/// Result of an InspectGate query for ingestion gating.
#[derive(Debug, Clone, PartialEq)]
pub struct GateInspectionResult {
    /// Maximum cosine similarity to any existing node of the requested NodeType.
    pub max_cosine: f32,
    /// ID of the nearest node (if any).
    pub nearest_node_id: Option<u64>,
    /// Bandit Beta posterior (alpha, beta) of the nearest node.
    pub nearest_bandit: Option<(f32, f32)>,
    /// Distance from the query embedding to its assigned cohort centroid.
    pub cohort_dist: Option<f32>,
}

// ─── Engine Handle ─────────────────────────────────────────────────────────

/// Client handle to the CMHM engine.
/// Cloneable: multiple producers can send commands.
#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::SyncSender<Command>,
}

impl EngineHandle {
    /// Retrieve the causal chain most relevant to `query_emb`.
    ///
    /// `seed_type`: restricts seed candidate selection to nodes of that role.
    /// This is the scoped path — candidate pool is limited to one NodeType.
     pub fn retrieve(&self, query_emb: Vec<f32>, model_id: impl Into<String>, seed_type: NodeType, query_text: impl Into<String>) -> Result<RetrievalResult> {
         let (resp_tx, resp_rx) = mpsc::sync_channel(1);
         self.tx.send(Command::Retrieve {
             query_emb,
             model_id: model_id.into(),
             seed_type: Some(seed_type),
             query_text: query_text.into(),
             tx: resp_tx,
        })?;
        resp_rx.recv()?
    }

    /// Learn with scrubbed raw text for BM25 lexical indexing.
    ///
    /// Identical to [`learn_with_context`] except it carries
    /// `raw_text = Some((state, action, outcome))` through to `birth_node`,
    /// which stores `Node.text` and inserts into the BM25 index.
    /// Vector-path callers keep using `learn_with_context` (raw_text = None).
    pub fn learn_with_text_context(
        &self,
        state_emb: Vec<f32>,
        action_emb: Vec<f32>,
        outcome_emb: Vec<f32>,
        prediction_error: f32,
        emotional_tag: f32,
        model_id: impl Into<String>,
        authority: f32,
        provenance: String,
        supersedes: Option<u64>,
        caller_role: crate::authority::CallerRole,
        assertion_key: Option<String>,
        polarity: Option<i8>,
        raw_text: Option<(String, String, String)>,
    ) -> Result<LearnResult> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        self.tx.send(Command::Learn {
            state_emb, action_emb, outcome_emb,
            prediction_error, emotional_tag,
            model_id: model_id.into(),
             authority, provenance, supersedes,
             caller_role, assertion_key, polarity,
             raw_text,
             tx: resp_tx,
        })?;
        resp_rx.recv()?
    }

    /// Unscoped retrieval — candidate pool includes all NodeType variants.
    ///
    /// Use when the caller cannot assert node type intent, or when cross-type
    /// contradiction safety must be verified (e.g., Phase 3 approval benchmark).
     pub fn retrieve_unscoped(&self, query_emb: Vec<f32>, model_id: impl Into<String>, query_text: impl Into<String>) -> Result<RetrievalResult> {
         let (resp_tx, resp_rx) = mpsc::sync_channel(1);
         self.tx.send(Command::Retrieve {
             query_emb,
             model_id: model_id.into(),
             seed_type: None,
             query_text: query_text.into(),
             tx: resp_tx,
         })?;
         resp_rx.recv()?
     }


    /// Feed a (state, action, outcome) learning signal to the engine with default trusted user context.
    pub fn learn(
        &self,
        state_emb: Vec<f32>,
        action_emb: Vec<f32>,
        outcome_emb: Vec<f32>,
        prediction_error: f32,
        emotional_tag: f32,
        model_id: impl Into<String>,
        authority: f32,
        provenance: String,
        supersedes: Option<u64>,
    ) -> Result<LearnResult> {
        self.learn_with_context(
            state_emb, action_emb, outcome_emb,
            prediction_error, emotional_tag, model_id,
            authority, provenance, supersedes,
            crate::authority::CallerRole::User,
            None,
            None,
        )
    }

    /// Feed a learning signal with caller ingestion role and optional semantic assertion fields.
    pub fn learn_with_context(
        &self,
        state_emb: Vec<f32>,
        action_emb: Vec<f32>,
        outcome_emb: Vec<f32>,
        prediction_error: f32,
        emotional_tag: f32,
        model_id: impl Into<String>,
        authority: f32,
        provenance: String,
        supersedes: Option<u64>,
        caller_role: crate::authority::CallerRole,
        assertion_key: Option<String>,
        polarity: Option<i8>,
    ) -> Result<LearnResult> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        self.tx.send(Command::Learn {
            state_emb, action_emb, outcome_emb,
            prediction_error, emotional_tag,
            model_id: model_id.into(),
             authority, provenance, supersedes,
             caller_role, assertion_key, polarity,
             raw_text: None,
             tx: resp_tx,
        })?;
        resp_rx.recv()?
    }


    /// Create explicit edges between known node IDs.
    pub fn explicit_edge(&self, state_id: u64, action_id: u64, outcome_id: u64, success: bool) -> Result<()> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        self.tx.send(Command::ExplicitEdge { state_id, action_id, outcome_id, success, tx: resp_tx })?;
        resp_rx.recv()?
    }

    /// Force a snapshot to the given path.
    pub fn snapshot(&self, path: PathBuf) -> Result<()> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        self.tx.send(Command::Snapshot { path, tx: resp_tx })?;
        resp_rx.recv()?
    }

    /// Provide outcome feedback for a set of retrieved node IDs.
    ///
    /// `feedback_key`: base key for deduplication (typically the retrieve_id).
    /// When provided, the engine skips any node whose `"{key}/{node_id}/{success}"` is already
    /// in `CoreState::feedback_seen`. This makes duplicate MCP calls and WAL replays no-ops.
    /// Returns the number of nodes that were actually updated (0 if all duplicates).
    pub fn feedback_nodes(&self, node_ids: Vec<u64>, success: bool, feedback_key: Option<String>) -> Result<usize> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        self.tx.send(Command::FeedbackNodes { node_ids, success, feedback_key, tx: resp_tx })?;
        resp_rx.recv()?
    }

    /// Maximum cosine similarity of `query_emb` against existing nodes with `model_id`.
    /// `node_type`: `Some(NodeType::State)` for the shadow gate (role-scoped);
    ///              `None` for unscoped all-roles comparison.
    /// Returns 0.0 on empty store or engine shutdown. Read-only — no WAL write.
    pub fn max_cosine_to_existing(
        &self,
        query_emb: &[f32],
        model_id: &str,
        node_type: Option<NodeType>,
    ) -> f32 {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        if self.tx.send(Command::MaxCosine {
            query_emb: query_emb.to_vec(),
            model_id: model_id.to_string(),
            node_type,
            tx: resp_tx,
        }).is_err() {
            return 0.0; // engine shut down
        }
        resp_rx.recv().unwrap_or(0.0)
    }

    /// Read-only gate inspection: computes max cosine, nearest node metadata (bandit priors),
    /// and cohort centroid distance without mutating graph state or writing to WAL.
    pub fn inspect_gate(
        &self,
        query_emb: &[f32],
        model_id: &str,
        node_type: NodeType,
    ) -> GateInspectionResult {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        if self.tx.send(Command::InspectGate {
            query_emb: query_emb.to_vec(),
            model_id: model_id.to_string(),
            node_type,
            tx: resp_tx,
        }).is_err() {
            return GateInspectionResult {
                max_cosine: 0.0,
                nearest_node_id: None,
                nearest_bandit: None,
                cohort_dist: None,
            };
        }
        resp_rx.recv().unwrap_or(GateInspectionResult {
            max_cosine: 0.0,
            nearest_node_id: None,
            nearest_bandit: None,
            cohort_dist: None,
        })
    }

    /// Number of nodes currently in the node store. Read-only — no WAL write.
    /// Returns 0 if the engine has shut down.
    pub fn node_count(&self) -> usize {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        if self.tx.send(Command::NodeCount { tx: resp_tx }).is_err() {
            return 0;
        }
        resp_rx.recv().unwrap_or(0)
    }

    /// Beta posterior `(bandit_alpha, bandit_beta)` for a specific node ID.
    /// Returns `None` if the node is not present. Read-only — no WAL write.
    /// Used by tests to verify Beta parameter correctness after feedback.
    pub fn get_node_beta(&self, node_id: u64) -> Option<(f32, f32)> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        if self.tx.send(Command::GetNodeBeta { node_id, tx: resp_tx }).is_err() {
            return None;
        }
        resp_rx.recv().unwrap_or(None)
    }

    /// Current value of the adaptive P₀.₃₀ birth gate quantile.
    /// Returns 0.30 (the prior) if the channel is closed.
    /// Read-only — no WAL write. Used by tests to verify quantile convergence.
    pub fn get_birth_q30(&self) -> f32 {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        if self.tx.send(Command::GetBirthQ30 { tx: resp_tx }).is_err() {
            return 0.30;
        }
        resp_rx.recv().unwrap_or(0.30)
    }

    /// Register a projection matrix at runtime.
    ///
    /// Matrix is row-major with shape [EMBED_DIM x input_dim].
    /// Bias is [f32; EMBED_DIM]. After successful registration, learn
    /// and retrieve calls with this model_id will project native-dim
    /// embeddings into the internal EMBED_DIM space.
    ///
    /// Returns Err if the engine worker has shut down, or if matrix
    /// length does not equal EMBED_DIM * input_dim.
    pub fn register_projection(
        &self,
        model_id: impl Into<String>,
        input_dim: usize,
        matrix: Vec<f32>,
        bias: [f32; crate::EMBED_DIM],
    ) -> Result<()> {
        if matrix.len() != crate::EMBED_DIM * input_dim {
            anyhow::bail!(
                "matrix shape mismatch: expected {} f32 ({} x {}), got {}",
                crate::EMBED_DIM * input_dim,
                crate::EMBED_DIM,
                input_dim,
                matrix.len()
            );
        }
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        self.tx.send(Command::RegisterProjection {
            model_id: model_id.into(),
            input_dim,
            matrix,
            bias,
            tx: resp_tx,
        })?;
        resp_rx.recv()?
    }

    /// Shut down the engine gracefully (takes a final snapshot and waits for disk flush).
    pub fn shutdown(self) -> Result<()> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        if self.tx.send(Command::Shutdown { tx: resp_tx }).is_err() {
            return Ok(()); // Worker thread already stopped
        }
        resp_rx.recv().unwrap_or(Ok(()))
    }
}

// ─── Engine Startup ───────────────────────────────────────────────────

/// Returned by [`start`] when a snapshot schema version mismatch is detected
/// and the engine is configured to fail closed
/// (`allow_incompatible_snapshot_reset = false`, the default).
///
/// ## Why a typed error?
/// A schema mismatch is fundamentally different from a WAL replay failure:
/// the data on disk is **intact** — nothing was written, nothing was lost.
/// Callers (specifically `ContextRuntime::ensure_loaded`) inspect this type
/// to decide whether to preserve the context identity (`Sleeping`) rather than
/// mark it `Deleted`.
#[derive(Debug)]
pub struct SchemaMismatchError {
    /// Schema version found in the snapshot file.
    pub found: u32,
    /// Schema version currently expected by this build.
    pub expected: u32,
    /// Path of the incompatible snapshot that was preserved.
    pub snapshot_path: std::path::PathBuf,
}

impl std::fmt::Display for SchemaMismatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "snapshot schema mismatch (found=v{}, expected=v{}, path={:?}). \
             Snapshot is untouched. \
             Set allow_incompatible_snapshot_reset=true to quarantine and reset, \
             or provide a migration path.",
            self.found, self.expected, self.snapshot_path
        )
    }
}

impl std::error::Error for SchemaMismatchError {}

/// Attempt to migrate a v1 snapshot (no `SnapshotEnvelope` wrapper) into the
/// current `CoreState` layout.
///
/// v1 snapshots are raw `bincode(CoreState)` bytes written before the envelope
/// was introduced. Migration succeeds only if the `CoreState` struct layout is
/// backward-compatible with the bytes on disk. If new fields were added (e.g.
/// `birth_q30` in v2), bincode will return an error because the byte offsets
/// shifted.
///
/// The caller decides what to do on failure: quarantine + opt-in data loss, or
/// fail closed (return `Err` from `start()`).
fn try_migrate_v1_snapshot(snapshot_path: &std::path::Path) -> anyhow::Result<CoreState> {
    let bytes = std::fs::read(snapshot_path)?;
    let state = bincode::deserialize::<CoreState>(&bytes)
        .map_err(|e| anyhow::anyhow!("v1 layout deserialization failed: {e}"))?;
    Ok(state)
}

/// Launch the CMHM engine on a background thread.
/// Returns an `EngineHandle` for the caller to interact with.
///
/// ## Startup sequence
/// 1. Load versioned snapshot (if present)
/// 2. Replay WAL on top of snapshot — crash recovery for entries since last compaction
/// 3. Write a post-replay snapshot and delete the WAL (compaction point)
/// 4. Open a fresh WAL for the new session
/// 5. Start the worker thread
///
/// ## Recovery boundary
/// The WAL is **compacted at each clean startup** (Step 3). After compaction, the WAL
/// only contains entries from the current session. This means:
///
/// - **Crash recovery**: Full recovery of any Learn or Feedback entries written since
///   the last successful startup (the WAL captured everything from that session).
/// - **Schema version mismatch**: `start()` returns `Err` by default — the incompatible
///   snapshot is left untouched so it can be inspected or restored. Set
///   `Config::allow_incompatible_snapshot_reset = true` to explicitly opt into data loss.
pub fn start(cfg: Config) -> Result<EngineHandle> {
    // Ensure data dirs exist
    std::fs::create_dir_all(&cfg.data_dir)?;
    std::fs::create_dir_all(&cfg.projections_dir)?;

    let snapshot_path = cfg.data_dir.join("snapshot.bin");
    let wal_path      = cfg.data_dir.join("wal.dat");

    // ── Step 1: Load snapshot ─────────────────────────────────────────────
    let mut state: CoreState = match read_snapshot::<CoreState>(&snapshot_path) {
        Ok(s) => {
            tracing::info!("Snapshot loaded: {} nodes", s.node_store.len());
            s
        }
        Err(SnapshotLoadError::VersionMismatch { found, expected, payload }) => {
            // Attempt deterministic v2→v3 migration
            if found == 2 {
                match crate::migration::migrate_v2_to_v3(&payload) {
                    Ok(migrated) => {
                        tracing::info!(
                            "Snapshot v2→v3 migration succeeded: {} nodes recovered with neutral defaults.",
                            migrated.node_store.len()
                        );
                        migrated
                    }
                    Err(migration_err) => {
                        tracing::error!(
                            "Snapshot schema mismatch (found=v{found}, expected=v{expected}). \
                             Migration attempt failed: {migration_err}."
                        );

                        if cfg.allow_incompatible_snapshot_reset {
                            let quarantine_path = snapshot_path.with_extension(
                                format!("v{found}.incompatible")
                            );
                            let _ = std::fs::rename(&snapshot_path, &quarantine_path);
                            for n in 1u32..=2 {
                                let gen_path = snapshot_path.with_extension(format!("bin.{n}"));
                                if gen_path.exists() {
                                    let q = snapshot_path.with_extension(
                                        format!("v{found}.incompatible.{n}")
                                    );
                                    let _ = std::fs::rename(&gen_path, &q);
                                }
                            }
                            CoreState::new(cfg.working_memory_size)
                        } else {
                            return Err(anyhow::Error::new(SchemaMismatchError {
                                found,
                                expected,
                                snapshot_path: snapshot_path.clone(),
                            }));
                        }
                    }
                }
            } else {
                tracing::error!(
                    "Snapshot schema mismatch (found=v{found}, expected=v{expected})."
                );

                if cfg.allow_incompatible_snapshot_reset {
                    let quarantine_path = snapshot_path.with_extension(
                        format!("v{found}.incompatible")
                    );
                    let _ = std::fs::rename(&snapshot_path, &quarantine_path);
                    for n in 1u32..=2 {
                        let gen_path = snapshot_path.with_extension(format!("bin.{n}"));
                        if gen_path.exists() {
                            let q = snapshot_path.with_extension(
                                format!("v{found}.incompatible.{n}")
                            );
                            let _ = std::fs::rename(&gen_path, &q);
                        }
                    }
                    CoreState::new(cfg.working_memory_size)
                } else {
                    return Err(anyhow::Error::new(SchemaMismatchError {
                        found,
                        expected,
                        snapshot_path: snapshot_path.clone(),
                    }));
                }
            }
        }
        Err(SnapshotLoadError::NotFound) => {
            tracing::info!("No snapshot found at {:?} — starting with empty state", snapshot_path);
            CoreState::new(cfg.working_memory_size)
        }
        Err(SnapshotLoadError::Corrupt(e)) => {
            tracing::error!(
                "Snapshot corrupt ({e}); all generations failed. \
                 Starting fresh and replaying WAL. Some data may be lost."
            );
            CoreState::new(cfg.working_memory_size)
        }
    };


    // ── Step 1b: Load projection matrices from disk ──────────────
    // Projections live outside snapshots (large, derived data).
    // Load after snapshot load and before WAL replay so that WAL
    // replay of prior Learn commands can resolve model_ids correctly.
    let projections_loaded = state.projections.load_from_dir(&cfg.projections_dir);
    if projections_loaded > 0 {
        tracing::info!(
            count = projections_loaded,
            dir = %cfg.projections_dir.display(),
            "loaded projection matrices"
        );
    }

    // ── Step 1c: Migrate native-dim nodes to projected 128-dim ──
    // Prior builds stored native MiniLM vectors (384-dim) directly.
    // Retrieval compares 128-dim queries; those nodes were invisible.
    let mut projected_count = 0usize;
    let mut quarantined_count = 0usize;
    let node_ids: Vec<u64> = state.node_store.all_ids();
    for id in node_ids {
        let native = match state.node_store.get(id) {
            Some(n) if n.embedding.len() != crate::EMBED_DIM => n.embedding.clone(),
            _ => continue,
        };
        let model_id = state
            .node_store
            .get(id)
            .map(|n| n.model_id.clone())
            .unwrap_or_default();
        match state.projections.project(&model_id, &native) {
            Ok(p) => {
                if let Some(n) = state.node_store.get_mut(id) {
                    n.embedding = p.to_vec();
                }
                projected_count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    "migration: node {} model_id={} dim={} — no projection: {e}; \
                     retained but not dense-retrievable",
                    id,
                    model_id,
                    native.len()
                );
                quarantined_count += 1;
            }
        }
    }
    if projected_count > 0 || quarantined_count > 0 {
        tracing::info!(
            projected = projected_count,
            quarantined = quarantined_count,
            "native-dim node migration complete"
        );
    }

    // ── Step 2: WAL replay ────────────────────────────────────────────────
    let mut replayed: usize = 0;
    let mut replay_failed = false;
    if wal_path.exists() {
        match read_wal(&wal_path) {
            Ok(entries) if entries.is_empty() => {
                tracing::info!("WAL exists but is empty — no replay needed");
            }
            Ok(entries) => {
                tracing::info!("Replaying {} WAL entries from {:?}", entries.len(), wal_path);
                for entry in &entries {
                    let payload = &entry.payload;
                    if payload.is_empty() {
                        tracing::debug!("WAL: empty payload, skipping");
                        continue;
                    }
                    match payload[0] {
                        WAL_TYPE_LEARN_V4 => {
                            // v4 text-path Learn: V3 fields plus scrubbed raw_text.
                            // Same trust basis as V3 (effective values, System replay).
                            match bincode::deserialize::<WalLearnEntryV3>(&payload[1..]) {
                                Ok(cmd) => {
                                    let _ = process_learn(
                                        &mut state,
                                        &cmd.state_emb, &cmd.action_emb, &cmd.outcome_emb,
                                        cmd.prediction_error, cmd.emotional_tag, &cmd.model_id,
                                        cmd.authority, cmd.provenance, cmd.supersedes,
                                        crate::authority::CallerRole::System,
                                         cmd.assertion_key, cmd.polarity,
                                         cmd.raw_text.as_ref().map(|(s, _, _)| s.as_str()),
                                         &cfg,
                                    );
                                    replayed += 1;
                                }
                                Err(e) => tracing::warn!("WAL LearnV4 decode error: {e}"),
                            }
                        }
                        WAL_TYPE_LEARN_V3 => {
                            // v3-hardened Learn: full effective authority + provenance + supersedes.
                            // SECURITY: WAL stores the EFFECTIVE (post-clamping) values.
                            // Replay with CallerRole::System so authenticate_and_clamp
                            // passes through the already-normalized values without re-clamping.
                            match bincode::deserialize::<WalLearnEntryV2>(&payload[1..]) {
                                Ok(cmd) => {
                                    let _ = process_learn(
                                        &mut state,
                                        &cmd.state_emb, &cmd.action_emb, &cmd.outcome_emb,
                                        cmd.prediction_error, cmd.emotional_tag, &cmd.model_id,
                                        cmd.authority, cmd.provenance, cmd.supersedes,
                                        crate::authority::CallerRole::System,
                                         cmd.assertion_key, cmd.polarity, None,
                                         &cfg,
                                    );
                                    replayed += 1;
                                }
                                Err(e) => tracing::warn!("WAL LearnV3 decode error: {e}"),
                            }
                        }
                        WAL_TYPE_LEARN_V2 => {
                            // Legacy v2 Learn: may contain pre-fix unauthenticated authority/provenance.
                            // SECURITY: Replay conservatively with CallerRole::Agent so any forged
                            // authority >0.60 or unauthenticated "user_explicit" provenance is clamped!
                            match bincode::deserialize::<WalLearnEntryV2>(&payload[1..]) {
                                Ok(cmd) => {
                                    let _ = process_learn(
                                        &mut state,
                                        &cmd.state_emb, &cmd.action_emb, &cmd.outcome_emb,
                                        cmd.prediction_error, cmd.emotional_tag, &cmd.model_id,
                                        cmd.authority, cmd.provenance, cmd.supersedes,
                                        crate::authority::CallerRole::Agent,
                                         cmd.assertion_key, cmd.polarity, None,
                                         &cfg,
                                    );
                                    replayed += 1;
                                }
                                Err(e) => tracing::warn!("WAL LearnV2 (legacy) decode error: {e}"),
                            }
                        }
                        WAL_TYPE_LEARN => {
                            // v1/v2 typed Learn (type 0x00): strip type byte, deserialize legacy entry.
                            // Replay with neutral defaults for the new fields (backward compat).
                            match bincode::deserialize::<WalLearnEntry>(&payload[1..]) {
                                Ok(cmd) => {
                                    let _ = process_learn(
                                        &mut state,
                                        &cmd.state_emb, &cmd.action_emb, &cmd.outcome_emb,
                                        cmd.prediction_error, cmd.emotional_tag, &cmd.model_id,
                                        0.5, String::new(), None,
                                        crate::authority::CallerRole::User,
                                         None, None, None,
                                         &cfg,
                                    );
                                    replayed += 1;
                                }
                                Err(e) => tracing::warn!("WAL Learn[typed] decode error: {e}"),
                            }
                        }
                        WAL_TYPE_FEEDBACK => {
                            // Feedback record: re-apply with feedback_seen dedup → exactly-once.
                            match bincode::deserialize::<WalFeedbackEntry>(&payload[1..]) {
                                Ok(fb) => {
                                    let _ = process_feedback_nodes(
                                        &mut state,
                                        &[fb.node_id],
                                        fb.outcome,
                                        Some(&fb.retrieve_id),
                                    );
                                    replayed += 1;
                                }
                                Err(e) => tracing::warn!("WAL Feedback decode error: {e}"),
                            }
                        }
                        _ => {
                            // Legacy untyped Learn (pre-44e4b15): whole payload = WalLearnEntry.
                            // Replay with neutral defaults for the new fields.
                            match bincode::deserialize::<WalLearnEntry>(payload) {
                                Ok(cmd) => {
                                    let _ = process_learn(
                                        &mut state,
                                        &cmd.state_emb, &cmd.action_emb, &cmd.outcome_emb,
                                        cmd.prediction_error, cmd.emotional_tag, &cmd.model_id,
                                        0.5, String::new(), None,
                                        crate::authority::CallerRole::User,
                                         None, None, None,
                                         &cfg,
                                    );
                                    replayed += 1;
                                }
                                Err(e) => tracing::debug!("WAL legacy decode error (skipping): {e}"),
                            }
                        }
                    }
                }
                tracing::info!("WAL replay complete: {replayed} entries replayed into state");
            }
            Err(e) => {
                tracing::error!("Failed to read WAL at {:?}: {e}", wal_path);
                replay_failed = true;
            }
        }
    }

    if replay_failed && !cfg.allow_incompatible_snapshot_reset {
        anyhow::bail!("WAL replay failed — refusing to start engine in inconsistent state");
    }

    // ── Step 2b: Rebuild BM25 lexical index ───────────────────────────────
    // The index is `serde(skip)` — never in the snapshot file. Rebuild from
    // persisted Node.text (text-path learns) so lexical retrieval works
    // immediately after restart. Vector-path / legacy nodes have empty text
    // and are skipped (dense-only, unchanged behavior).
    state.rebuild_bm25_index();

    // ── Step 3: Compaction point ──────────────────────────────────────────
    write_snapshot(&state, &snapshot_path)?;
    if wal_path.exists() {
        let _ = std::fs::remove_file(&wal_path);
    }

    // ── Step 4: Open fresh WAL ────────────────────────────────────────────
    let wal = Some(WalWriter::open(&wal_path)?);

    // ── Step 5: Start worker thread ───────────────────────────────────────
    let (cmd_tx, cmd_rx) = mpsc::sync_channel::<Command>(64);
    let worker_cfg = cfg.clone();
    let worker_snapshot_path = snapshot_path.clone();
    let worker_wal_path = wal_path.clone();


    std::thread::Builder::new()
        .name("cmhm-worker".to_string())
        .spawn(move || {
            worker_loop(state, cmd_rx, wal, worker_cfg, worker_snapshot_path, worker_wal_path);
        })?;

    Ok(EngineHandle { tx: cmd_tx })
}

// ─── Worker Thread ─────────────────────────────────────────────────────────

fn worker_loop(
    mut state: CoreState,
    rx: mpsc::Receiver<Command>,
    mut wal: Option<WalWriter>,
    cfg: Config,
    snapshot_path: PathBuf,
    _wal_path: PathBuf,
) {
    let mut rng = SmallRng::from_entropy();
    let mut cmd_count: u64 = 0;

    loop {
        let cmd = match rx.recv() {
            Ok(c) => c,
            Err(_) => break,
        };

        match cmd {
             Command::Retrieve { query_emb, model_id, seed_type, query_text, tx } => {
                 let result = process_retrieve(&state, &query_emb, &model_id, seed_type, &query_text, &mut rng, &cfg);
                let _ = tx.send(result);
            }

             Command::Learn { state_emb, action_emb, outcome_emb, prediction_error, emotional_tag, model_id, authority, provenance, supersedes, caller_role, assertion_key, polarity, raw_text, tx } => {
                let result = process_learn(
                    &mut state, &state_emb, &action_emb, &outcome_emb,
                    prediction_error, emotional_tag, &model_id,
                    authority, provenance.clone(), supersedes,
                     caller_role, assertion_key.clone(), polarity,
                     raw_text.as_ref().map(|(s, _, _)| s.as_str()),
                     &cfg,
                );
                // Atomic WAL append: ONLY write to WAL if process_learn succeeded!
                // SECURITY: Write the EFFECTIVE (post-clamping) authority and provenance,
                // not the raw caller-supplied values. This ensures WAL replay cannot
                // restore a higher privilege level than was actually granted at learn time.
                if let Ok(ref lr) = result {
                    if let Some(wal) = &mut wal {
                        // V4 when this learn carried scrubbed text (BM25 must survive
                        // restart); V3 otherwise (bytes identical to before).
                        if let Some((s, a, o)) = raw_text.as_ref() {
                            let entry = WalLearnEntryV3 {
                                state_emb:        state_emb.clone(),
                                action_emb:       action_emb.clone(),
                                outcome_emb:      outcome_emb.clone(),
                                prediction_error,
                                emotional_tag,
                                model_id:         model_id.clone(),
                                authority:        lr.effective_authority,
                                provenance:       lr.effective_provenance.clone(),
                                supersedes,
                                assertion_key:    assertion_key.clone(),
                                polarity,
                                raw_text:         Some((s.clone(), a.clone(), o.clone())),
                            };
                            if let Ok(payload) = bincode::serialize(&entry) {
                                let mut tagged = Vec::with_capacity(1 + payload.len());
                                tagged.push(WAL_TYPE_LEARN_V4);
                                tagged.extend_from_slice(&payload);
                                let _ = wal.append(&tagged);
                            }
                        } else {
                        let entry = WalLearnEntryV2 {
                            state_emb:        state_emb.clone(),
                            action_emb:       action_emb.clone(),
                            outcome_emb:      outcome_emb.clone(),
                            prediction_error,
                            emotional_tag,
                            model_id:         model_id.clone(),
                            authority:        lr.effective_authority,
                            provenance:       lr.effective_provenance.clone(),
                            supersedes,
                            assertion_key:    assertion_key.clone(),
                            polarity,
                        };
                        if let Ok(payload) = bincode::serialize(&entry) {
                            let mut tagged = Vec::with_capacity(1 + payload.len());
                            tagged.push(WAL_TYPE_LEARN_V3);
                            tagged.extend_from_slice(&payload);
                            let _ = wal.append(&tagged);
                        }
                        }
                    }
                }
                let _ = tx.send(result);
            }


            Command::ExplicitEdge { state_id, action_id, outcome_id, success, tx } => {
                let result = process_explicit_edge(&mut state, state_id, action_id, outcome_id, success);
                let _ = tx.send(result);
            }

            Command::FeedbackNodes { node_ids, success, feedback_key, tx } => {
                let result = process_feedback_nodes(&mut state, &node_ids, success, feedback_key.as_deref());
                // WAL append: one typed record per applied (not skipped) node.
                // Replay re-checks feedback_seen → exactly-once even if WAL is replayed twice.
                if let Some(wal) = &mut wal {
                    if let Some(key) = &feedback_key {
                        let ts = crate::core_state::now_secs();
                        let outcome_bit = if success { 1u64 } else { 0u64 };
                        for &nid in &node_ids {
                            // Only WAL-log nodes that were actually applied (check seen set).
                            let hash = crate::core_state::fnv64(&format!("{key}/{nid}/{outcome_bit}"));
                            if state.feedback_seen.contains(&hash) {
                                // This node is now in feedback_seen → it was just applied.
                                let fb = WalFeedbackEntry {
                                    retrieve_id: key.clone(),
                                    node_id:     nid,
                                    outcome:     success,
                                    ts_secs:     ts,
                                };
                                if let Ok(payload) = bincode::serialize(&fb) {
                                    let mut tagged = Vec::with_capacity(1 + payload.len());
                                    tagged.push(WAL_TYPE_FEEDBACK);
                                    tagged.extend_from_slice(&payload);
                                    let _ = wal.append(&tagged);
                                }
                            }
                        }
                    }
                }
                let _ = tx.send(result);
            }

            Command::Snapshot { path, tx } => {
                let result = write_snapshot(&state, &path)
                    .map_err(Into::into);
                let _ = tx.send(result);
            }

            Command::MaxCosine { query_emb, model_id, node_type, tx } => {
                // Read-only — no WAL, no state change.
                let score = match node_type {
                    Some(nt) => state.node_store.max_cosine_to_existing_typed(&query_emb, &model_id, nt),
                    None     => state.node_store.max_cosine_to_existing(&query_emb, &model_id),
                };
                let _ = tx.send(score);
            }

            Command::InspectGate { query_emb, model_id, node_type, tx } => {
                // Read-only — no WAL, no state change.
                let nearest = state.node_store.nearest_by_type(&query_emb, &model_id, 1, node_type);
                let (max_cosine, nearest_node_id, nearest_bandit) = if let Some(&(id, sim)) = nearest.first() {
                    let bandit = state.node_store.get(id).map(|n| (n.bandit_alpha, n.bandit_beta));
                    (sim, Some(id), bandit)
                } else {
                    (0.0, None, None)
                };
                let c_idx = state.cohorts.label(&query_emb);
                let cohort_dist = state.cohorts.centroids.get(c_idx).map(|c| {
                    query_emb.iter().zip(c.iter()).map(|(x, y)| (x - y).powi(2)).sum::<f32>().sqrt()
                });
                let _ = tx.send(GateInspectionResult {
                    max_cosine,
                    nearest_node_id,
                    nearest_bandit,
                    cohort_dist,
                });
            }

            Command::NodeCount { tx } => {
                // Read-only — no WAL, no state change.
                let _ = tx.send(state.node_store.len());
            }

            Command::GetNodeBeta { node_id, tx } => {
                // Read-only — no WAL, no state change.
                let result = state.node_store.get(node_id)
                    .map(|n| (n.bandit_alpha, n.bandit_beta));
                let _ = tx.send(result);
            }

            Command::GetBirthQ30 { tx } => {
                // Read-only — no WAL, no state change.
                let _ = tx.send(state.birth_q30);
            }

            Command::RegisterProjection { model_id, input_dim, matrix, bias, tx } => {
                let result = process_register_projection(&mut state, model_id, input_dim, matrix, bias);
                let _ = tx.send(result);
            }

            Command::Shutdown { tx } => {
                tracing::info!("Worker shutting down. Taking final snapshot.");
                let res = write_snapshot(&state, &snapshot_path);
                let _ = tx.send(res);
                break;
            }
        }

        state.cmd_seq += 1;
        cmd_count += 1;

        // Periodic snapshot every `snapshot_interval_cmds` commands (default 1000).
        // `.max(1)` guards a zero value in user-supplied config (modulo-by-zero panics).
        if cmd_count % cfg.snapshot_interval_cmds.max(1) == 0 {
            let _ = write_snapshot(&state, &snapshot_path);
        }

        // Consolidation check
        if state.should_consolidate(&cfg) {
            tracing::info!("Running consolidation (nodes={}, edges={}, reinforce={})",
                state.node_store.len(), state.edge_store.len(), state.cumulative_reinforce);
            let now = now_secs();
            run_consolidation(
                &mut state.node_store,
                &mut state.edge_store,
                &mut state.dense_core,
                &mut state.episode_log,
                &mut state.cohorts,
                &mut state.bm25_index,
                &state.projections,
                now,
                &mut rng,
            );
            state.last_consolidation_secs = now;
            state.cumulative_reinforce = 0.0;
        }
    }
}

// ─── Command Processors ────────────────────────────────────────────────────

fn process_retrieve(
    state: &CoreState,
    raw_emb: &[f32],
    model_id: &str,
     seed_type: Option<NodeType>,
     query_text: &str,
     rng: &mut impl rand::Rng,
    cfg: &Config,
) -> Result<RetrievalResult> {
    let projected = state.projections.project(model_id, raw_emb)
        .map_err(|e| anyhow::anyhow!("Projection error: {e}"))?;

    Ok(retrieval::retrieve(
        &projected,
        model_id,
        &state.node_store,
        &state.edge_store,
        &state.dense_core,
        rng,
        cfg.retrieval_max_depth,
        cfg.retrieval_query_weight,
        cfg.retrieval_context_weight,
        cfg.retrieval_curiosity_threshold,
         seed_type,
         cfg.retrieval_authority_weight,
         cfg.conflict_detection_threshold,
         cfg.abstain_on_conflict,
         query_text,
         &state.bm25_index,
      ))
 }


fn process_learn(
    state: &mut CoreState,
    raw_state: &[f32],
    raw_action: &[f32],
    raw_outcome: &[f32],
    epsilon: f32,
    eta: f32,
    model_id: &str,
    authority: f32,
    provenance: String,
    supersedes: Option<u64>,
    caller_role: crate::authority::CallerRole,
     assertion_key: Option<String>,
     polarity: Option<i8>,
     text: Option<&str>,
     cfg: &Config,
) -> Result<LearnResult> {
    // Fail-safe validation for non-finite values and out-of-range inputs
    if !epsilon.is_finite() || !(0.0..=1.0).contains(&epsilon) {
        return Err(anyhow::anyhow!("prediction_error must be a finite number in [0.0, 1.0]"));
    }
    if !eta.is_finite() || !(0.0..=1.0).contains(&eta) {
        return Err(anyhow::anyhow!("emotional_tag must be a finite number in [0.0, 1.0]"));
    }
    if raw_state.is_empty() || raw_state.iter().any(|x| !x.is_finite()) {
        return Err(anyhow::anyhow!("state embedding contains non-finite values (NaN or Inf) or is empty"));
    }
    if raw_action.is_empty() || raw_action.iter().any(|x| !x.is_finite()) {
        return Err(anyhow::anyhow!("action embedding contains non-finite values (NaN or Inf) or is empty"));
    }
    if raw_outcome.is_empty() || raw_outcome.iter().any(|x| !x.is_finite()) {
        return Err(anyhow::anyhow!("outcome embedding contains non-finite values (NaN or Inf) or is empty"));
    }

    let now = now_secs() as f32;

    // B6: Supersession validation BEFORE node creation and BEFORE any mutation.
    if let Some(old_id) = supersedes {
        let old_node = state.node_store.get(old_id)
            .ok_or_else(|| SupersessionError::TargetNotFound(old_id))?;
        if old_node.node_type != NodeType::State {
            return Err(SupersessionError::CrossTypeMismatch(old_id, old_node.node_type).into());
        }
        if let Some(existing_successor) = old_node.superseded_by {
            return Err(SupersessionError::AlreadySuperseded(old_id, existing_successor).into());
        }
    }

    // Project all three embeddings into node-store space.
    let state_proj  = state.projections.project(model_id, raw_state)?;
    let action_proj = state.projections.project(model_id, raw_action)?;
    let outcome_proj = state.projections.project(model_id, raw_outcome)?;

    // B4: Authenticate caller role and clamp authority to authorized class bounds.
    let (authority, provenance) = crate::authority::authenticate_and_clamp(authority, &provenance, caller_role);

    // Capture the effective (clamped) values for durable WAL recording.
    let effective_authority = authority;
    let effective_provenance = provenance.clone();

    // Birth nodes
      let state_id  = birth_node(&mut *state, raw_state.to_vec(),  &state_proj,  model_id, epsilon as f64, eta as f64, NodeType::State,   now as i64, authority, provenance.clone(), assertion_key, polarity, text);
     let action_id = birth_node(&mut *state, raw_action.to_vec(), &action_proj, model_id, epsilon as f64, eta as f64, NodeType::Action,  now as i64, 0.5, String::new(), None, None, None);
     let outcome_id= birth_node(&mut *state, raw_outcome.to_vec(),&outcome_proj,model_id, epsilon as f64, eta as f64, NodeType::Outcome, now as i64, 0.5, String::new(), None, None, None);

    // Fail-closed: birth_node returns 0 when no projection exists for
    // the model_id. Refuse the whole learn rather than recording
    // half-built graph state.
    if state_id == 0 || action_id == 0 || outcome_id == 0 {
        return Err(anyhow::anyhow!(
            "process_learn: node birth refused — one or more embeddings \
             could not be projected to EMBED_DIM. Register the missing \
             projection or check the model_id."
        ));
    }

    // B6: Apply supersession link to old node
    if let Some(old_id) = supersedes {
        if old_id == state_id {
            // Dedup returned existing node -> self-loop
            return Err(SupersessionError::SelfLoop(old_id).into());
        }
        if let Some(old_node) = state.node_store.get_mut(old_id) {
            old_node.superseded_by = Some(state_id);
            tracing::debug!("Node {} superseded by {}", old_id, state_id);
        }
    }

    // Update the CausalHypothesis edge Action→Outcome
    update_causal_edge(state, action_id, outcome_id, epsilon, now, cfg);

    // δ-immunity: if this learn was high-surprise, mark all three nodes as δ-immune.
    for id in [state_id, action_id, outcome_id] {
        if let Some(node) = state.node_store.get_mut(id) {
            node.update_max_prediction_error(epsilon);
        }
    }

    // Update cumulative reinforcement tracker
    state.cumulative_reinforce += (epsilon - 0.5).abs();

    Ok(LearnResult {
        state_node_id: state_id,
        action_node_id: action_id,
        outcome_node_id: outcome_id,
        effective_authority,
        effective_provenance,
    })
}


/// Create or find a node for an embedding, following the birth decision logic.
fn birth_node(
    state: &mut CoreState,
    full_emb: Vec<f32>,
    proj_emb: &[f32; EMBED_DIM],
    model_id: &str,
    epsilon: f64,
    eta: f64,
    node_type: NodeType,
    timestamp: i64,
    authority: f32,
    provenance: String,
     assertion_key: Option<String>,
     polarity: Option<i8>,
     text: Option<&str>,
 ) -> u64 {
    // Salience calculation
    let salience = state.salience_engine.update_and_calc(epsilon, eta) as f32;

    // Novelty check: only compare against nodes of the same role.
    let nearest = state.node_store.nearest_by_type(proj_emb, model_id, 1, node_type);
    let cos_max = nearest.first().map(|(_, c)| *c).unwrap_or(-1.0);

    let lr = 0.01_f32;
    let indicator = if cos_max < state.birth_q30 { 1.0_f32 } else { 0.0_f32 };
    state.birth_q30 += lr * (0.30 - indicator);
    state.birth_q30 = state.birth_q30.clamp(0.01, 0.99);

    let should_birth = cos_max < state.birth_q30 || salience > 0.70 || epsilon > 0.5;

    if should_birth {
        // Update global EMA with full_emb (tracks semantic centroid)
        state.node_store.update_global_ema(&full_emb);
        // Blend embedding — result is Vec<f32> at the same dim as full_emb
        let blended = state.node_store.blend_with_ema(&full_emb);
        // Project blended embedding to EMBED_DIM and store the projected
        // form. Retrieval compares 128-dim queries, so storing native-dim
        // vectors would make every such node invisible to dense search.
        // Fail-closed: refuse birth when no projection exists.
        let blended_projected: [f32; EMBED_DIM] =
            match state.projections.project(model_id, &blended) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        "birth_node: model_id={} native_dim={} — no projection: {e}; \
                         refusing to birth node without a 128-dim form",
                        model_id, blended.len()
                    );
                    return 0;
                }
            };

         let node = Node::new(blended_projected.to_vec(), model_id.to_string(), salience, timestamp, eta as f32, node_type, authority, provenance)
             .with_assertion(assertion_key, polarity)
             .with_text(text.unwrap_or_default());
         let id = state.node_store.insert(node);
         if let Some(t) = text {
             if !t.is_empty() {
                 state.bm25_index.insert(id, t);
             }
         }

        // Edge birth: connect to each node in working memory
        trigger_edge_birth(state, id, timestamp, &blended_projected);

        // Push to working memory
        state.working_memory.push(id);

        id
    } else {
        // Return the nearest existing node
        nearest.first().map(|(id, _)| *id).unwrap_or_else(|| {
            let node = Node::new(full_emb, model_id.to_string(), salience, timestamp, eta as f32, node_type, authority, provenance)
                .with_assertion(assertion_key, polarity);
            state.node_store.insert(node)
        })
    }
}

/// Edge birth: for each node in working memory (excluding `new_id`), attempt inferred edge.

/// `new_emb` is the projected 128-dim embedding used for dense-core updates.
fn trigger_edge_birth(state: &mut CoreState, new_id: u64, new_ts: i64, new_emb: &[f32; EMBED_DIM]) {
    let lambda_t = 0.0001_f32;
    let threshold = 0.35_f32;
    let now_f = now_secs() as f32;

    let candidates: Vec<u64> = state.working_memory.recent_excluding(new_id).collect();

    for src_id in candidates {
        if let Some(src_node) = state.node_store.get(src_id) {
            let dt = (new_ts - src_node.timestamp).abs() as f32;
            let phi = (-lambda_t * dt).exp();
            // Use min-len cosine for compatibility with different dims during transition
            let min_len = src_node.embedding.len().min(new_emb.len());
            let sim = crate::node_store::cosine(&src_node.embedding[..min_len], &new_emb[..min_len]).max(0.0) * phi;

            if sim > threshold {
                let new_node_type = state.node_store.get(new_id)
                    .map(|n| n.node_type)
                    .unwrap_or(NodeType::State);

                let edge_type = if src_node.node_type == NodeType::Action && new_node_type == NodeType::Outcome {
                    EdgeType::CausalHypothesis
                } else {
                    EdgeType::Sequence
                };
                let is_intervention = src_node.node_type == NodeType::Action;

                let edge = Edge::new(1.0, 1.0, now_f, edge_type, EvidenceType::Inferred, is_intervention);

                // Dense core update uses projected 128-dim vectors
                let slice = DenseCore::slice_index(edge_type as u8, src_node.node_type as u8, new_node_type as u8);
                // Project src_node embedding to EMBED_DIM. Fail-closed on missing projection.
                let src_projected = state.projections.project(&src_node.model_id, &src_node.embedding);
                match src_projected {
                    Ok(src_proj) => {
                        state.dense_core.update(slice, &src_proj, new_emb, 0.5);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "trigger_edge_birth: src model_id={} native_dim={} — no projection: {e}; skipping dense update",
                            src_node.model_id, src_node.embedding.len()
                        );
                    }
                }

                state.edge_store.upsert(src_id, new_id, edge);
            }
        }
    }
}

/// Apply forgetting + Beta update to the Action→Outcome CausalHypothesis edge.
fn update_causal_edge(
    state: &mut CoreState,
    action_id: u64,
    outcome_id: u64,
    epsilon: f32,
    now: f32,
    _cfg: &Config,
) {
    let lambda_c_per_sec = 0.1_f32 / 3600.0;

    // Ensure edge exists
    if state.edge_store.get(action_id, outcome_id).is_none() {
        let edge = Edge::new(1.0, 1.0, now, EdgeType::CausalHypothesis, EvidenceType::Inferred, true);
        state.edge_store.upsert(action_id, outcome_id, edge);
    }

    let outcome_salience = state.node_store.get(outcome_id)
        .map(|n| n.salience)
        .unwrap_or(0.5);

    // PRE-COMPUTE state_id for credit propagation BEFORE taking mutable borrow of edge_store.
    // This avoids the E0502 conflict (can't borrow edge_store immutably while mutably borrowed).
    let state_id_for_credit: Option<u64> = {
        let wm_ids: Vec<u64> = state.working_memory.recent_excluding(action_id).collect();
        wm_ids.into_iter().find(|&sid| {
            state.edge_store.get(sid, action_id).is_some()
                && state.node_store.get(sid)
                    .map(|n| n.node_type == NodeType::State)
                    .unwrap_or(false)
        })
    };

    let was_success = epsilon <= 0.7;
    let mut increment = 0.0f32;
    let mut final_w = 0.5f32;

    // Mutable borrow scope — update edge, then release before any further edge_store reads.
    {
        if let Some(edge) = state.edge_store.get_mut(action_id, outcome_id) {
            // Temporal forgetting
            let dt = (now - edge.last_traversal_time).max(0.0);
            let decay = (-lambda_c_per_sec * dt).exp();
            edge.alpha = (edge.alpha * decay).max(0.01);
            edge.beta  = (edge.beta  * decay).max(0.01);

            // Learning rate multipliers
            let mut lr = 1.0_f32;
            if edge.is_intervention { lr *= 4.0; }
            if edge.evidence_type == EvidenceType::Exact { lr *= 4.0; }
            lr = lr.max(0.1);

            increment = outcome_salience * lr;
            if was_success {
                edge.alpha += increment;
            } else {
                edge.beta += increment;
            }
            edge.last_traversal_time = now;
            final_w = edge.alpha / (edge.alpha + edge.beta);
        }
    } // mutable borrow of edge_store released here

    if increment > 0.0 {
        // Episode log for credit propagation
        if let Some(state_id) = state_id_for_credit {
            state.episode_log.log(state_id, action_id, increment, was_success);
        }

        // Dense core update — safe now, edge_store not borrowed
        let a_emb  = state.node_store.get(action_id).map(|n| n.embedding.clone());
        let a_type = state.node_store.get(action_id).map(|n| n.node_type as u8);
        let a_mid  = state.node_store.get(action_id).map(|n| n.model_id.clone());
        let o_emb  = state.node_store.get(outcome_id).map(|n| n.embedding.clone());
        let o_type = state.node_store.get(outcome_id).map(|n| n.node_type as u8);
        let o_mid  = state.node_store.get(outcome_id).map(|n| n.model_id.clone());

        if let (Some(a_emb), Some(a_type), Some(o_emb), Some(o_type), Some(a_mid), Some(o_mid)) =
            (a_emb, a_type, o_emb, o_type, a_mid, o_mid)
        {
            let slice = DenseCore::slice_index(EdgeType::CausalHypothesis as u8, a_type, o_type);
            // Project both embeddings to EMBED_DIM. Both must succeed; either
            // failing warns and skips the dense update (fail-closed).
            match (state.projections.project(&a_mid, &a_emb),
                   state.projections.project(&o_mid, &o_emb))
            {
                (Ok(ap), Ok(op)) => {
                    state.dense_core.update(slice, &ap, &op, final_w);
                }
                (a_res, o_res) => {
                    let a_ok = a_res.is_ok();
                    let o_ok = o_res.is_ok();
                    tracing::warn!(
                        "update_causal_edge: dense update skipped (action_ok={}, outcome_ok={}); \
                         action_mid={} action_dim={} outcome_mid={} outcome_dim={}",
                        a_ok, o_ok, a_mid, a_emb.len(), o_mid, o_emb.len()
                    );
                }
            }
        }
    }
}

/// Update bandit posteriors and half-life for a list of retrieved node IDs.
///
/// Called by the `memory_feedback` MCP tool after the agent observes whether
/// following the retrieved memory advice led to success or failure.
///
/// Returns the count of node IDs that were found and updated.
/// Node IDs not found in the store are silently skipped (stale references are fine).
/// Apply outcome feedback to the given node IDs.
///
/// If `feedback_key` is provided, each node's effective key is
/// `"{feedback_key}/{node_id}/{success_as_01}"`. Nodes whose key is already in
/// `state.feedback_seen` are skipped — their Beta posterior is NOT updated again.
/// This makes duplicate calls (from retries, WAL replays, or concurrent MCP calls)
/// a no-op: Alpha(α,β) reflects evidence count, not submission count.
///
/// If `feedback_key` is None (unjoined feedback), feedback is applied unconditionally.
/// The caller is responsible for upstream dedup in this case.
fn process_feedback_nodes(
    state: &mut CoreState,
    node_ids: &[u64],
    success: bool,
    feedback_key: Option<&str>,
) -> Result<usize> {
    let now = now_secs() as i64;
    let outcome_bit = if success { 1 } else { 0 };
    let mut updated = 0usize;
    for &id in node_ids {
        // Dedup check: if a stable key exists, skip already-applied feedback.
        // Key is hashed via FNV-1a 64-bit to reduce snapshot size (8 B vs 60 B/entry).
        let seen_hash = feedback_key.map(|k| {
            crate::core_state::fnv64(&format!("{k}/{id}/{outcome_bit}"))
        });
        if let Some(h) = seen_hash {
            if state.feedback_seen.contains(&h) {
                // Duplicate: this (retrieve, node, outcome) triple was already applied.
                // Skipping prevents Alpha from being inflated by call volume.
                continue;
            }
        }
        if let Some(node) = state.node_store.get_mut(id) {
            node.update_from_outcome(success);
            node.record_retrieval(now);
            updated += 1;
            // Mark applied — subsequent identical calls are no-ops.
            if let Some(h) = seen_hash {
                state.feedback_seen.insert(h);
            }
        }
    }
    Ok(updated)
}

fn process_explicit_edge(
    state: &mut CoreState,
    state_id: u64,
    action_id: u64,
    outcome_id: u64,
    success: bool,
) -> Result<()> {
    let now = now_secs() as f32;
    let (alpha, beta) = if success { (5.0, 1.0) } else { (1.0, 5.0) };

    // State→Action edge (Sequence, Exact)
    let sa_edge = Edge::new(alpha, beta, now, EdgeType::Sequence, EvidenceType::Exact, false);
    state.edge_store.upsert(state_id, action_id, sa_edge);

    // Action→Outcome edge (CausalHypothesis, Exact, Intervention)
    let ao_edge = Edge::new(alpha, beta, now, EdgeType::CausalHypothesis, EvidenceType::Exact, true);
    state.edge_store.upsert(action_id, outcome_id, ao_edge);

    // Push to working memory
    state.working_memory.push(state_id);
    state.working_memory.push(action_id);
    state.working_memory.push(outcome_id);

    // Update dense core for both edges
    // Capture node data into owned values so the immutable borrow of
    // state.node_store does not conflict with the mutable borrow of
    // state.dense_core below.
    let state_info = state.node_store.get(state_id)
        .map(|n| (n.embedding.clone(), n.model_id.clone(), n.node_type as u8));
    let action_info = state.node_store.get(action_id)
        .map(|n| (n.embedding.clone(), n.model_id.clone(), n.node_type as u8));

    if let (Some((s_emb, s_mid, s_type)), Some((a_emb, a_mid, a_type))) = (state_info, action_info) {
        let slice = DenseCore::slice_index(EdgeType::Sequence as u8, s_type, a_type);
        let w = alpha / (alpha + beta);
        match (state.projections.project(&s_mid, &s_emb),
               state.projections.project(&a_mid, &a_emb))
        {
            (Ok(sp), Ok(ap)) => {
                state.dense_core.update(slice, &sp, &ap, w);
            }
            (s_res, a_res) => {
                tracing::warn!(
                    "process_explicit_edge: state->action dense update skipped; \
                     state_mid={} state_dim={} state_ok={} \
                     action_mid={} action_dim={} action_ok={}",
                    s_mid, s_emb.len(), s_res.is_ok(),
                    a_mid, a_emb.len(), a_res.is_ok()
                );
            }
        }
    }
    // Capture node data into owned values so the immutable borrow of
    // state.node_store does not conflict with the mutable borrow of
    // state.dense_core below.
    let action_info_b = state.node_store.get(action_id)
        .map(|n| (n.embedding.clone(), n.model_id.clone(), n.node_type as u8));
    let outcome_info = state.node_store.get(outcome_id)
        .map(|n| (n.embedding.clone(), n.model_id.clone(), n.node_type as u8));

    if let (Some((a_emb_b, a_mid_b, a_type_b)), Some((o_emb, o_mid, o_type))) = (action_info_b, outcome_info) {
        let slice = DenseCore::slice_index(EdgeType::CausalHypothesis as u8, a_type_b, o_type);
        let w = alpha / (alpha + beta);
        match (state.projections.project(&a_mid_b, &a_emb_b),
               state.projections.project(&o_mid, &o_emb))
        {
            (Ok(ap), Ok(op)) => {
                state.dense_core.update(slice, &ap, &op, w);
            }
            (a_res, o_res) => {
                tracing::warn!(
                    "process_explicit_edge: action->outcome dense update skipped; \
                     action_mid={} action_dim={} action_ok={} \
                     outcome_mid={} outcome_dim={} outcome_ok={}",
                    a_mid_b, a_emb_b.len(), a_res.is_ok(),
                    o_mid, o_emb.len(), o_res.is_ok()
                );
            }
        }
    }

    Ok(())
}

fn process_register_projection(
    state: &mut CoreState,
    model_id: String,
    input_dim: usize,
    matrix: Vec<f32>,
    bias: [f32; crate::EMBED_DIM],
) -> Result<()> {
    let expected = crate::EMBED_DIM * input_dim;
    if matrix.len() != expected {
        anyhow::bail!(
            "process_register_projection: matrix len {} != expected {}",
            matrix.len(),
            expected
        );
    }
    let proj = crate::projection::Projection {
        input_dim,
        matrix,
        bias,
    };
    state.projections.register(model_id.clone(), proj);
    tracing::info!(
        model_id = %model_id,
        input_dim,
        "registered projection: {} -> {}",
        input_dim,
        crate::EMBED_DIM
    );
    Ok(())
}
