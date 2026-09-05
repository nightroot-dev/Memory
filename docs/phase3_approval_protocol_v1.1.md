# Phase 3 Approval Protocol v1.1 — Living Field Architecture

**Version:** 1.1  
**Supersedes:** v1.0 (`docs/phase3_approval_protocol.md`, commit `8bc2017`)  
**Status:** SUBMITTED FOR REVIEW — awaiting explicit user approval before corpus creation or execution  
**Implementation Source Revision:** `dacb140` (HEAD at protocol submission)

> **MANDATORY CONSTRAINT:** No corpus file may be created, opened, or inspected until the user
> explicitly approves this document. The verifier and source revision are already frozen.
> Any modification to a frozen artifact after corpus creation invalidates that corpus.

---

## Resolution of Blocker Issues (v1.0 → v1.1)

| Blocker | Resolution |
|---|---|
| B1: Stated path not externally reachable | `handle_retrieve_text` now supports absent `retrieval_intent` → `seed_type = None` (unscoped). Committed `126829a`. |
| B2: Frozen revision contains compromised corpus | Acknowledged. Corpus author isolation and overlap audit requirements added in Section 3.6. |
| B3: Pool terminology inconsistent | Four pool stages defined with distinct names. Every gate references a specific named stage. Section 4.4 rewritten. |
| B4: CI direction wrong for upper-bound gates | Corrected: G4/G5 use UPPER CI bound (must be < threshold). G6/G1/G2/G3/G8/G9 use LOWER CI bound. Section 5.2 fixed. |
| B5: G8 eligibility undefined | Predeclared: G8 uses end-to-end denominator (all CONTRADICTION scenarios). Strict supporting gate S1 added. |
| B6: Abstention conflated with retrieval failure | Four selection outcome rates added (Section 5.4). Retrieval gates preserved unchanged. |
| B7: Verifier not frozen before authorization | Verifier committed at `dacb140` and hashed before this protocol submission. Synthetic self-tests pass (9/9). |
| B8: Hash/freeze metadata inconsistent | All six source files and the verifier hashed. Table corrected with implementation commit ID. engine.rs added. |

Open question answers incorporated:

- OQ-1: `abstain_on_conflict = true` for all approval scenarios.
- OQ-2: MRR uses full composite-scored candidate list; absent D+ contributes 0 (no rank cap).
- OQ-3: G8 and G9 are independent. G8 = LogicalContradiction classified. G9 = abstained=true.
- OQ-4: Minimum corpus size = 40 queries with predeclared per-class counts.

---

## 1. Governing Constraints (Non-Negotiable)

1. Never remove, weaken, or reinterpret an acceptance gate after observing a failure.
2. Test actual end-to-end operational retrieval risk, not pairwise cosine similarity alone.
3. Pure-paraphrase Recall@K is not contradiction-safety evidence unless realistic hard negatives are in the candidate pool.
4. Do not assume Beta-bandit feedback solves contradiction retrieval.
5. Keep `TEXT_MODEL_ID` unchanged (`"hash-128d"`).
6. Do not use MMR, increased retrieval count, accidental dense-core bias, or presumed role scoping as contradiction-safety fixes.
7. `retrieval_intent` is caller-supplied metadata, not authenticated truth. Contradiction safety must hold without trusting it.
8. The prior 15-query rejection set (`dprime_bench.rs`) and the 32-query compromised corpus (`phase3_held_out_bench.rs`) are **rejection/diagnostic evidence only**. They may not be reused, paraphrased, or used for tuning.
9. No corpus creation, inspection, or execution until Phase 2 is independently closed and the user explicitly approves this protocol.
10. Native ONNX/INT8 cost testing is locked until Phase 3 approval gates pass.

---

## 2. Externally Reachable Retrieval Contract (Blocker 1 Resolution)

### 2.1 Unscoped Mode — New Production Path (commit `126829a`)

`handle_retrieve_text` in `cmhm_mcp/src/tools.rs:838` now supports two modes:

