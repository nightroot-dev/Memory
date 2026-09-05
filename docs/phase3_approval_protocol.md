# Phase 3 Approval Protocol — Living Field Architecture

**Version:** 1.0  
**Status:** SUBMITTED FOR REVIEW — awaiting explicit user approval before corpus creation or execution  
**Source Revision at Protocol Freeze:** `25febae030854a6a71a02fe9a32e20b62c1655a9`  
**Governs:** `cmhm_core/tests/phase3_approval_bench.rs` (not yet created)

---

> **MANDATORY CONSTRAINT:** This document must be committed and its hash recorded  
> **before** the approval corpus is created, and the retrieval implementation must be  
> frozen before corpus creation. Any implementation change after observing corpus  
> results invalidates that corpus for approval evidence.

---

## 1. Purpose and Scope

This protocol predeclares every parameter of the Phase 3 approval evaluation before a
single query is written. The evaluation will independently verify that the production
retrieval path is safe for contradiction suppression, supersession, hard-negative
discrimination, and safe abstention using a fully fresh held-out corpus.

**What this protocol does NOT do:**  
It does not create, seal, inspect, or execute any benchmark query. Corpus creation is
blocked until the user explicitly approves this document after review.

---

## 2. Governing Constraints (Non-Negotiable)

These constraints carry over from the governing handoff document verbatim:

1. Never remove, weaken, or reinterpret an acceptance gate after observing a failure.
2. Test actual end-to-end operational retrieval risk, not pairwise cosine similarity alone.
3. Pure-paraphrase Recall@K is not contradiction-safety evidence unless realistic hard negatives are in the candidate pool.
4. Do not assume Beta-bandit feedback solves contradiction retrieval. Verify the complete retrieval and selection path.
5. Keep `TEXT_MODEL_ID` unchanged (`"hash-128d"`).
6. Do not use MMR, increased retrieval count, accidental dense-core bias, or presumed role scoping as contradiction-safety fixes.
7. Role/type intent must be explicit. Do not infer it merely from an embedding or because the query is text.
8. The prior 15-query rejection set (`dprime_bench.rs`) and the compromised 32-query corpus (`phase3_held_out_bench.rs`) are **rejection/diagnostic evidence only** — they may not be reused, paraphrased, or used for tuning.
9. `retrieval_intent` is caller-supplied metadata, not authenticated truth. Contradiction safety must hold without trusting it.

---

## 3. Corpus Specification

### 3.1 Size and Freshness

- **Minimum:** 35 entirely new queries.
- **Zero reuse:** No query, paraphrase, domain-template substitution, or scenario  
  adaptation from:
  - the 15-query `dprime_bench.rs` rejection set, or
  - the 32-query `phase3_held_out_bench.rs` compromised corpus.
- **Sealing:** Corpus content is sealed from inspection by any agent or human reviewer  
  until both the protocol and the implementation source hash are recorded.

### 3.2 Domain Distribution

Queries must span at least **8 distinct operational domains**, with no single domain  
accounting for more than 6 queries. Required domain coverage:

| Domain class | Required minimum |
|---|---|
| Infrastructure / OS-level configuration | 4 |
| Database configuration and durability | 4 |
| Network, TLS, and reverse-proxy | 4 |
| Container orchestration (Kubernetes / Docker) | 4 |
| Cloud platform and IAM policy | 4 |
| Security controls and cryptography | 4 |
| Distributed systems and consensus | 4 |
| Observability and incident response | 3 |
| Storage and filesystem | 4 |

Total >= 35 queries. No ceiling per domain above 6.

### 3.3 Required Scenario Classes

Every scenario must belong to at least one of the following classes. At least **6 scenarios** must belong to each of the first four classes:

