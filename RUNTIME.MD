
---

# CMHM Core Runtime Specification

## 1. Overview

The CMHM core runtime is a **single‑threaded, event‑driven memory processor** implemented in Rust. It manages a typed causal graph, a dense tensor core, and all learning/consolidation logic. The runtime is designed to be embedded into a host process (via MCP server, library, or custom agent loop) and communicates solely through an internal command queue. It does not perform any I/O other than optional disk persistence (WAL & snapshots) and does not have a network layer — networking is the responsibility of the embedding host.

Key design constraints:
- **No GPU usage** — all computation is CPU‑only.
- **Bounded memory** — ~80 MB for a decade of daily interactions.
- **Deterministic, sequential mutation** — only one writer, no locks.
- **Self‑calibrating** — no manual hyperparameter tuning required at runtime.

---

## 2. Runtime Architecture

### 2.1 Thread Model

The runtime consists of exactly **one worker thread**. All mutable state is owned by this thread. Clients (MCP server, embedded API handles) send commands via an MPSC (multi‑producer, single‑consumer) channel. The worker pops commands one by one and executes them sequentially.

```
┌──────────────────────────────────────────────────────┐
│                    HOST PROCESS                       │
│                                                       │
│  ┌───────────┐    ┌───────────────┐    ┌───────────┐ │
│  │  MCP      │    │   Embedded    │    │  Other    │ │
│  │  Server   │    │   API         │    │  callers  │ │
│  └─────┬─────┘    └───────┬───────┘    └─────┬─────┘ │
│        │                  │                  │        │
│        └──────────────────┼──────────────────┘        │
│                           │ MPSC channel               │
│                           ▼                            │
│                ┌──────────────────┐                    │
│                │  WORKER THREAD   │                    │
│                │  (single)        │                    │
│                └────────┬─────────┘                    │
│                         │                              │
│              ┌──────────▼──────────┐                   │
│              │    CORE STATE       │                   │
│              │  - Nodes            │                   │
│              │  - Edges            │                   │
│              │  - Dense Core       │                   │
│              │  - Cohorts          │                   │
│              │  - Salience Stats   │                   │
│              │  - Episode Log      │                   │
│              └──────────┬──────────┘                   │
│                         │                              │
│              ┌──────────▼──────────┐                   │
│              │  PERSISTENCE        │                   │
│              │  - WAL (file)       │                   │
│              │  - Snapshot (file)  │                   │
│              └─────────────────────┘                   │
└──────────────────────────────────────────────────────┘
```

### 2.2 Command Loop

The worker thread executes the following loop (pseudocode):

```rust
fn run(mut state: CoreState, rx: mpsc::Receiver<Command>) {
    loop {
        match rx.recv() {
            Ok(cmd) => {
                state.process_command(cmd);
                if state.should_consolidate() {
                    state.consolidate();
                }
            }
            Err(_) => break, // channel closed
        }
    }
    state.final_snapshot();
}
```

All commands return results through a `oneshot::Sender` included in the command struct, so callers can await completion asynchronously without blocking the worker.

### 2.3 Command Types

```rust
enum Command {
    Retrieve {
        query_emb: [f32; 128],
        model_id: String,
        response_tx: oneshot::Sender<RetrieveResult>,
    },
    Learn {
        state_emb: [f32; 128],
        action_emb: [f32; 128],
        outcome_emb: [f32; 128],
        prediction_error: f32,
        model_id: String,
        response_tx: oneshot::Sender<LearnResult>,
    },
    ExplicitEdge {
        state_id: u64,
        action_id: u64,
        outcome_id: u64,
        success: bool,
        response_tx: oneshot::Sender<()>,
    },
    Snapshot {
        path: PathBuf,
        response_tx: oneshot::Sender<()>,
    },
    Shutdown,
}
```

`Retrieve` is a read‑only operation but still dispatched through the worker to avoid any concurrent access to the graph. In future, snapshot‑based read replicas could be supported, but a single agent’s query rate is extremely low (<100 QPS), so a single thread is more than sufficient.

---

## 3. Core State & Data Structures

All state is held in the `CoreState` struct:

```rust
struct CoreState {
    // Node Store
    nodes: SlotMap<u64, Node>,      // dense, cache-friendly
    node_index: Box<dyn NearestNeighborIndex>,
    
    // Edge Store
    out_edges: HashMap<u64, Vec<(u64, Edge)>>,
    in_edges: HashMap<u64, Vec<(u64, Edge)>>,
    
    // Working Memory
    working_memory: VecDeque<u64>,   // max capacity 5
    
    // Episode Log
    episode_log: Vec<EpisodeLogEntry>,
    
    // Salience Engine
    salience_tracker: SalienceTracker,
    
    // Dense Core
    dense_core: Box<[f32; 128*128*8]>,
    
    // Cohort Centroids
    centroids: Box<[[f32; 128]; 512]>,
    
    // Consolidation bookkeeping
    last_consolidation: Instant,
    cumulative_reinforce: f32,
    
    // Persistence
    wal: Option<WalWriter>,
    snapshot_seq: u64,
    cmd_seq: u64,
    
    // Cross-model projections
    projections: HashMap<String, Projection>,
}
```