| `retrieval_intent` in JSON | Behavior | `seed_type` | Pool |
|---|---|---|---|
| Absent or null | **Unscoped retrieval** | `None` | All NodeType variants compete |
| `"state"` / `"action"` / `"outcome"` | Scoped retrieval | `Some(t)` | Only nodes of type `t` |
| Any other non-null string | Error (fail closed) | — | No retrieval |

**The approval benchmark must call `handle_retrieve_text` with `retrieval_intent` absent.**
This routes through `rt.retrieve_unscoped()` → `EngineHandle::retrieve_unscoped()` → `Command::Retrieve { seed_type: None }` → `node_store.nearest(q, model_id, 8)`.

### 2.2 Scoped Mode (Existing Behavior — Secondary Diagnostic Only)

Scoped results (`retrieval_intent` present and parseable) may be reported as secondary diagnostics. They do not determine approval.

### 2.3 Production Path Trace (Frozen at `dacb140`)

```
handle_retrieve_text(args: JSON)                     [cmhm_mcp/src/tools.rs:838]
  → serde_json::from_value(args) → RetrieveTextParams
      .query: String                    (required)
      .context_id: Option<String>       (optional)
      .retrieval_intent: Option<String> (optional — NEW, was required in v1.0)
  → text_to_embedding(&params.query)    [hash-128d, FNV-1a, 128-dim, L2-normalised]
  → match params.retrieval_intent:
      None    → seed_type: Option<NodeType> = None
      Some(s) → NodeType::from_str(s) → Some(t) or fail-closed
  → match seed_type:
      None    → rt.retrieve_unscoped(ctx_id, emb, "hash-128d")
      Some(t) → rt.retrieve(ctx_id, emb, "hash-128d", t)
  → ContextRuntime::retrieve_unscoped()              [context/runtime.rs]
  → EngineHandle::retrieve_unscoped()                [engine.rs]
  → Command::Retrieve { seed_type: None, ... }
  → retrieval::retrieve(query, model_id, node_store, ..., seed_type=None, ...)
      → node_store.nearest(query, model_id, 8)       [unscoped — ALL node types]
      → supersession filter (Step 2a)
      → composite scoring (Step 2b)
      → winner selection (Step 2c)
      → conflict detection (Step 2d)
      → abstention (Step 2e, abstain_on_conflict=true for approval)
  → RetrievalResult
```

---

## 3. Corpus Specification

### 3.1 Size and Freshness

- **Minimum:** 40 entirely new queries.
- **Zero reuse:** No query, paraphrase, domain-template substitution, or scenario
  adaptation from:
  - the 15-query `dprime_bench.rs` rejection set, or
  - the 32-query `phase3_held_out_bench.rs` compromised corpus.
- **Per-class counts predeclared:** See Section 3.3. Multi-label scenarios may
  satisfy multiple class minimums only if they include a **dedicated, non-overlapping
  scenario element** for each class they claim. Silent cross-crediting is not permitted.

### 3.2 Domain Distribution

Queries must span at least 8 distinct operational domains, with no single domain
accounting for more than 6 queries:

| Domain class | Required minimum |
|---|---|
| Infrastructure / OS-level configuration | 4 |
| Database configuration and durability | 4 |
| Network, TLS, and reverse-proxy | 4 |
| Container orchestration (Kubernetes / Docker) | 5 |
| Cloud platform and IAM policy | 4 |
| Security controls and cryptography | 5 |
| Distributed systems and consensus | 5 |
| Observability and incident response | 4 |
| Storage and filesystem | 5 |

Total = 40 minimum. No ceiling per domain above 6.

### 3.3 Required Scenario Classes with Predeclared Per-Class Counts