| Class | Description |
|---|---|
| **CONTRADICTION** | Positive and hard negative share an identical non-empty `assertion_key` with opposing `polarity` (+1 / -1). LogicalContradiction must fire if both reach the top-2 scored pool with gap < threshold. |
| **SUPERSESSION** | Positive node explicitly `supersedes` the hard negative. The hard negative must be eligible at NN selection but filtered at Step 2a. SupersessionConflict fires only if superseded node leaks past the filter. |
| **AUTHORITY-POLARITY** | Positive has `provenance` in `{"corrected", "user_explicit"}`, hard negative has `provenance = "contradicted"`. AuthorityPolarity conflict class exercises the mid-path signal. |
| **ADVERSARIAL-AUTHORITY** | Hard negative carries forged high authority (>= 0.80) but `provenance = "contradicted"`. Validates that the engine does not promote contradicted nodes solely on authority weight. |
| **SAFE-ABSTENTION** | Scenario where the correct outcome is abstain: gap < threshold, `LogicalContradiction` class, `abstain_on_conflict = true`. Result must have `abstained = true`, empty `causal_chain`, non-empty `conflict_candidates`. |
| **UNRESOLVED-CONFLICT** | Gap < threshold, no semantic incompatibility class matches — engine must NOT set `conflict_detected`. Validates that score proximity alone is insufficient. |

Scenarios may belong to multiple classes simultaneously (e.g., CONTRADICTION + ADVERSARIAL-AUTHORITY).

### 3.4 Hard Negative Requirements

Every scenario must include at least one hard negative. Hard negatives must:

1. Share **high lexical and/or embedding overlap** with the query (same key terms, same operational context).
2. Carry **opposite operational meaning** (not merely a different recommendation; a directly contradicting one).
3. Be applicable to the **same model ID** (`hash-128d`) and the same active context.
4. Remain **eligible to compete** unless a documented production safety rule legitimately removes them:
   - Supersession (Step 2a filter) is the only rule that may remove them before scoring.
   - If the supersession link is not set, the hard negative must reach the scored pool.
5. CONTRADICTION scenarios must set `assertion_key` (identical for positive and hard negative) and `polarity` (+1 on positive, -1 on hard negative).

### 3.5 Distractor Background Population

Each scenario's live memory context must contain:

- **>= 60 background distractor nodes** from domains other than the query's assertion domain, using the same `model_id = "hash-128d"`.
- Distractors must be seeded with a **fixed RNG seed declared here:** `DISTRACTOR_RNG_SEED = 0xA3F7_2B91_C5D4_E608u64`.
- Distractors must be inserted before positive/negative nodes to prevent ordering artifacts.

### 3.6 Excluded Mechanisms

The following mechanisms are explicitly forbidden as a means to achieve gate passage:

- MMR (Maximal Marginal Relevance) re-ranking.
- Increasing `k` beyond the production default of 8 in `nearest()` / `nearest_by_type()`.
- Dense-core bias tuning.
- Fixing `seed_type` to a single type for all queries (the root flaw of the compromised corpus).
- Relying on `retrieval_intent` being truthful from an untrusted caller.

---

## 4. Production Path Specification

### 4.1 Primary Evaluation Path

**The approval benchmark must exercise:**

```
handle_retrieve_text() [cmhm_mcp/src/tools.rs:838]
  -> text_to_embedding(&params.query)          [hash-128d, FNV-1a, 128-dim, L2-normalised]
  -> NodeType::from_str(&params.retrieval_intent) [fails closed on invalid/missing]
  -> rt.retrieve(ctx_id, emb, "hash-128d", seed_type)
      -> node_store.nearest_by_type(q, model_id, 8, t)   [when seed_type = Some(t)]
        OR node_store.nearest(q, model_id, 8)             [when seed_type = None, unscoped]
      -> supersession filter (Step 2a)
      -> composite scoring: query_weight * cos(q,n) + context_weight * cos(dense_ctx,n) + authority_weight * n.authority
      -> conflict detection (Step 2d): gap < threshold AND semantic class
      -> abstention (Step 2e): if conflict_detected && abstain_on_conflict
```

`TEXT_MODEL_ID` is `"hash-128d"` and must not be changed for this evaluation.

### 4.2 `seed_type` Policy