### 3.1 Node Store
- **Primary container**: `SlotMap<u64, Node>` — guarantees O(1) access and stable indices. New nodes get a unique key.
- **Nearest neighbor index**: A trait object allowing pluggable backends. Default implementation: brute‑force with SIMD (via `wide` crate) for up to 200k nodes, using f32 dot product on 128‑dim vectors. For larger scale, HNSW or LSH can be swapped in without changing the rest of the system.

### 3.2 Edge Store
- Adjacency lists as `Vec<(target, Edge)>` stored in `out_edges` and `in_edges` hash maps.
- `Edge` struct:
  ```rust
  struct Edge {
      alpha: f32,
      beta: f32,
      last_traversal_time: f32,
      edge_type: u8,         // 0=Sequence, 1=CausalHypothesis
      evidence_type: u8,     // 0=Inferred, 1=Exact
      is_intervention: bool,
  }
  ```
- All lookups are by node ID; deletions update both maps.

### 3.3 Dense Core
- Stored as a contiguous `[f32; 131072]` (128*128*8). Represented as 8 matrices of 128×128.
- Updates: delta rule (see math spec) applied in a tight loop using `ndarray` or custom code with SIMD intrinsics.
- Queries: single matrix‑vector multiply per slice, sum the results.

### 3.4 Cohort Centroids
- 512 vectors of 128 f32, stored as a boxed array. Updated during consolidation via mini‑batch k‑means with momentum 0.8.

### 3.5 Salience Tracker
- Welford accumulators: `count: u64`, `mean: f64`, `m2: f64`.

### 3.6 Episode Log
- `Vec<EpisodeLogEntry>` with fields: `state_id, action_id, delta: f32, was_success: bool`.

---

## 4. Command Processing

### 4.1 Retrieve

1. **Projection**: Apply cross‑model projection to `query_emb` based on `model_id` (if available).
2. **Dense context**: Compute dense context vector `c = sum_{k} C_k * q`.
3. **Seed selection**: Query nearest neighbor index for top‑3 matches to `q`. Compute mixed score `0.7*cos(q, node_emb) + 0.3*cos(c, node_emb)`. Choose highest scoring node as seed.
4. **Thompson‑sampled Dijkstra**:
   - Priority queue seeded with the seed node (cost 0).
   - For each popped node, for each outgoing edge, sample `w_tilde` from Beta(alpha, beta). Edge cost = `-log(w_tilde + 1e-6)`. Push neighbor with accumulated cost.
   - Limit depth to 3.
5. **Path assembly**: Backtrack to construct the chain, compute mean weight, variance, evidence for each edge.
6. **Return**: `RetrieveResult` containing causal chain steps and dense context vector.

### 4.2 Learn

1. **Projection**: Project all three embeddings (state, action, outcome).
2. **Node creation**: For each of the three embeddings, attempt node birth (see Section 5). This may create new nodes and edges.
3. **Edge birth**: After node creation, working memory contains the new nodes; inferred edges are created.
4. **Action→Outcome update**:
   - Find the CausalHypothesis edge between the action and outcome nodes (create if not exists). If explicit edge already provided? The learn command doesn't specify explicit success flag; we infer success from `prediction_error` (≤0.7 success, >0.7 failure).
   - Apply forgetting and update counts as per math spec.
   - Log episode entry if a State→Action edge was traversed in the retrieval path that led to this action. *However, the Learn command does not inherently come from a retrieval*. For simplicity, we assume the agent also provides the state_id and action_id that led to this outcome. The command includes `state_emb` and `action_emb`, so we can locate the corresponding nodes and log the episode.
5. **Dense core update**: Update the dense core for the Action→Outcome edge with its new weight.
6. **Cumulative reinforcement**: Increment `cumulative_reinforce` by `|delta|` for the update.

### 4.3 ExplicitEdge

Directly creates or updates edges between known node IDs with Exact evidence and intervention flag, bypassing similarity thresholds. Used for integration with structured feedback sources (e.g., unit test results).

### 4.4 Snapshot

Triggers a blocking snapshot of the entire state to the given path. Used by the host for backup.

---

## 5. Internal Algorithms