| Class | Min dedicated scenarios | Description |
|---|---|---|
| **CONTRADICTION** | 10 | Same non-empty `assertion_key`, opposing `polarity` (+1 / -1). LogicalContradiction must fire if D+/D- reach top-2 SCORED with gap < threshold. |
| **SUPERSESSION** | 6 | Positive explicitly `supersedes` D-. D- must appear in RAW_NN but be absent from POST_FILTER. SupersessionConflict fires only if D- leaks. |
| **AUTHORITY-POLARITY** | 6 | Positive in `{"corrected","user_explicit"}`, D- has `"contradicted"`. AuthorityPolarity class exercises mid-path trust signal. |
| **ADVERSARIAL-AUTHORITY** | 6 | D- has authority >= 0.80 AND `provenance = "contradicted"`. Validates authority alone cannot override contradicted provenance. |
| **SAFE-ABSTENTION** | 6 | CONTRADICTION scenario with gap < threshold, `abstain_on_conflict = true`. Must produce `abstained = true`, empty CHAIN, non-empty `conflict_candidates`. |
| **UNRESOLVED-CONFLICT** | 6 | Gap < threshold, no semantic incompatibility class matches → `conflict_detected = false`. Validates score proximity alone is insufficient. |

Scenarios may be multi-label (e.g., CONTRADICTION + ADVERSARIAL-AUTHORITY) only if both scenario
elements are fully implemented and independently verifiable. No minimum may be satisfied solely
by claiming a multi-label without dedicated implementation.

### 3.4 Hard Negative Requirements

Every scenario must include at least one hard negative (D-). D- must:

1. Share **high lexical and/or embedding overlap** with the query.
2. Carry **opposite operational meaning** (a direct contradiction, not a different recommendation).
3. Use `model_id = "hash-128d"`.
4. Remain eligible to reach RAW_NN unless legitimately filtered:
   - Supersession (Step 2a) is the only rule that may remove D- before SCORED.
   - Without supersession link, D- must appear in RAW_NN.
5. CONTRADICTION class: `assertion_key` must be identical (non-empty) for D+ and D-; `polarity` = +1 on D+, -1 on D-.

### 3.5 Distractor Background Population

Each scenario's live memory context must contain:

- >= 60 background distractor nodes from domains other than the scenario's assertion domain.
- Same `model_id = "hash-128d"`.
- Inserted **before** D+ and D- to prevent ordering artifacts.
- Fixed RNG seed: `DISTRACTOR_RNG_SEED = 0xA3F72B91C5D4E608u64`.

### 3.6 Corpus Author Isolation and Overlap Audit (Blocker 2 Resolution)

**The compromised 32-query corpus (`phase3_held_out_bench.rs`) exists in the frozen revision.**
To prevent contamination:

1. The corpus author must declare, before writing any query, that they have not read
   the content of `phase3_held_out_bench.rs` or its associated evidence log since the
   corpus was last committed.
2. An **independent overlap audit** must be performed before the corpus is executed:
   - Extract all query strings from the new corpus file.
   - Compare them (fuzzy: Jaccard similarity of word trigrams, threshold < 0.30 considered distinct)
     against all queries in `phase3_held_out_bench.rs` and `dprime_bench.rs`.
   - Record the audit results in the evidence log before the first benchmark run.
   - Any pair with Jaccard similarity >= 0.30 is a contamination error and blocks execution.
3. The audit script (`scripts/audit_corpus_overlap.ps1`) must be committed and hashed before
   the corpus is created.

### 3.7 Excluded Mechanisms

The following are explicitly forbidden as a means to achieve gate passage:

- MMR re-ranking.
- Increasing `k` beyond 8 in `nearest()` / `nearest_by_type()`.
- Dense-core bias tuning.
- Fixing `seed_type` to a single type for all queries (root flaw of compromised corpus).
- Relying on `retrieval_intent` being truthful from an untrusted caller.

---

## 4. Pool Stage Definitions (Blocker 3 Resolution)

### 4.1 Named Stages