| Scenario class | `seed_type` value | Rationale |
|---|---|---|
| All scenarios | `None` | Full unscoped scan — the only path that genuinely tests contradiction safety across mixed candidate pools. `Some(t)` is permitted as a secondary diagnostic run only; it cannot determine approval. |

**Note:** `seed_type = None` means `node_store.nearest(q, model_id, 8)` — unscoped across all node types. This is the path that exercises real contradiction risk.

### 4.3 `retrieval_intent` Handling

- Every scenario declares a syntactically valid `retrieval_intent` value.
- The evaluation records whether `NodeType::from_str` parses successfully.
- Approval gates are computed from **unscoped (`seed_type = None`) results only**.
- Scoped results (with `seed_type = Some(t)` derived from `retrieval_intent`) are reported as secondary diagnostics but **do not determine approval**.

### 4.4 Reporting Obligations

The verification script must report, for every scenario:

1. All candidates returned by `nearest()` (IDs, cosine scores, types).
2. Candidates surviving supersession filter (Step 2a).
3. Final scored ranking with composite scores.
4. Whether conflict was detected and which `ConflictType` class was assigned.
5. Whether abstention fired.
6. The seed node identity, authority, and provenance.
7. Whether the positive node was ranked first (Rank@1).
8. Whether the positive node was ranked in top-5 (Rank@5).
9. Hard negative rank in the scored pool.
10. Score gap between ranks 1 and 2.

Any scenario where a hard negative that should be visible is missing from the pool must be flagged as a **pool contamination error** and fails the run.

---

## 5. Acceptance Gates

### 5.1 Gate Definitions

All gates apply to the **unscoped (`seed_type = None`) primary evaluation path**.  
All gates are predeclared and locked. None may be removed or weakened after observing results.

| Gate ID | Metric | Formula | Threshold | Pass condition |
|---|---|---|---|---|
| **G1** | Recall@1 | `count(positive ranked 1st) / total_scenarios` | >= 0.40 | Point estimate >= 0.40 |
| **G2** | Recall@5 | `count(positive in top-5) / total_scenarios` | >= 0.67 | Point estimate >= 0.67 |
| **G3** | MRR | `mean(1 / rank_of_positive)` (rank capped at 10; unranked = 0) | >= 0.40 | Point estimate >= 0.40 |
| **G4** | Contradiction-at-1 | `count(hard_neg ranked 1st) / total_scenarios` | < 0.40 | Point estimate < 0.40 |
| **G5** | Hard-Neg Occupancy@5 | `count(hard_neg in top-5) / total_scenarios` | < 0.40 | Point estimate < 0.40 |
| **G6** | Mean Margin | `mean(score_of_positive - score_of_hard_neg)` | > 0.00 | Point estimate > 0.00 |
| **G7** | Supersession Safety | `count(superseded_hard_neg NOT filtered at Step 2a) / supersession_scenarios` | = 0.00 | Zero leakage |
| **G8** | LogicalContradiction Fire Rate | `count(LogicalContradiction detected) / contradiction_scenarios` | >= 0.80 | >= 80% of eligible scenarios detect the conflict |
| **G9** | Safe Abstention Rate | `count(abstained=true) / safe_abstention_scenarios` | >= 0.80 | >= 80% of eligible scenarios abstain correctly |
| **G10** | Pool Contamination Errors | Count of scenarios where a required hard negative is missing from the pool | = 0 | Zero contamination |

### 5.2 Bootstrap Confidence Intervals

For gates G1-G6, compute 95% bootstrap confidence intervals:

- **Resamples:** 10,000
- **Seed:** `BOOTSTRAP_RNG_SEED = 0x1F4A_9C2E_B8D3_7506u64`
- **Tie handling:** If the positive and hard negative achieve identical composite scores, count the positive as ranked below the hard negative (conservative, worst-case tie resolution).
- **Abstention handling:** If a scenario abstains, the positive node has no rank. Count as rank = infinity (MRR contribution = 0, Recall@K = 0 for that scenario).

