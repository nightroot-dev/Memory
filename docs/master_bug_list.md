# CMHM Master Bug List — Audited Fix Ledger

> Verified against codebase at commit baseline `cf10d19` + working tree.
> Suite: 338 tests, 0 failures, 0 production warnings.

---

## 🔴 CRITICAL (6)

| ID | Location | Summary / Fix | Status |
|---|---|---|---|
| C-01 | `tools.rs:1604` | `handle_memory_promote` defaults caller role to `"agent"` | 🟩 FIXED |
| C-02 | `edge_store.rs:90-92` | `degree()` computes bidirectional sum ($\text{in} + \text{out}$) | 🟩 FIXED |
| C-03 (R1) | `retrieval.rs` | Preserves dense cosine score alongside RRF rank fusion | 🟩 FIXED |
| C-04 | `tools.rs` | Model ID parity across text and vector pathways | 🟩 FIXED |
| C-05 (R4) | `retrieval.rs` | `ProvenancePair` abstention requires matching `assertion_key` | 🟩 FIXED |
| C-06 (R3) | `retrieval.rs` & `config.rs` | Pool size configurable via `retrieval_seed_pool_size` (default: 20) | 🟩 FIXED |

---

## 🟠 HIGH (12 Fixed, 1 Deferred, 1 Retracted)

| ID | Location | Summary / Fix | Status |
|---|---|---|---|
| H-01 | `node_store.rs` | Explicit rejection of embedding dimension mismatch on insert | 🟩 FIXED |
| H-02 (R5) | `engine.rs` | Action & outcome text indexed into BM25 on birth and WAL replay | 🟩 FIXED |
| H-03 | `retrieval.rs` | Wired `thompson_draw` (weight: 0.10) with prior guard ($\alpha>1 \lor \beta>1$) | 🟩 FIXED |
| H-04 | `dense_core.rs` | Replaced destructive anti-associative $w=0$ edge removal with no-op | 🟩 FIXED |
| H-05 | `salience.rs` | Corrected Welford sample variance to unbiased $n-1$ denominator | 🟩 FIXED |
| H-06 (R2) | `config.rs` | `retrieval_authority_weight` tuned to 0.75 (balanced with cosine 0.70) | 🟩 FIXED |
| H-07 | `engine.rs` | Option (b): Random 128-bit ID for unkeyed feedback; WAL durability + no seq collisions | 🟩 FIXED |
| H-08 | `engine.rs` | Error logging on WAL append; restored full payload buffering | 🟩 FIXED |
| H-09 | `tools.rs` | Gate evaluation uses clamped/authenticated authority (3 sites) | 🟩 FIXED |
| H-10 | `bm25.rs` | Added reverse term index `node_terms` for $O(\text{terms/doc})$ removal | 🟩 FIXED |
| H-12 | `engine.rs` | Dedup preserves corrections, updates authority monotonically, guards assertions | 🟩 FIXED |
| H2 | `runtime.rs` | Gate inspection calls `ensure_loaded` so sleeping contexts aren't 100% novel | 🟩 FIXED |
| H-11 | `engine.rs` | WAL runtime rotation (requires 2-phase commit checkpointing) | ⏭ DEFERRED |
| H-13 | `tools.rs` | Gate TOCTOU (verified continuous mutex guard) | ❌ RETRACTED |

---

## 🟡 MEDIUM (10 Fixed, 1 Deferred)

| ID | Location | Summary / Fix | Status |
|---|---|---|---|
| M-01 | `text_embed.rs` | Removed `"file"` and `"path"` from stopwords; eliminated duplicate entries | 🟩 FIXED |
| M-02 | `tools.rs` | Manifest parser section-aware (`[package]`, `[project]`, `[tool.poetry]`) | 🟩 FIXED |
| M-03 | `engine.rs` | Bounded `feedback_seen` with 100k cap to prevent RAM growth | 🟩 FIXED |
| M-04 | `episode_log.rs` | Bounded `EpisodeLog` ring-buffer eviction at 1,000 entries | 🟩 FIXED |
| M-05 | `core_state.rs` | Documented dead `global_ema` array; preserved bincode positional alignment | 🟩 FIXED |
| M-06 | `retrieval.rs` | Removed dead `SupersessionConflict` check after Step 2a filter | 🟩 FIXED |
| M-07 | `tools.rs` | Clamped gate `novelty` to $[0.0, 1.0]$ across all 3 sites | 🟩 FIXED |
| M-08 | `node.rs` | Renamed dead `salience()` method to `salience_value()` | 🟩 FIXED |
| M-10 | `embed.rs` | Added Mutex poison recovery (`into_inner()`) to `OnnxEmbedder` | 🟩 FIXED |
| R6 | `tools.rs` & `schema.rs` | Added optional `query_text` to vector retrieval params | 🟩 FIXED |
| M-09 | `core_state.rs` | FNV-1a to XXH3 upgrade (requires `xxhash-rust` dependency) | ⏭ DEFERRED |

---

## 🟢 LOW / NIT (6 Fixed, 2 Workspace-Preset)