| Stage name | Defined as | Gate references |
|---|---|---|
| **RAW_NN** | Direct output of `node_store.nearest(q, model_id, 8)` — all nodes, before any filter | G7 (D- presence check), G10 (contamination check) |
| **POST_FILTER** | RAW_NN candidates after Step 2a: nodes where `n.superseded_by.is_some()` are removed | G7 (D- must be absent here for SUPERSESSION scenarios) |
| **SCORED** | POST_FILTER candidates after Step 2b composite scoring and Step 2c descending sort | G1, G2, G3, G4, G5, G6 (all retrieval metrics) |
| **CHAIN** | `result.causal_chain` — the result returned to the caller; empty when `abstained = true` | Not directly measured by gates (G9 measures abstained flag) |

### 4.2 Per-Scenario Reporting Obligations

The benchmark must emit one `PHASE3_RESULT` line per scenario with these fields:

```
PHASE3_RESULT|id=<str>|raw_nn=<comma-sep-ids>|post_filter=<comma-sep-ids>|
  scored=<comma-sep-ids-in-order>|winner_id=<id>|
  pos_id=<id>|pos_rank=<1-indexed-in-SCORED; 0=absent>|pos_score=<f32>|
  neg_id=<id>|neg_rank=<1-indexed-in-SCORED; 0=absent>|neg_score=<f32>|
  pos_in_top5=<true|false>|neg_in_top5=<true|false>|
  gap=<f32>|conflict_type=<LogicalContradiction|SupersessionConflict|
    AuthorityPolarity|ScoreAmbiguity|None>|abstained=<true|false>|
  pool_contamination=<true|false>|supersession_leaked=<true|false>|
  class=<comma-sep-class-labels>
```

Where:
- `pos_rank` and `neg_rank` are positions in the **SCORED** pool (not RAW_NN, not CHAIN).
- `gap` = `pos_score - neg_score` in SCORED (positive if D+ outscores D-).
- `pool_contamination` = `true` if D- is absent from RAW_NN for a scenario where it should be present.
- `supersession_leaked` = `true` if D- is present in POST_FILTER for a SUPERSESSION scenario.

---

## 5. Acceptance Gates (Corrected)

### 5.1 Gate Definitions

All gates apply to the **unscoped primary path** (absent `retrieval_intent`).
All gates are predeclared and locked. None may be removed or weakened after observing results.

| Gate | Metric | Definition | Stage | Threshold | CI direction |
|---|---|---|---|---|---|
| **G1** | Recall@1 | `count(pos_rank_SCORED = 1) / total` | SCORED | >= 0.40 | LOWER bound >= 0.40 |
| **G2** | Recall@5 | `count(pos_in_top5_SCORED) / total` | SCORED | >= 0.67 | LOWER bound >= 0.67 |
| **G3** | MRR | `mean(1/pos_rank_SCORED; 0 if absent or abstained)` | SCORED | >= 0.40 | LOWER bound >= 0.40 |
| **G4** | Contradiction-at-1 | `count(neg_rank_SCORED = 1) / total` | SCORED | < 0.40 | **UPPER** bound < 0.40 |
| **G5** | Hard-Neg Occ@5 | `count(neg_in_top5_SCORED) / total` | SCORED | < 0.40 | **UPPER** bound < 0.40 |
| **G6** | Mean Margin | `mean(pos_score_SCORED - neg_score_SCORED)` | SCORED | > 0.00 | LOWER bound > 0.00 |
| **G7** | Supersession Safety | `count(supersession_leaked) / supersession_scenarios` | POST_FILTER | = 0.00 | Exact count |
| **G8** | LC Fire Rate | `count(conflict_type=LogicalContradiction) / CONTRADICTION_scenarios` | SCORED | >= 0.80 | LOWER bound >= 0.80 |
| **G9** | Safe Abstention Rate | `count(abstained=true) / SAFE-ABSTENTION_scenarios` | — | >= 0.80 | LOWER bound >= 0.80 |
| **G10** | Pool Contamination | `count(pool_contamination=true)` | RAW_NN | = 0 | Exact count |
| **S1** | LC Eligibility Rate | `count(D+ and D- both in SCORED top-2 with gap < threshold) / CONTRADICTION_scenarios` | SCORED | >= 0.60 | Exact count |

