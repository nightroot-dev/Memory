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
    persistence::{WalWriter, read_wal, write_snapshot, read_snapshot},
    retrieval::{self, RetrievalResult},
};

// ─── Public Command API ────────────────────────────────────────────────────

/// A command sent to the CMHM worker thread.
pub enum Command {
    Retrieve {
        query_emb: Vec<f32>,
        model_id: String,
        tx: mpsc::SyncSender<Result<RetrievalResult>>,
    },
    Learn {
        state_emb: Vec<f32>,
        action_emb: Vec<f32>,
        outcome_emb: Vec<f32>,
        prediction_error: f32,
        emotional_tag: f32,
        model_id: String,
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
    FeedbackNodes {
        node_ids: Vec<u64>,
        success: bool,
        tx: mpsc::SyncSender<Result<usize>>, // returns number of nodes updated
    },
    /// Read-only: max cosine similarity of query against all nodes with model_id.
    /// Used by shadow gate (no WAL write, no state mutation).
    MaxCosine {
        query_emb: Vec<f32>,
        model_id:  String,
        tx: mpsc::SyncSender<f32>,
    },
    Snapshot {
        path: PathBuf,
        tx: mpsc::SyncSender<Result<()>>,
    },
    Shutdown,
}

/// Serialisable record written to WAL for every Learn command.
/// Must contain ALL fields needed to deterministically replay the operation.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct WalLearnEntry {
    state_emb:        Vec<f32>,
    action_emb:       Vec<f32>,
    outcome_emb:      Vec<f32>,
    prediction_error: f32,
    emotional_tag:    f32,
    model_id:         String,
}

/// Result of a Learn command.
#[derive(Debug, Clone)]
pub struct LearnResult {
    pub state_node_id: u64,
    pub action_node_id: u64,
    pub outcome_node_id: u64,
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
    pub fn retrieve(&self, query_emb: Vec<f32>, model_id: impl Into<String>) -> Result<RetrievalResult> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        self.tx.send(Command::Retrieve {
            query_emb,
            model_id: model_id.into(),
            tx: resp_tx,
        })?;
        resp_rx.recv()?
    }

    /// Feed a (state, action, outcome) learning signal to the engine.
    pub fn learn(
        &self,
        state_emb: Vec<f32>,
        action_emb: Vec<f32>,
        outcome_emb: Vec<f32>,
        prediction_error: f32,
        emotional_tag: f32,
        model_id: impl Into<String>,
    ) -> Result<LearnResult> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        self.tx.send(Command::Learn {
            state_emb, action_emb, outcome_emb,
            prediction_error, emotional_tag,
            model_id: model_id.into(),
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
    /// Updates each node's Beta(α,β) bandit posterior and half-life.
    /// Returns the number of nodes that were actually found and updated.
    pub fn feedback_nodes(&self, node_ids: Vec<u64>, success: bool) -> Result<usize> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        self.tx.send(Command::FeedbackNodes { node_ids, success, tx: resp_tx })?;
        resp_rx.recv()?
    }

    /// Maximum cosine similarity of `query_emb` against all existing nodes with `model_id`.
    /// Returns 0.0 on empty store. Read-only — no WAL write.
    /// Used by the shadow gate to compute `novelty = 1 − max_cosine_to_existing`.
    pub fn max_cosine_to_existing(&self, query_emb: &[f32], model_id: &str) -> f32 {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        if self.tx.send(Command::MaxCosine {
            query_emb: query_emb.to_vec(),
            model_id: model_id.to_string(),
            tx: resp_tx,
        }).is_err() {
            return 0.0; // engine shut down
        }
        resp_rx.recv().unwrap_or(0.0)
    }

    /// Shut down the engine gracefully (takes a final snapshot).
    pub fn shutdown(self) -> Result<()> {

        self.tx.send(Command::Shutdown)?;
        Ok(())
    }
}

// ─── Engine Startup ────────────────────────────────────────────────────────