Gates G1-G6 pass if and only if **both** the point estimate and the lower bound of the 95% CI meet the threshold.

Gates G7, G9, G10 are exact counts — no bootstrap required.  
Gate G8 uses the same bootstrap procedure as G1-G6 (lower CI bound >= 0.80).

### 5.3 All Gates Must Pass

All 10 gates (G1-G10) must pass simultaneously. Partial passage is not approval.

---

## 6. Separation Guarantees and Hashing Protocol

### 6.1 Pre-Corpus Freezes (all required before corpus creation)

The following artifacts must be committed and their hashes recorded **before** any
corpus query is written or any corpus file is created:

| Artifact | Must-freeze | How |
|---|---|---|
| This protocol document (`docs/phase3_approval_protocol.md`) | YES (this commit) | SHA-256 of file at freeze commit |
| Source revision | YES (`25febae`) | Full commit SHA |
| Retrieval implementation (`cmhm_core/src/retrieval.rs`) | YES (frozen at `25febae`) | SHA-256 of file |
| MCP tools (`cmhm_mcp/src/tools.rs`) | YES (frozen at `25febae`) | SHA-256 of file |
| Verification script (`scripts/verify_phase3_approval.ps1`) | Required before corpus | SHA-256 recorded in evidence log |
| Sealed corpus file (`tests/phase3_approval_bench.rs`) | Sealed after creation | SHA-256 recorded before first execution |

### 6.2 Invalidation Rule

Any modification to `cmhm_core/src/retrieval.rs`, `cmhm_core/src/engine.rs`,
`cmhm_mcp/src/tools.rs`, or `cmhm_core/src/text_embed.rs` after the corpus is
created (but before or after execution) **invalidates the corpus** for approval
evidence. A fresh protocol revision and new corpus are required.

### 6.3 Evidence Log Requirements

The verification script must emit a single evidence log containing:

1. Git commit SHA and `git status --porcelain` output (must be clean).
2. SHA-256 of every frozen artifact listed in Section 6.1.
3. SHA-256 of the sealed corpus file.
4. Full per-scenario reporting as specified in Section 4.4.
5. Per-gate pass/fail with point estimates, bootstrap 95% CI lower bounds, and threshold.
6. Overall PASS or FAIL verdict.
7. Exit code 0 only on unanimous gate passage; non-zero otherwise.

---

## 7. Implementation Sequence (Authorized After Explicit Approval Only)

The following sequence is predeclared for traceability. **None of these steps may begin
until the user explicitly approves this protocol document.**

1. Record SHA-256 of this protocol file at the freeze commit.
2. Record SHA-256 of `retrieval.rs`, `tools.rs` at HEAD.
3. Write `scripts/verify_phase3_approval.ps1` — commit and record its SHA-256.
4. Create `cmhm_core/tests/phase3_approval_bench.rs` with >= 35 fresh scenarios.
5. Record SHA-256 of the sealed corpus file before executing it.
6. Execute the benchmark via the verification script exactly once.
7. Emit and commit the evidence log.
8. Submit the log and source archive for user review.

**Step 4 must not begin before Steps 1-3 are complete.**  
**Step 6 must not begin before Step 5 is complete.**  
**Steps 3-8 require user explicit approval of this protocol first.**

---

## 8. Open Questions for User Review

| # | Open question | Impact |
|---|---|---|
| OQ-1 | Should `abstain_on_conflict` be `true` for ALL approval scenarios or only SAFE-ABSTENTION class scenarios? | Changes G9 denominator and how G1/G3 interact with abstaining scenarios |
| OQ-2 | Is rank cap of 10 for MRR the correct cutoff, or should unranked scenarios use `N+1` (total candidates + 1)? | Minor but affects MRR point estimate |
| OQ-3 | Should G8 (LogicalContradiction Fire Rate) require the conflict to also have `abstained = true`, or merely that `conflict_detected = true`? | Affects whether G8 and G9 are linked or independent |
| OQ-4 | Is 35 the agreed minimum query count, or does the user require a higher floor (e.g., 40)? | Corpus size |