**S1 (Supporting gate):** Ensures CONTRADICTION scenarios actually reach the eligible state for G8.
If S1 fails, G8 may pass via corpus construction without genuine safety evidence. Both must pass.

> [!IMPORTANT]
> G8 denominator = ALL CONTRADICTION scenarios (end-to-end measurement).
> G8 and G9 are independent (OQ-3 answer): G8 measures classification; G9 measures abstention.

### 5.2 Bootstrap Confidence Intervals (Corrected)

For gates G1–G6, G8, G9, compute 95% bootstrap confidence intervals:

- **Resamples:** 10,000
- **Seed:** `BOOTSTRAP_RNG_SEED = 0x1F4A9C2EB8D37506u64`
- **Tie handling:** Identical composite scores for D+ and D- → D+ is counted below D- (D+ rank = D- rank + 1). Conservative, worst-case.
- **Abstention handling:** `abstained = true` → D+ has no rank → reciprocal rank = 0, Recall@K = 0 for that scenario.
- **MRR rank cap:** NONE — full SCORED candidate list. D+ at rank N contributes 1/N for any N. Absent D+ contributes 0.

**Gate pass conditions (corrected directions):**

| Gate | Point estimate condition | Bootstrap CI condition |
|---|---|---|
| G1 | estimate >= 0.40 | lower 95% CI bound >= 0.40 |
| G2 | estimate >= 0.67 | lower 95% CI bound >= 0.67 |
| G3 | estimate >= 0.40 | lower 95% CI bound >= 0.40 |
| G4 | estimate < 0.40 | **upper** 95% CI bound < 0.40 |
| G5 | estimate < 0.40 | **upper** 95% CI bound < 0.40 |
| G6 | estimate > 0.00 | lower 95% CI bound > 0.00 |
| G8 | estimate >= 0.80 | lower 95% CI bound >= 0.80 |
| G9 | estimate >= 0.80 | lower 95% CI bound >= 0.80 |

Gates G7, G10, S1: exact counts, no bootstrap.

### 5.3 All Gates Must Pass Simultaneously

G1 through G10 plus S1 must all pass. Partial passage is not approval.

### 5.4 Selection Quality Decomposition (Blocker 6 Resolution)

Reported separately — these do not determine approval but must be reported in the evidence log:

| Rate | Definition |
|---|---|
| **Safe answer/selection rate** | `count(pos_rank_SCORED=1, abstained=false) / total` |
| **Correct abstention rate** | `count(abstained=true, class contains SAFE-ABSTENTION) / total` |
| **Unsafe contradiction-selection rate** | `count(neg_rank_SCORED=1, abstained=false) / total` |
| **Unnecessary abstention rate** | `count(abstained=true, class does NOT contain SAFE-ABSTENTION) / total` |

These four rates must sum to 1.0. Any scenario not covered by these four categories is a
reporting error.

---

## 6. Separation Guarantees and Freeze Record (Corrected)

### 6.1 Frozen Artifact Hash Table

All the following files are frozen at the pre-corpus revision. Hashes verified by the verifier
before each run. Any null value must be filled in before execution (not before approval).

| Artifact | Freeze commit | SHA-256 |
|---|---|---|
| `docs/phase3_approval_protocol.md` (v1.0) | `8bc2017` | `D51F0B5E80505DCF0DA3C5DB135E380FD432E048687D59D9743D84B68709F137` |
| `cmhm_core/src/retrieval.rs` | `dacb140` | `DA6B1845AA1811293BCE99BAB1EE7833BF5DCC81E6C295B8A63347FDBDF4A7B9` |
| `cmhm_core/src/engine.rs` | `dacb140` | `F4BBF63D2A12D31DF7BE394F9EFD8B899E112B448972B64D9B5A2A3D0A89B066` |
| `cmhm_mcp/src/tools.rs` | `dacb140` | `EC7924CD401EBC3ACD8864D949F4FB031460396C9DFFEC7E49D9C7727F2C8D49` |
| `cmhm_core/src/text_embed.rs` | `dacb140` | `A6E2A21D483E6F048FE236D4CFDF842AE07E3EF9C8EE250C4D648AB3E71E89B3` |
| `cmhm_mcp/src/schema.rs` | `dacb140` | `BE27A3E4E4956F0C589409A9AE7FD06DF8BD71F13691A4FD63CAFF744F0380FC` |
| `scripts/verify_phase3_approval.ps1` | `dacb140` | `5586559DEA14E2381DB983C1FBF480D66ECB46FDF92FC26DFDC8FBEC23A09405` |
| `scripts/audit_corpus_overlap.ps1` | TBD (after approval) | — |
| `cmhm_core/tests/phase3_approval_bench.rs` | TBD (after approval) | — |

