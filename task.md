# CMHM Project State & Backlog Tracking

**Document Version:** 1.0  
**Last Updated:** 2026-09-15  
**Governing Protocol:** Phase 3 Approval Protocol v2.0 (`FE72B68C...F4DE`)  
**Production Stack:** `hash-128d` (`EMBED_DIM = 128`, `TEXT_MODEL_ID = "hash-128d"`)

---

## 1. Project Phase & Governance Status

- **Phase 3 Approval Benchmark:** **CLOSED & CO-SIGNED** (Attempt 4, Corpus v4 `A3DFFF9B...EECD3`).
- **Phase 3 Retrieval & Authority Invariants:** Locked. Zero unreviewed edits to `cmhm_core/src/retrieval.rs` and `cmhm_core/src/authority.rs`. All 318+ tests in workspace passing.
- **Phase 4 Pre-requisite State:** ONNX embedder cost and benchmark analysis completed; adoption gated on §4.1 resolution.

---

## 2. Ingestion Gate & Epistemic Admission Policy (Option A)

The system formally operates under **Option A (Fail-Closed Graph Boundary)**:

$$\text{gate\_score} = \text{novelty} \times (1.0 + \kappa \cdot |\text{prediction\_error}|) \times \text{verifiability} \ge \text{GATE\_THRESHOLD} \quad (0.35)$$

### Epistemic Invariants
1. **Fail-Closed Boundary:** Low-trust, uncorroborated claims must not pollute active graph topology. Downstream retrieval authority scaling acts as a secondary defense, but unverified claims are quarantined pre-graph.
2. **Zero Data Loss Guarantee:** Every `learn` attempt is appended immutably to Tier 0 (`learn_log.jsonl`) *before* the gate runs, preserving full raw event context (`state`, `action`, `outcome`, `prediction_error`, `emotional_tag`, `tags`).
3. **Fail-Safe Shadow Default:** `enforce_gate = false` remains locked by default. Gate decisions are evaluated and recorded to `gate_shadow_log.jsonl` (Schema v=2) without dropping graph insertions until live calibration analysis authorizes enforcement.

### Governing Boundary Constants (at $\tau = 0.35, \kappa = 2.0$)
- **Hard Rejection Floor ($\text{authority} \le 0.116$):** An uncorroborated virgin memory ($N = 0$) from a caller with $\text{authority} \le 0.116$ cannot clear the gate even at maximal novelty ($1.0$) and maximal surprise ($|\text{pe}| = 1.0$).
- **Routine Novelty Floor ($\text{authority} < 0.292$):** A virgin memory with routine surprise ($|\text{pe}| = 0.10$) requires $\text{authority} \ge 0.292$ to clear threshold at maximal novelty.
- **Corroboration Escape Hatch:** For observations near established nodes ($N \gg 1$), sample-size shrinkage ($w = \frac{N}{N + 4.0}$) blends authority with the empirical Wilson Lower Confidence Bound (95% CI):
  $$\text{verifiability} = (1 - w) \cdot \text{authority} + w \cdot \text{LCB}$$
  This permits lower-trust callers to contribute into verified topic neighborhoods.

---

## 3. Active Backlog & Milestone Tracking

### [COMPLETE] Item 1: Tier-0 Promotion Mechanism (`memory_promote`)
- **Status:** COMPLETE (implemented and verified).
- **Objective:** Provide a formal promotion pathway from Tier 0 (`learn_log.jsonl`) back into the active graph, preventing permanent dead-ends for quarantined observations.
- **Architecture Implementation (Option 1):**
  - `memory_promote(event_id, authority, caller_role, context_id, auto_tag)` tool in `cmhm_mcp`.
  - **Drift Audit Record:** `promotion_log.jsonl` records `promotion_id`, `event_id`, original event context, granted authority, and promotion-time `GateInspectionResult` (novelty, verifiability, gate_score, max_cosine).
  - **Epistemic Trust Escalation:** Indexed with reviewer-authorized `provenance: "user_explicit"`, `supersedes: null`, and tags `["promoted_from_archive", "archive_event:<id>"]`, preserving Phase 3 protocol freeze while maintaining complete origin transparency.

