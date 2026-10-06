# CMHM Memory Engine — Audit, Verification & Review Handoff Report

> **Project:** CMHM — Causal Hierarchical Memory Model (Rust workspace)
> **Crates:** `cmhm_core` (library) · `cmhm_mcp` (MCP server binary + lib)
> **Repository Baseline:** Commit `cf10d19` (working tree contains all verified candidate fixes)
> **Verification Status:** All fixes tested against `cargo test --workspace` (338 tests, 0 failures, 0 production warnings)

---

## 1. Verified Architecture & In-Tree Parameter State

```
cmhm_core
├── node.rs          — Node struct: embedding, authority, provenance, bandit (alpha/beta), BM25 text
├── node_store.rs    — NodeStore: HashMap<u64, Node> + nearest() ANN scan
├── edge_store.rs    — EdgeStore: causal edges with Thompson bandit per edge (degree counts in+out)
├── engine.rs        — Single worker thread: learn, retrieve, feedback, WAL, snapshot, dedup
├── retrieval.rs     — retrieve() pipeline: NN → filter → score → Dijkstra causal chain
├── bm25.rs          — Inverted BM25 index (reverse node_terms index for O(terms/doc) removal)
├── dense_core.rs    — DenseCore: outer-product Hebbian associative matrix
├── salience.rs      — Welford online variance (unbiased sample variance: n-1)
├── consolidation.rs — Periodic graph pruning: Newcombe superstition filter + dead-edge removal
├── persistence.rs   — WAL (append-only binary log) + MessagePack/Bincode snapshots
├── embed.rs         — Embedder trait: hash-128d (default) or OnnxEmbedder (MiniLM-L6-v2-int8)
├── projection.rs    — ProjectionRegistry: 384-dim → 128-dim learned projection
├── context/         — Multi-tenant context runtime orchestrator, LRU cache, and FSM lifecycle
└── core_state.rs    — CoreState: master owned state, serialized to snapshot
```

### Actual Shipped Scoring Configuration (`cmhm_core/src/config.rs`)

```rust
retrieval_query_weight:    0.70  // Direct query cosine similarity
retrieval_context_weight:  0.30  // Dense context cosine similarity (sums to 1.0 with query)
retrieval_authority_weight: 0.75  // Tuned from 0.80 -> 0.75 to balance cosine vs contradiction safety
BANDIT_WEIGHT:             0.10  // Thompson draw bonus (retrieval.rs: active only when alpha>1 || beta>1)
```

**Scoring Equation:**
$$\text{Score} = 0.70 \cdot \cos(q, \text{node}) + 0.30 \cdot \cos(\text{ctx}, \text{node}) + 0.75 \cdot \text{node.authority} + 0.10 \cdot \mathbb{I}_{[\alpha>1 \lor \beta>1]} \cdot \text{ThompsonDraw}$$

---

## 2. Empirical Banked Recall Benchmarks (In-Tree)

All numbers are measured using in-tree benchmark harnesses on the banked **honest $n=40$ corpus** (`honest_bench.rs` and `honest_minilm_bench.rs`).

| Embedder | Dimensionality | Banked Recall (n=40) | Test Suite Harness |
|---|---|---|---|
| `hash-128d` | 128 (FNV-1a trigrams) | **5.0%** (0.05) | `tests/honest_bench.rs` |
| `minilm-l6-v2-int8` | 384 (ONNX projected to 128) | **40.0%** (0.40) | `tests/honest_minilm_bench.rs` |

---

## 3. Review Flag Resolutions & Structural Fixes

### Flag 1 — H-07 Redesign: Option (b) Random 128-Bit IDs
- **Problem:** `WalWriter::open` resets `seq: 0` on every session startup. Using `wal-seq-N` as unkeyed IDs caused collisions between old session keys in snapshot `feedback_seen` and fresh new session records, causing subsequent unkeyed feedback to be silently dropped.
- **Root Cause & Fix:**
  1. **Write site (`engine.rs`):** Unkeyed feedback now synthesizes a random 128-bit identifier (`format!("unkeyed-{:032x}", rand::random::<u128>())`) *before* invoking `process_feedback_nodes`.
  2. **Replay site (`engine.rs`):** Deserialized records pass `Some(fb.retrieve_id.as_str())` into `process_feedback_nodes`. Snapshot `feedback_seen` properly gates duplicate replay on crash, while future sessions mint fresh 128-bit random IDs with $P(\text{collision}) \approx 2^{-128} \approx 0$.
  3. **Verification:** Verified by dedicated 3-session restart cycle test `test_unkeyed_feedback_restart_seq_reset_cycle` covering learn $\to$ feedback $\to$ clean shutdown $\to$ reload (seq reset) $\to$ new unkeyed feedback $\to$ crash $\to$ crash recovery replay.

