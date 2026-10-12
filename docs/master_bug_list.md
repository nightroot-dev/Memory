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
| S-05 | `engine.rs` (+`core_state`/`migration`/`persistence`/`learn_ledger`, `tests/s05_restart.rs`) | WAL replay not idempotent (was: 46→77 nodes across restart). Fixed at lifecycle, not dedup: snapshot-side `last_applied_wal_seq` watermark (v5; v2/v3/v4 migrate with watermark 0) + replay skips applied prefix (truncated-WAL guard replays all — safe direction) + ledger skip-before-append gated on `is_replay` + `birth_node→(id,birthed)`. Shutdown retains WAL (an attempted delete broke E7 quarantine recovery — caught by test, reverted, retention required); startup compaction bounds the WAL. Verified: s05 3/3 (30/10/11 exact incl. crash path; low-surprise control flat), T8 unmodified green, T6 green, approval G1/G3/G4 baseline-identical, MiniLM seed/pool/MRR bit-identical, full suite 50/50 | 🟩 SHIPPED |
| S-06 | measurements | Seed amendment Session L before/after: seed/pool/chain/MRR bit-identical in all 4 groups — L1 is authority-insensitive. Superseded as the deciding instrument by S-08: the approval corpus (60-distractor scenarios) proved the ordering rule catastrophic there | 📐 MEASURED (see S-08) |
| S-07 | `tests/loadbench.rs` | 10k sweep: p50 32.8ms / p99 42.2ms, 5.0KB/node, 44 learns/s; brute force fails the p50≤100ms gate well before 100k nodes | 📐 MEASURED |
| S-08 | `retrieval.rs` + `config.rs` (locked, owner-approved deviation) | Bounded authority term restored at **α=0.25**: relevance-only ordering (S-01) collapsed the approval gate (G1 42/43→0/43, MRR 0.988→0.308 — 60-distractor pools let cosine noise beat the positive). Measured sweep: 0.15→41/43, 0.25→42/43 (= baseline), 0.35→43/43, ≥0.5 fails the irrelevant-trusted property. Contradicted-ineligibility stays structural (G4 0/43 at every weight) | 🟩 SHIPPED |
| S-09 | `tests/` | Contract suite `test_config` pin 0.8→0.25; phase3 gap assertion back to `>0.10` (gap = 0.25×0.65 ≈ 0.16); e7 fail-safe 1 restored to the pre-amendment direction with node authority 0.85→1.00 | 🟩 SHIPPED |
| S-11 | `tests/retrieval_e2e.rs` | e7 fail-safe 1 sat 0.0015 from its decision boundary at α=0.25 (0.089 relevance edge vs 0.25×0.35 authority edge; crossover α≈0.254) — passed standalone by luck, flaked in the workspace run. Fixed by raising the authoritative node's authority to 1.00 (margin 0.036) + 4 consecutive green runs. **Lesson: when a weight lands near a test's decision boundary, the margin must be computed, not assumed** | 🟩 FIXED |
| S-10 | verification process | Suite-green checks used case-insensitive `Select-String "FAILED"` + `-First 6`, which matched `0 failed` lines and truncated real failures — three latent failures (phase3_gap, e7, approval bench) hid behind it. **Second defect: `cargo test --workspace` stops at the first failing binary**, so every earlier "full workspace" run only verified binaries up to the first failure — e1p (cmhm_mcp tool_tests) never ran at all. All suite checks must be case-sensitive on `test result: FAILED`, unbounded, and use `--no-fail-fast` | 🟩 PROCESS FIXED |
| S-12 | `cmhm_mcp/tests/tool_tests.rs` | `e1p` asserted a contradicted node IS the seed (pre-amendment contract); now abstains with no seed. 0.30 cap still unit-verified in `authority.rs::clamp_authority`. Found only after `--no-fail-fast` exposed the binaries that had never run | 🟩 SHIPPED |
| S-13 | `retrieval.rs` (locked, note-approved) + `tools.rs` | B1: retrieval returns stored text. `format_explanation` finally uses the node store it was already passed; `ChainStep` gains `from_text`/`to_text` (`Option<ChainNodeText{text, timestamp}>`), `RetrievalResult` gains `seed_text`. Explanation, MCP human block and JSON all carry text; 120-char cap with visible `…[+N chars]` marker in human halves, lossless in JSON; textless nodes render "no text stored". All JSON additions additive (`causal_chain_len` kept) | 🟩 SHIPPED |
| S-14 | `cmhm_mcp` (no locked files) | B3: `context_route` gains `min_score` (0.5) + explicit `no_match`; `project_detect` idempotent with normalized project identity (Windows-aware) returning a usable `context_id`. B4: instructions 3441→1321 chars, tiered triggers, moved to lib as testable `CMHM_INSTRUCTIONS`. B5: raw-embedding tools hidden unless `CMHM_EXPOSE_EMBEDDING_TOOLS` explicitly enables; unknown values are errors. One-commit exception (B3+B4 atomic) recorded in the commit message | 🟩 SHIPPED |
| S-15 | design note (owner-written, reviewer-approved) | B1b: return the full learned triple + top-3 candidate texts; ledger option A recommended; edge-walking rejected (verified: `Node` has no episode link, only action→outcome is guaranteed per learn at `engine.rs:1624`, state→action needs cosine > 0.35). Locked-file grant: `retrieval.rs` for `candidate_texts`/episode resolution — read-only join, no scoring change. Latency tolerance: p50 regression ≤15% vs pre-B1b at same N, ship gates unchanged | 🟩 GO with riders: (1) ledger cap 100k oldest-first; evicted entries show "episode record evicted" (distinct from pre-B1b "no triple recorded"); eviction clears the node→episode index, index rebuilt on load (never persisted) and counted in the memory budget, with a test. (2) Five-field `LearnEpisode`, no text duplication. (3) "(+N other episodes…)" marker agreed. R1 next immediately after B1b; no release between (replay duplicates ledger entries via open S-05) |
| S-16 | shipped: B1b (`19ad4c7`) + B1c (`b554e95`) | B1b: LearnLedger (100k cap, oldest-first, distinct evicted/pre-B1b markers, transient index + budget accounting) + snapshot v3→v4 migration + read-only retrieval join + triple/top-3 rendering. B1c: std-only date helper, "first recorded" label, untrusted-data marking in block + both retrieve descriptions (TOOLS table is the real description source — schema docs alone don't reach tools/list). Gates: approval bench APPROVED post-B1b (G1 42/43, MRR 0.988, G4 0/43); Session L 3+3 seed/pool/MRR bit-identical, chain means inside pre ranges; loadbench 10k p50 +4–7% / p99 +10–11% vs pre, ship gates pass at ~1/3 budget | 🟩 SHIPPED |
| S-17 | `cmhm_gateway/src/audit.rs` (real bug, found blocking the suite) | `ts_secs: f64` hashed raw float bits, but serde_json parses "1791781224.7492247" to different bits than it serialized (proven failing round-trip) → ~20% intermittent "entry N hash mismatch" on files that hand-verify fine. Every other theory died with evidence (no second writer, TempDir didn't help, Python round-trips exact). Fix: integer `ts_millis` in the hashed path + TempDir test isolation. 0/30 fails after (was ~20%). Pre-fix local logs won't verify — documented: they were already probabilistically broken. **FORMAT BREAK (owner-recorded): float-`ts_secs` audit.log files do not verify under integer-`ts_millis`; no migration is provided (v0.2.0 pre-release, local operational files only)** | 🟩 FIXED |
| S-19 | B7 scope (owner live-run finding, 2026-10-12) | Visible chains render cross-experience links with untrained priors (0.50/evidence 2.0) as causal claims; the correct triple sits below but an agent can read the chain as causation. **Rider (owner-accepted): predicate is `evidence <= 2.0 + eps`, never `== 2.0`. Verified in code: temporal forgetting decays α/β toward a 0.01 floor (`engine.rs:1718-1719`), NOT the 1.0 prior — an aged never-reinforced edge drifts from 2.0 toward 0.02, so exact equality would miss it. Required tests: aged-unreinforced edge still counts prior-only (hidden); freshly-reinforced edge is shown (eps validated by tests, not fixed by fiat)** | 🟩 SHIPPED as refined direction below |
| S-21 | B7 (`0dd528e` core + `526a85d` surface, built on `wip/b7`, merged to master) | Refined direction (owner): hide CROSS-EXPERIENCE links via ledger membership, not prior-only links — first attempt (strict evidence predicate) hid every fresh Beta(1,1) edge and broke B1 (pre-B7 B1 12/12 stable proves it). `LinkKind::{SameExperience, CrossExperienceUnverified, CrossExperienceLearned, Unknown}` + pure `classify_link`; hide iff Unverified with visible count marker; triple renders first; chain labeled "related observations, not verified causation"; learned→"learned association", unknown→"unverified" (unknown = visible, never silent loss); JSON keeps every step + link_kind + verified_causation. S-15 grant covers endpoint episode resolution (same read-only join site/pattern, no scoring change). Gates: b7 5/5 ×5 + B1 3/3 ×5 stable, tool_tests 70/70 ×5, e2e 33, s05 3/3, b1b 9/9, approval APPROVED (G1/G3/G4 identical), Session L hash seed/pool/MRR bit-identical, full suite 51/51 | 🟩 SHIPPED |
| S-20 | `cmhm_core/tests/b1b_ledger.rs` | T8 (`b1b_t8_snapshot_restart_retrieves_triple`): learn → shutdown (v4 snapshot) → restart on same dir → retrieve asserts Found + verbatim triple. Closes the owner-flagged gap (migration covered ledgerless loads; nothing proved a retained triple survives restart). Asserts status + texts only — robust to S-05 replay duplication by design | 🟩 SHIPPED |
| S-22 | B2 (`60057d1`, no locked files) | `memory_list` (id-ordered pages, limit 50/cap 500, offset) + `memory_get` (explicit error on unknown id) dump verbatim records (id/type/text/ts/authority, never embeddings). Read-only end to end: NodeRecord + 2 Command arms + handle/runtime getters + MCP handlers/TOOLS/dispatch. Tests: 3 core + 1 MCP (incl. pagination + unknown-id). Full suite 52/52 | 🟩 SHIPPED |
| S-23 | B6a (`c4aa916`, approved design note, no locked files) | Visible learn disposition (`Birthed \| DedupedTo(id)` in LearnResult; both MCP learn responses render it + additive JSON; enforced-suppression reports `gated`) + opt-in `strict_no_dup` (both learn tools; threads to birth_node; dedup arm skips H-12 corrections while edges/ledger/ids proceed; NOT WAL-persisted — documented deviation, R1 watermark shrinks the window). 53 test/example call sites take the trailing default (scripted, compiler-verified). Tests: 3 core (incl. strict-keeps vs loose-raises control) + 2 MCP. Full suite 53/53 | 🟩 SHIPPED |
| S-18 | verification process | After file-restore operations (Copy-Item), cargo fingerprints can go stale and run old binaries while reporting success — always confirm "Compiling" lines appear after a restore. Second instance of the class from S-10: green claims need a freshness check, not just a result grep | 🟩 PROCESS FIXED |