### [OPEN / GATED] Item 2: §4.1 Cohort Dimensionality in Consolidation
- **Status:** OPEN (deliberate deferral).
- **Scope:** Ensure `Cohorts` in `cmhm_core/src/consolidation.rs` and `InspectGate` in `cmhm_core/src/engine.rs` dynamically re-align or project centroids during model transitions without Euclidean distance distortion.
- **Invariance:** Inert under current `hash-128d` baseline. **Strictly blocks any activation of ONNX 384d embeddings (`MiniLM`) or multi-model runtime switches.**

### [PENDING ACCUMULATION] Item 3: Operational Shadow Log Calibration
- **Status:** Tool verified ([`tools/analyze_gate_shadow.py`](tools/analyze_gate_shadow.py)). Awaiting operational log accumulation.
- **Scope:** Ingest dual-schema logs (v1 at $\tau=1.0$, v2 at $\tau=0.35$), evaluate empirical distributions, and verify threshold sensitivity before authorizing `enforce_gate = true`.

---

## 4. Integrity & Continuity Ledger

| Component | Identifier / Hash | Status |
|---|---|---|
| Protocol v2.0 | `FE72B68C...F4DE` | GOVERNING |
| Corpus v4 Benchmark | `A3DFFF9B...EECD3` | VERIFIED |
| Core Engine (`retrieval.rs`) | `BE6CE9DA...97CA` | LOCKED |
| Core Authority (`authority.rs`) | `A820C779...85C16` | LOCKED |
| MCP & Gate Baseline | Commit `36d93cf` | OPERATIONAL |
| Shadow Log Analyzer | `tools/analyze_gate_shadow.py` | VERIFIED |
| Approval Bench Harness (`cmhm_mcp/tests/phase3_approval_bench.rs`) | `19C50907...90A90C` | MODIFIED — new identity from commit `07b8573`. Reason: `retrieve()` signature now takes `query_text + bm25`; the 2 bench call sites pass `""` (fusion skipped, behavior identical). Corpus v4 data hash `A3DFFF9B...EECD3` UNCHANGED (scenario text untouched). |

Scoping note: BM25 indexes state text only. Action/outcome text is in WAL V4
but not in the lexical index. Queries phrased as "how did I fix X" will not
benefit from BM25 until this is extended. Extension requires its own measurement.

## 5. Known Baselines (measured 2026-09, corpus `bm25_fusion_bench`, n=80)

| Metric | Value | Notes |
|---|---|---|
| Hash-128 paraphrase P@1 | 5.0% | Honest baseline; replaces earlier ~6.7% estimate |
| Hash-128 lexical P@1 | 60.0% | Boilerplate-inflated (shared template phrasing) |
| Hash-128 + BM25 overall P@1 | 48.8% | +16.3 pts over dense-only |
| Hash-128 + BM25 lexical P@1 | 92.5% | +32.5 pts; P@5 at 100% |
| Hash-128 + BM25 paraphrase P@1 | 5.0% | Flat — BM25 adds nothing without token overlap (control) |

The paraphrase ceiling is set by the dense embedder. Next lever is
fixing the MiniLM truncation sites in `engine.rs` / `consolidation.rs` /
`dense_core.rs`. Est. +20-25 pts overall P@1 after fix.

## Truncation fix complete (2026-09)
All 11 silent 384->128 truncation sites eliminated:
  engine.rs: birth_node, trigger_edge_birth, update_causal_edge x2,
             process_explicit_edge x2
  consolidation.rs: credit-propagation x1, dead-edge prune x1
Behavior: fail-closed. On missing projection: warn + skip dense update.
Storage layer (nodes, edges, WAL) unaffected.

Two follow-up findings from this session:
  1. No projection registration infrastructure exists. MiniLM embeddings
     currently fail closed at learn time. Positive-path MiniLM requires
     either Command::RegisterProjection or startup loading from
     cfg.projections_dir (which is created but never read — engine.rs:582).
  2. process_learn stringifies ProjectionError via map_err, losing the
     typed error. Callers cannot programmatically match on NoProjectionFound
     vs DimMismatch. Fix: replace the three map_err sites with direct ?
     propagation.

Test coverage: projection_failclosed.rs (2 tests) + projection.rs
(3 contract tests) lock the fail-closed behavior end-to-end.

## Projection registration landed (2026-09)

- Command::RegisterProjection + EngineHandle::register_projection
  (commit 974ef21)