**Source revision at protocol submission:** `dacb140`

### 6.2 Invalidation Rule

Any modification to any of the following files after the corpus is created invalidates the
corpus for approval evidence:

- `cmhm_core/src/retrieval.rs`
- `cmhm_core/src/engine.rs`
- `cmhm_mcp/src/tools.rs`
- `cmhm_core/src/text_embed.rs`
- `cmhm_mcp/src/schema.rs`
- `scripts/verify_phase3_approval.ps1`

### 6.3 Evidence Log Requirements

The evidence log must contain:

1. Git commit SHA and `git status --porcelain` (must be clean).
2. SHA-256 of every frozen artifact (matching the table in Section 6.1).
3. Overlap audit results (Jaccard similarity table vs. both prohibited corpora).
4. Full per-scenario `PHASE3_RESULT` lines.
5. Per-gate verdict: estimate, bootstrap 95% CI bound (correct direction per gate), threshold, PASS/FAIL.
6. Selection quality decomposition rates (Section 5.4).
7. Overall PASS or FAIL.
8. Exit code 0 only on unanimous gate passage (G1–G10 and S1).

---

## 7. Implementation Sequence (Authorized After Explicit Approval Only)

**None of these steps may begin until the user explicitly approves this protocol.**

1. Record SHA-256 of this protocol document (v1.1) at its commit.
2. Write `scripts/audit_corpus_overlap.ps1` — commit and record SHA-256.
3. Create `cmhm_core/tests/phase3_approval_bench.rs` with >= 40 fresh scenarios.
4. Record SHA-256 of the sealed corpus file before any run.
5. Run `scripts/audit_corpus_overlap.ps1` — verify zero contamination before the benchmark.
6. Run `scripts/verify_phase3_approval.ps1 -SyntheticOnly` — verify evaluation logic.
7. Run `scripts/verify_phase3_approval.ps1` (full corpus) exactly once.
8. Emit and commit the evidence log.
9. Submit log and source archive for user review.

**Step 3 must not begin before Steps 1–2 are complete.**  
**Step 5 must precede Step 7.** Any contamination error detected at Step 5 blocks execution.  
**Step 7 runs exactly once** per corpus. Re-running with a modified corpus requires a new protocol revision.

---

## 8. Immutable Parameters

| Parameter | Value | Source |
|---|---|---|
| `TEXT_MODEL_ID` | `"hash-128d"` | `cmhm_mcp/src/tools.rs:21` |
| Embedder | FNV-1a 128-dim L2-normalised | `cmhm_core/src/text_embed.rs:54` |
| NN pool size k | 8 | `retrieval.rs:189-190` |
| `DISTRACTOR_RNG_SEED` | `0xA3F72B91C5D4E608u64` | This protocol Section 3.5 |
| `BOOTSTRAP_RNG_SEED` | `0x1F4A9C2EB8D37506u64` | This protocol Section 5.2 |
| Bootstrap resamples | 10,000 | This protocol Section 5.2 |
| MRR rank cap | NONE | This protocol Section 5.2 (OQ-2 answer) |
| `abstain_on_conflict` | `true` (all scenarios) | This protocol (OQ-1 answer) |

---

*End of Phase 3 Approval Protocol v1.1*  
*Awaiting explicit user approval. No corpus exists. No corpus action is authorized.*
