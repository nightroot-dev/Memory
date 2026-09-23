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