| ID | Location | Summary / Fix | Status |
|---|---|---|---|
| L-01 | `edge.rs` | Removed unused variable `x` in Marsaglia-Tsang generator | 🟩 FIXED |
| L-03 | `consolidation.rs` | Added conservative 0.25 variance fallback to Newcombe CI when $n_{\text{ctrl}}=0$ | 🟩 FIXED |
| L-04 | `engine.rs` | Annotated `try_migrate_v1_snapshot` with `#[allow(dead_code)]` | 🟩 FIXED |
| L-05 | `bm25.rs` | Annotated `PostingList::tf` with `#[allow(dead_code)]` | 🟩 FIXED |
| L-06 | `retrieval.rs` | Removed dead `superseded_by` field from internal `PoolEntry` struct | 🟩 FIXED |
| L-07 | `tests/` | Applied scoped warning allowances to 5 test fixtures | 🟩 FIXED |
| L-02 | `Cargo.toml` | `libm` already present as workspace dependency | ✅ PRESET |
| L-08 | `Cargo.toml` | `thiserror` already present as workspace dependency | ✅ PRESET |

---

## Summary Counts & Reconciliation

| Category | Count | Items |
|---|---|---|
| 🟩 **Fixed** | **34** | C-01..C-06 (6), H-01..H-10, H-12, H2 (12), M-01..M-04, M-06/E6f, M-07..M-08, M-10, R6 (10), L-01, L-03..L-07 (6) |
| ⏭ **Deferred** | **2** | H-11 (runtime WAL rotation), M-09 (FNV to XXH3 upgrade) |
| ❌ **Retracted** | **1** | H-13 (gate TOCTOU non-issue under continuous mutex) |
| ✅ **Preset** | **2** | L-02 (`libm`), L-08 (`thiserror`) — already present in Cargo.toml |
| 🔄 **Reverted / Stable** | **1** | M-05 (`global_ema` kept in serde layout to preserve bincode positional alignment) |
| **Total Tracked** | **40** | **34 Fixed + 2 Deferred + 1 Retracted + 2 Preset + 1 Reverted/Stable = 40** |

> Counts above are the `cf10d19` baseline. Post-baseline work is tracked in the session section below.

---

## 🧪 Session 2026-10-11 — Recall v1 seed-selection & measurements (post-baseline)

| ID | Location | Summary / Fix | Status |
|---|---|---|---|
| S-01 | `retrieval.rs` (locked, reviewer-approved) | Contradicted nodes remain in diagnostics/conflict pool but are structurally ineligible as seeds; relevance-only seed ordering (query cos + dense cos + bandit 0.10); empty eligible set returns explicit abstention, never an index panic | 🟩 SHIPPED |
| S-02 | `config.rs` | `retrieval_authority_weight` kept readable for compatibility, deprecated, removed from seed ordering; docs corrected to drop the false claim | 🟩 SHIPPED |
| S-03 | `tests/retrieval_contract.rs` | `irrelevant_high_authority_node_cannot_displace_relevance_winner` + `all_contradicted_candidates_return_abstained_empty_result`; `test_contradictory_same_role_resolution` rewritten to the intended safety contract | 🟩 SHIPPED |
| S-04 | `persistence.rs` (tests) | `tmp_dir` stamps counter + thread id — WAL crc-flip test no longer polluted by stale fixed-name dirs | 🟩 FIXED |
| S-05 | `engine.rs` | WAL replay not idempotent: node count 46→77 across restart (BGE 82→104). Blocks standby/hibernation/migration | 🔴 OPEN |
| S-06 | measurements | Seed amendment Session L before/after: seed/pool/chain/MRR bit-identical in all 4 groups — L1 is authority-insensitive. Superseded as the deciding instrument by S-08: the approval corpus (60-distractor scenarios) proved the ordering rule catastrophic there | 📐 MEASURED (see S-08) |
| S-07 | `tests/loadbench.rs` | 10k sweep: p50 32.8ms / p99 42.2ms, 5.0KB/node, 44 learns/s; brute force fails the p50≤100ms gate well before 100k nodes | 📐 MEASURED |
| S-08 | `retrieval.rs` + `config.rs` (locked, owner-approved deviation) | Bounded authority term restored at **α=0.25**: relevance-only ordering (S-01) collapsed the approval gate (G1 42/43→0/43, MRR 0.988→0.308 — 60-distractor pools let cosine noise beat the positive). Measured sweep: 0.15→41/43, 0.25→42/43 (= baseline), 0.35→43/43, ≥0.5 fails the irrelevant-trusted property. Contradicted-ineligibility stays structural (G4 0/43 at every weight) | 🟩 SHIPPED |
| S-09 | `tests/` | Contract suite `test_config` pin 0.8→0.25; phase3 gap assertion back to `>0.10` (gap = 0.25×0.65 ≈ 0.16); e7 fail-safe 1 rewrite retained (relevance winner wins at α=0.25) | 🟩 SHIPPED |
| S-10 | verification process | Suite-green checks used case-insensitive `Select-String "FAILED"` + `-First 6`, which matched `0 failed` lines and truncated real failures — three latent failures (phase3_gap, e7, approval bench) hid behind it. All suite greps must be case-sensitive on `test result: FAILED` and unbounded | 🟩 PROCESS FIXED |