/// Launch the CMHM engine on a background thread.
/// Returns an `EngineHandle` for the caller to interact with.
///
/// ## Startup sequence
/// 1. Load snapshot (if present)
/// 2. Replay WAL on top of snapshot (if WAL present) — this is the crash recovery path
/// 3. Take a fresh snapshot and truncate the WAL (clean state for next session)
/// 4. Open a fresh WAL for the new session
/// 5. Start the worker thread
pub fn start(cfg: Config) -> Result<EngineHandle> {
    // Ensure data dirs exist
    std::fs::create_dir_all(&cfg.data_dir)?;
    std::fs::create_dir_all(&cfg.projections_dir)?;

    let snapshot_path = cfg.data_dir.join("snapshot.bin");
    let wal_path      = cfg.data_dir.join("wal.dat");

    // ── Step 1: Load snapshot ─────────────────────────────────────────────
    let mut state: CoreState = if snapshot_path.exists() {
        match read_snapshot::<CoreState>(&snapshot_path) {
            Ok(s) => {
                tracing::info!("Snapshot loaded from {:?} ({} nodes)",
                    snapshot_path, s.node_store.len());
                s
            }
            Err(e) => {
                tracing::warn!("Snapshot corrupt ({}); starting fresh", e);
                CoreState::new(cfg.working_memory_size)
            }
        }
    } else {
        tracing::info!("No snapshot — starting fresh");
        CoreState::new(cfg.working_memory_size)
    };

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
                let _rng = rand::rngs::SmallRng::from_entropy();
                for entry in &entries {
                    match bincode::deserialize::<WalLearnEntry>(&entry.payload) {
                        Ok(cmd) => {
                            let _ = process_learn(
                                &mut state,
                                &cmd.state_emb,
                                &cmd.action_emb,
                                &cmd.outcome_emb,
                                cmd.prediction_error,
                                cmd.emotional_tag,
                                &cmd.model_id,
                                &cfg,
                            );
                            replayed += 1;
                        }
                        Err(e) => {
                            // WAL entry is a non-Learn record (e.g. debug string from old version);
                            // skip it rather than failing the whole replay.
                            tracing::debug!("Skipping non-Learn WAL entry: {e}");
                        }
                    }
                }
                tracing::info!("WAL replay complete: {} entries replayed", replayed);
            }
            Err(e) => {
                tracing::error!("WAL read failed: {e} — replay aborted");
                replay_failed = true;
            }
        }
    }

    if replay_failed {
        return Err(anyhow::anyhow!("WAL replay failed — context cannot be recovered"));
    }

    // ── Step 3: Checkpoint + truncate WAL ─────────────────────────────────
    // Now that we have a consistent state (snapshot + replayed WAL),
    // write a fresh snapshot and delete the old WAL. The next session
    // starts with a clean WAL.
    if replayed > 0 || wal_path.exists() {
        tracing::info!("Writing post-replay snapshot to {:?}", snapshot_path);
        write_snapshot(&state, &snapshot_path)?;
        // Truncate WAL by removing it — a fresh one will be opened below
        if let Err(e) = std::fs::remove_file(&wal_path) {
            tracing::warn!("Could not remove old WAL {:?}: {e}", wal_path);
        } else {
            tracing::info!("WAL truncated after replay");
        }
    }

    // ── Step 4: Load projection matrices ─────────────────────────────────
    if cfg.projections_dir.exists() {
        for entry in std::fs::read_dir(&cfg.projections_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "bin").unwrap_or(false) {
                let model_id = path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                match std::fs::read(&path) {
                    Ok(data) => match crate::projection::Projection::from_bytes(&data) {
                        Ok(proj) => {
                            state.projections.register(model_id.clone(), proj);
                            tracing::info!("Projection loaded for model '{}'", model_id);
                        }
                        Err(e) => tracing::warn!("Bad projection {:?}: {}", path, e),
                    },
                    Err(e) => tracing::warn!("Cannot read projection {:?}: {}", path, e),
                }
            }
        }
    }

    // ── Step 5: Open fresh WAL + start worker ─────────────────────────────
    let wal_writer = WalWriter::open(&wal_path).ok();

    let (tx, rx) = mpsc::sync_channel::<Command>(64);
    let cfg_clone = cfg.clone();
    std::thread::Builder::new()
        .name("cmhm-worker".to_string())
        .spawn(move || {
            worker_loop(state, rx, wal_writer, cfg_clone, snapshot_path, wal_path);
        })?;

    Ok(EngineHandle { tx })
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
            Command::Retrieve { query_emb, model_id, tx } => {
                let result = process_retrieve(&state, &query_emb, &model_id, &mut rng, &cfg);
                let _ = tx.send(result);
            }

            Command::Learn { state_emb, action_emb, outcome_emb, prediction_error, emotional_tag, model_id, tx } => {
                let result = process_learn(
                    &mut state, &state_emb, &action_emb, &outcome_emb,
                    prediction_error, emotional_tag, &model_id, &cfg,
                );
                // WAL append: write the full entry so it can be replayed on crash recovery
                if let Some(wal) = &mut wal {
                    let entry = WalLearnEntry {
                        state_emb:        state_emb.clone(),
                        action_emb:       action_emb.clone(),
                        outcome_emb:      outcome_emb.clone(),
                        prediction_error,
                        emotional_tag,
                        model_id:         model_id.clone(),
                    };
                    if let Ok(bytes) = bincode::serialize(&entry) {
                        let _ = wal.append(&bytes);
                    }
                }
                let _ = tx.send(result);
            }

            Command::ExplicitEdge { state_id, action_id, outcome_id, success, tx } => {
                let result = process_explicit_edge(&mut state, state_id, action_id, outcome_id, success);
                let _ = tx.send(result);
            }

            Command::FeedbackNodes { node_ids, success, tx } => {
                let result = process_feedback_nodes(&mut state, &node_ids, success);
                let _ = tx.send(result);
            }

            Command::Snapshot { path, tx } => {
                let result = write_snapshot(&state, &path)
                    .map_err(Into::into);
                let _ = tx.send(result);
            }

            Command::MaxCosine { query_emb, model_id, tx } => {
                // Read-only — no WAL, no state change.
                let score = state.node_store.max_cosine_to_existing(&query_emb, &model_id);
                let _ = tx.send(score);
            }

            Command::Shutdown => {
                tracing::info!("Worker shutting down. Taking final snapshot.");
                let _ = write_snapshot(&state, &snapshot_path);
                break;
            }
        }

        state.cmd_seq += 1;
        cmd_count += 1;

        // Periodic snapshot every 1000 commands
        if cmd_count % 1000 == 0 {
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
    cfg: &Config,
) -> Result<LearnResult> {
    let now = now_secs() as f32;

    // Project all three embeddings into node-store space.
    // For hash embedder: raw_* is 128-dim → project → 128-dim [f32; EMBED_DIM]
    // For onnx embedder: raw_* is 384-dim → project → still 128-dim (projection compresses)
    // The Vec<f32> wrappers let node_store hold any dim; we use the projected [f32; EMBED_DIM]
    // for dense_core (fixed at 128-dim until Stage 4) and full raw for node storage.
    let state_proj  = state.projections.project(model_id, raw_state)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let action_proj = state.projections.project(model_id, raw_action)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let outcome_proj = state.projections.project(model_id, raw_outcome)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Store the raw (pre-projection) embedding in the node for semantic fidelity.
    // When onnx is active, this is 384-dim MiniLM. When hash: 128-dim.
    // Dense core always uses the projected 128-dim vector (compatible with EMBED_DIM).
    let state_id  = birth_node(&mut *state, raw_state.to_vec(),  &state_proj,  model_id, epsilon as f64, eta as f64, NodeType::State,   now as i64);
    let action_id = birth_node(&mut *state, raw_action.to_vec(), &action_proj, model_id, epsilon as f64, eta as f64, NodeType::Action,  now as i64);
    let outcome_id= birth_node(&mut *state, raw_outcome.to_vec(),&outcome_proj,model_id, epsilon as f64, eta as f64, NodeType::Outcome, now as i64);

    // Edge birth is triggered automatically inside birth_node via working memory.

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

    Ok(LearnResult { state_node_id: state_id, action_node_id: action_id, outcome_node_id: outcome_id })
}