### M-05 Bincode Positional Integrity Resolution
- **Problem:** Attempting `skip_serializing` on `CoreState::global_ema` broke bincode's schema-less positional deserialization, causing snapshot loads to read `feedback_seen` bytes as 128 floats and fail with `payload corrupt (expected exactly 128 floats, got 1)`.
- **Fix:** Restored `#[serde(with = "crate::serde_helpers::f32_128")]` on `global_ema: [f32; 128]`. Preserves exact 512-byte positional alignment across all snapshots.

### Issue H2 — Gate Inspects Sleeping Context as Maximally Novel (RESOLVED)
- **File:** `cmhm_core/src/context/runtime.rs` (`inspect_gate_in_context` and `max_cosine_in_context`)
- **Fix:** Both methods now call `self.ensure_loaded(&id)`. Sleeping contexts are brought into RAM so novelty calculations evaluate against real historical nodes rather than falsely reporting $0.0$ max cosine. Tradeoff noted: inspection may trigger LRU eviction / disk load if contexts are sleeping.

### Issue H-12 — Asymmetric Assertion Clobbering by Low-Authority Near-Duplicates (RESOLVED)
- **File:** `cmhm_core/src/engine.rs` (`birth_node` dedup branch)
- **Fix:** Guarded assertion updates with `if assertion_key.is_some() && (authority >= existing.authority || is_correction)`. Low-authority near-duplicates cannot clobber high-authority node assertions.

---

## 4. Master Bug Ledger Summary & Reconciliation

- **34 Confirmed Fixes**:
  - Critical (6): C-01..C-06
  - High (12): H-01..H-10, H-12, H2
  - Medium (10): M-01..M-04, M-06/E6f, M-07..M-08, M-10, R6
  - Low/Nit (6): L-01, L-03..L-07
- **2 Formally Deferred**: H-11 (runtime WAL rotation; requires 2-phase commit checkpointing; startup compaction remains active and safe), M-09 (FNV-1a to XXH3; requires `xxhash-rust` workspace dependency).
- **1 Retracted**: H-13 (retracted; continuous mutex holds across inspect/learn).
- **2 Preset**: L-02 / L-08 (workspace dependencies `libm` and `thiserror` already declared).
- **1 Reverted / Stable**: M-05 (`global_ema` kept in serde layout to preserve bincode positional alignment).
- **Total Catalog Reconciliation**: 34 Fixed + 2 Deferred + 1 Retracted + 2 Preset + 1 Reverted/Stable = **40 tracked items**.

---

## 5. Recommended Staged Commit Plan

1. **`fix(retrieval)`**: R1, R2, R3, R4, R6, H-03 (Carry cosine, config weights 0.70/0.30/0.75, pool size 20, key-guarded abstention, bandit draw).
2. **`fix(engine)`**: H-01, H-02, H-07, H-08, H-10, H-12, M-03, M-04, M-05 (Dimension rejection, BM25 text index, WAL typed records & 128-bit unkeyed IDs, reverse BM25 index, correction updates, resource caps, bincode alignment).
3. **`fix(context)`**: H2, H-09, M-07 (Sleeping context loading in gate, authority clamping, novelty range clamping).
4. **`fix(tools)`**: C-01, M-01, M-02, M-10 (Promote role default, stopword cleanup, manifest section awareness, OnnxEmbedder poison recovery).
5. **`fix(math)`**: C-02, H-04, H-05, L-01, L-03, M-08 (Degree sum, Hebbian no-op, unbiased Welford variance, Marsaglia-Tsang cleanup, Newcombe variance fallback, salience naming).
6. **`docs`**: In-repo bug list and audit fix report (`docs/master_bug_list.md` and `docs/audit_fix_report.md`).