- Positive test: registered 384->128 projection → learn succeeds,
  nodes created (projection_positive.rs)
- Negative test: matrix-length mismatch → Err

Resolves both follow-ups logged under "Truncation fix complete":
  - Finding 2 (typed errors): process_learn propagates ProjectionError
    via ?, callers can downcast — commit f266954.
  - Finding 1 (registration infra): runtime registration now exists —
    commit 974ef21.

Remaining to enable MiniLM end-to-end:
  1. Train real 384 -> 128 projection for "minilm-l6-v2-int8"
     (Python, offline, ~1 day).
  2. Expose register_projection via MCP tool
     (memory_register_projection) + ContextRuntime passthrough.
  3. Optionally wire startup loading from cfg.projections_dir
     (currently created at engine.rs:582 but never read).

Toolchain note: cargo +stable-x86_64-pc-windows-msvc hits a link error
(rust-lld cannot find system/mingw libs — suspected gnu-target pin in
.cargo/config.toml). Sessions currently use +stable-x86_64-pc-windows-gnu
which links cleanly. Investigate the config pin in a future session.

## Projection auto-load landed (2026-09)

- engine::start() loads all *.bin from cfg.projections_dir before
  WAL replay; registers each by filename stem as model_id.
- Trained MiniLM projection at data/projections/minilm-l6-v2-int8.bin
  (Session D: corpus 17946, variance retained 0.8346, 197124 bytes).
- Closes the "created but never read" bug at engine.rs:628.

MiniLM is now end-to-end wired:
  1. Trained 384->128 projection exists on disk.
  2. Engine auto-loads it on start.
  3. 384-dim learn and retrieve succeed with model_id="minilm-l6-v2-int8".

To enable MiniLM in production:
  - Build with --features onnx so the embedder produces 384-dim vectors.
  - Ensure cfg.projections_dir contains minilm-l6-v2-int8.bin.
  - Learn/retrieve with model_id="minilm-l6-v2-int8".

## MiniLM benchmark (2026-09)

Measured on n=80 (40 lexical, 40 paraphrase) via
cmhm_core/tests/minilm_bench.rs:

| Embedder   | Lex P@1 | Para P@1 | Overall P@1 | Overall MRR |
|------------|---------|----------|-------------|-------------|
| hash-128d  | 0.0750  | 0.2000   | 0.1375      | 0.3094      |
| MiniLM     | 1.0000  | 0.1500   | 0.5750      | 0.5942      |

Caveats — read before citing these numbers:

1. LEXICAL HALF HAS A BENCHMARK DESIGN FLAW. All 40 lexical
   situations share the boilerplate "system experienced X failure
   during phase N", so 5 content tokens are identical across all of
   them. BM25 (which the hash path relies on) cannot discriminate
   within this cluster. The hash-128d Lex P@1 = 0.075 is a
   measurement of this flaw, not of hash-128d's lexical ability.
   Session B's bm25_fusion_bench measured 0.925 Lex P@1 with
   properly distinct situations — that number is the true hash+BM25
   lexical ceiling.

2. PARAPHRASE HALF SHOWS NO MINILM ADVANTAGE ON THIS CORPUS.
   MiniLM Para P@1 = 0.15 vs hash Para P@1 = 0.20. On n=40 this is
   a 2-pair difference — statistically noisy but clearly not a
   paraphrase win. This contradicts the single-scenario result in
   minilm_e2e.rs (where the target ranked #1) and indicates that
   the trained PCA projection may not generalize beyond the
   mini_e2e scenario.

3. Overall P@1 = 0.5750 for MiniLM vs 0.1375 for hash is real but
   dominated by the pathological lexical half. Do not cite as
   "MiniLM is 4x better."

Action items (future sessions):
  - Rebuild lexical corpus with truly distinct situations (zero
    content-token overlap between any two situations).
  - Investigate the paraphrase gap: is the projection undertrained,
    is the corpus too narrow, or is MiniLM genuinely limited here?
  - Consider training the PCA projection on a broader domain that
    matches the benchmark's ops scenarios.

Also note: tools/setup_ort.ps1 re-downloads ORT even when a valid
copy is on disk — its version-string scan misses the existing DLL.
Non-blocking; fix in a future housekeeping session.