/// Create or find a node for an embedding, following the birth decision logic.
///
/// Parameters:
/// - `full_emb`: the raw embedding (any dim) to store in the node
/// - `proj_emb`: the projected 128-dim embedding used for similarity + dense core
/// - `model_id`: stamped on the node (N3)
fn birth_node(
    state: &mut CoreState,
    full_emb: Vec<f32>,
    proj_emb: &[f32; EMBED_DIM],
    model_id: &str,
    epsilon: f64,
    eta: f64,
    node_type: NodeType,
    timestamp: i64,
) -> u64 {
    // Salience calculation
    let salience = state.salience_engine.update_and_calc(epsilon, eta) as f32;

    // Novelty check (using projected embedding for comparable dim)
    let nearest = state.node_store.nearest(proj_emb, model_id, 1);
    let cos_max = nearest.first().map(|(_, c)| *c).unwrap_or(-1.0);

    let should_birth = cos_max < 0.55 || salience > 0.70 || epsilon > 0.5;

    if should_birth {
        // Update global EMA with full_emb (tracks semantic centroid)
        state.node_store.update_global_ema(&full_emb);
        // Blend embedding — result is Vec<f32> at the same dim as full_emb
        let blended = state.node_store.blend_with_ema(&full_emb);
        let blended_proj: [f32; EMBED_DIM] = {
            let mut arr = [0.0f32; EMBED_DIM];
            let len = blended.len().min(EMBED_DIM);
            arr[..len].copy_from_slice(&blended[..len]);
            arr
        };

        let node = Node::new(blended, model_id.to_string(), salience, timestamp, eta as f32, node_type);
        let id = state.node_store.insert(node);

        // Edge birth: connect to each node in working memory
        trigger_edge_birth(state, id, timestamp, &blended_proj);

        // Push to working memory
        state.working_memory.push(id);

        id
    } else {
        // Return the nearest existing node
        nearest.first().map(|(id, _)| *id).unwrap_or_else(|| {
            // Fallback: insert anyway
            let node = Node::new(full_emb, model_id.to_string(), salience, timestamp, eta as f32, node_type);
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
                let mut src_proj = [0.0f32; EMBED_DIM];
                let copy_len = src_node.embedding.len().min(EMBED_DIM);
                src_proj[..copy_len].copy_from_slice(&src_node.embedding[..copy_len]);
                state.dense_core.update(slice, &src_proj, new_emb, 0.5);

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
        let o_emb  = state.node_store.get(outcome_id).map(|n| n.embedding.clone());
        let o_type = state.node_store.get(outcome_id).map(|n| n.node_type as u8);

        if let (Some(a_emb), Some(a_type), Some(o_emb), Some(o_type)) =
            (a_emb, a_type, o_emb, o_type)
        {
            let slice = DenseCore::slice_index(EdgeType::CausalHypothesis as u8, a_type, o_type);
            let mut ap = [0.0f32; EMBED_DIM]; let al = a_emb.len().min(EMBED_DIM); ap[..al].copy_from_slice(&a_emb[..al]);
            let mut op = [0.0f32; EMBED_DIM]; let ol = o_emb.len().min(EMBED_DIM); op[..ol].copy_from_slice(&o_emb[..ol]);
            state.dense_core.update(slice, &ap, &op, final_w);
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
fn process_feedback_nodes(
    state: &mut CoreState,
    node_ids: &[u64],
    success: bool,
) -> Result<usize> {
    let now = now_secs() as i64;
    let mut updated = 0usize;
    for &id in node_ids {
        if let Some(node) = state.node_store.get_mut(id) {
            node.update_from_outcome(success);
            node.record_retrieval(now);
            updated += 1;
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
    if let (Some(sn), Some(an)) = (state.node_store.get(state_id), state.node_store.get(action_id)) {
        let slice = DenseCore::slice_index(EdgeType::Sequence as u8, sn.node_type as u8, an.node_type as u8);
        let w = alpha / (alpha + beta);
        let mut sp = [0.0f32; EMBED_DIM]; let sl = sn.embedding.len().min(EMBED_DIM); sp[..sl].copy_from_slice(&sn.embedding[..sl]);
        let mut ap = [0.0f32; EMBED_DIM]; let al = an.embedding.len().min(EMBED_DIM); ap[..al].copy_from_slice(&an.embedding[..al]);
        state.dense_core.update(slice, &sp, &ap, w);
    }
    if let (Some(an), Some(on)) = (state.node_store.get(action_id), state.node_store.get(outcome_id)) {
        let slice = DenseCore::slice_index(EdgeType::CausalHypothesis as u8, an.node_type as u8, on.node_type as u8);
        let w = alpha / (alpha + beta);
        let mut ap = [0.0f32; EMBED_DIM]; let al = an.embedding.len().min(EMBED_DIM); ap[..al].copy_from_slice(&an.embedding[..al]);
        let mut op = [0.0f32; EMBED_DIM]; let ol = on.embedding.len().min(EMBED_DIM); op[..ol].copy_from_slice(&on.embedding[..ol]);
        state.dense_core.update(slice, &ap, &op, w);
    }

    Ok(())
}