---

## 9. Production Retrieval Contract

This section records the exact production retrieval contract that the benchmark must exercise,
derived from source inspection at `25febae`.

### 9.1 `handle_retrieve_text` Contract (`cmhm_mcp/src/tools.rs:838`)

**Input (JSON args from MCP caller):**
```
{
  "context_id": <u64>,
  "query":      <string>,              // free-form text
  "retrieval_intent": <string>         // "state" | "action" | "outcome" | ...
}
```

**Failure modes (fail closed):**
- `retrieval_intent` missing -> error returned, no retrieval attempted.
- `retrieval_intent` unparseable by `NodeType::from_str` -> error returned.
- `context_id` not found -> error returned.

**Processing:**
1. `text_to_embedding(query)` — FNV-1a 128-dim, L2-normalised, deterministic.
2. `NodeType::from_str(retrieval_intent)` -> `seed_type: Option<NodeType>`.
3. `rt.retrieve(ctx_id, emb, "hash-128d", seed_type)`.

**Output:**
```
RetrievalResult {
  causal_chain:        Vec<Node>,
  dense_context:       Vec<f32>,
  explanation:         String,
  seed_id:             Option<u64>,
  seed_node_type:      Option<NodeType>,
  seed_authority:      f32,
  seed_provenance:     String,
  conflict_detected:   bool,
  conflict_candidates: Vec<ConflictCandidate>,
  abstained:           bool,
}
```

### 9.2 `retrieve()` Internal Contract (`cmhm_core/src/retrieval.rs:140`)

**Step 2 (Seed Selection):**
- `seed_type = Some(t)` -> `node_store.nearest_by_type(q, model_id, 8, t)` (type-fenced).
- `seed_type = None`    -> `node_store.nearest(q, model_id, 8)` (unscoped — approval path).

**Step 2a (Supersession Filter):**
- Candidates where `n.superseded_by.is_some()` are removed entirely before scoring.

**Step 2b (Composite Scoring):**
```
score(n) = query_weight   * cos(query, n.embedding)
         + context_weight * cos(dense_ctx, n.embedding)
         + authority_weight * n.authority
```
`n.emotional_tag` is NOT in the formula.

**Step 2c (Winner):** `scored[0]` after descending sort.

**Step 2d (Conflict Detection):**  
Requires BOTH:
- `score[0] - score[1] < conflict_detection_threshold`
- At least one semantic class:
  - `LogicalContradiction`: `key_a == key_b` (non-empty), `polarity_a * polarity_b < 0`.
  - `SupersessionConflict`: `a.superseded_by == Some(b.id)` OR `b.superseded_by == Some(a.id)`.
  - `AuthorityPolarity`: one provenance in `{"corrected","user_explicit"}`, the other `"contradicted"`.
  - `ScoreAmbiguity`: provenance strings differ, both non-empty, `|auth_a - auth_b| < 0.1`.

**Step 2e (Abstention):**  
If `conflict_detected && abstain_on_conflict`: return empty `causal_chain`, `abstained = true`.

### 9.3 Immutable Parameters

| Parameter | Value | Source |
|---|---|---|
| `TEXT_MODEL_ID` | `"hash-128d"` | `cmhm_mcp/src/tools.rs` |
| Embedder | FNV-1a 128-dim L2-normalised | `cmhm_core/src/text_embed.rs:54` |
| NN pool size k | 8 | `retrieval.rs:189-190` |
| `DISTRACTOR_RNG_SEED` | `0xA3F7_2B91_C5D4_E608u64` | This protocol (Section 3.5) |
| `BOOTSTRAP_RNG_SEED` | `0x1F4A_9C2E_B8D3_7506u64` | This protocol (Section 5.2) |
| Bootstrap resamples | 10,000 | This protocol (Section 5.2) |

---

*End of Phase 3 Approval Protocol v1.0*  
*Awaiting explicit user approval before corpus creation or execution.*