### 5.1 Node Birth
- Input: embedding, salience, timestamp, emotion, type.
- Nearest neighbor check: if `cos_max < 0.55` or `salience > 0.70` or `prediction_error > 0.5`, create node.
- Blend embedding with global EMA, normalize, store.
- Add to working memory buffer (evicting oldest if full).

### 5.2 Edge Birth
- When a new node `j` is created, for each `i` in working memory (excluding `j`):
  - Compute temporal decay and cosine similarity; if >0.35, create edge with Beta(1,1).
- Edge type determined by node types of `i` and `j`.

### 5.3 Edge Life Update
- For a CausalHypothesis edge:
  - Apply exponential forgetting to alpha/beta.
  - Determine success/failure from `epsilon` threshold.
  - Compute increment = `outcome_salience * lr`.
  - Update alpha or beta.
  - Log episode if previous edge was State→Action.

### 5.4 Superstition Filter (Causal Contrast)
- During consolidation:
  - Build cohort counts from intervention edges only.
  - For each intervention edge, compute ATE with Wilson CI.
  - Prune if upper CI <= 0.

### 5.5 Consolidation Order
1. Update centroids (mini‑batch k‑means).
2. Propagate episode log (update State→Action edges).
3. Superstition filter (prune non‑causal edges).
4. Edge death (prune low‑weight, inactive edges).
5. Node death & bridging (cull low‑salience, old nodes; bridge top‑3 pairs).
6. Merge near‑identical nodes (cos>0.95, time window 1h).
7. Skill compilation (high‑confidence chains → Skill nodes).

### 5.6 Dense Core Update
- On edge creation/update/removal, apply delta rule to the appropriate slice.

### 5.7 Cross‑Model Projection
- Stored per model as `Projection { matrix: [f32; 128*128], bias: [f32; 128] }`.
- Apply `out = normalize(matrix * emb + bias)`.

---

## 6. Persistence Details

### 6.1 Write‑Ahead Log (WAL)
- File format: sequence of entries: `[len: u32][cmd_id: u8][payload...][crc32: u32]`.
- Payloads are bincode‑serialized representations of the mutating commands (Learn, ExplicitEdge).
- The WAL is `fsync`ed after each write if durability is required; can be tuned for performance.

### 6.2 Snapshot
- A full snapshot is a bincode‑serialized `CoreState` (minus the channel, WAL handle, and ephemeral fields). It includes:
  - All nodes (embeddings, metadata)
  - All edges
  - Dense core
  - Centroids
  - Salience tracker state
  - Global EMA
  - Snapshot sequence number (the `cmd_seq` of the last included command)
- After writing, the WAL is truncated to that sequence number (all earlier entries can be discarded).

### 6.3 Crash Recovery
1. Read the latest snapshot; instantiate `CoreState` from it.
2. Open the WAL; scan forward to find the first entry with sequence > snapshot_seq.
3. Replay those commands (they only need to be decoded and applied, not re‑logged).
4. Corrupted entries are skipped; the system continues with the last good state.

---

## 7. Error Handling & Resilience

- **NaN embeddings**: detected via `is_nan()` check; command rejected with error, no state mutation.
- **Out‑of‑memory**: the runtime monitors allocation; if any allocation fails, the worker panics (and can be restarted from snapshot). For embedded uses, panic is caught by the host.
- **WAL disk full**: writes fail; the error is returned to the caller, and the command is not applied (to maintain consistency). The host can then pause or alert.
- **Thread panic**: the worker thread is the single point; if it panics, the host should restart the runtime from the last snapshot. Future enhancement: automatic restart with WAL replay.

---

## 8. Performance Budget & Tuning

Target metrics (single agent, 100k nodes, 500k edges):
- Node creation: <10 µs
- Edge creation: <5 µs per edge (batch)
- Retrieval: <100 µs p99
- Consolidation: <50 ms total
- Memory: ~80 MB resident
- CPU: single core at 2 GHz can handle >50,000 learn ops/sec

Tuning knobs (configurable via TOML):
- Working memory size (default 5)
- Edge birth similarity threshold (default 0.35)
- Novelty threshold (default 0.55)
- Forgetting rate λ_c (default 0.1 per hour)
- Consolidation interval / triggers
- WAL sync mode (full fsync, periodic, none)

---

## 9. Build & Deployment

The core is a `no_std`‑compatible library crate with `alloc`, plus a `std` feature enabling threading, file I/O, and networking (optional). The runtime can be compiled as:

- `libcmhm_core.a` — static library for embedding.
- `cmhm_standalone` — binary with built‑in MCP server (uses `std` and `tokio` for async I/O).
- `cmhm_py` — Python module via PyO3.

All components share the same core logic.

---

This is the complete runtime specification. It covers every internal aspect of the CMHM engine, from thread model to persistence, and is designed to be implementable without additional design decisions.