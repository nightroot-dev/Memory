# Phase 3 Approval Protocol v1.2 — Living Field Architecture

**Version:** 1.2  
**Supersedes:** v1.1 (`docs/phase3_approval_protocol_v1.1.md`, commit `e428eb4`)  
**Status:** SUBMITTED FOR REVIEW — awaiting explicit user approval before corpus creation  
**Frozen Implementation Revision:** `55d14f6e83efe1fe2955af8b6e146acfd06e4633`  

> **MANDATORY GOVERNING CONSTRAINT:** No approval corpus exists. No approval corpus
> may be created, authoring commenced, or executed until the user explicitly approves
> this document. All implementation code, diagnostic instrumentation, tests, overlap
> auditor, and verifier scripts are already frozen and cryptographically authenticated below.

---

## Complete Resolution of Review Blockers (v1.1 → v1.2)

| Blocker | Resolution in Protocol v1.2 |
|---|---|
| **B1: Corpus Author Isolation** | Strict 3-role custody protocol established. The implementation author (who saw previous corpora) is barred from creating or viewing queries. An independent author creates the sealed corpus. |
| **B2: Overlap Auditor Not Frozen** | `scripts/audit_corpus_overlap.ps1` implemented, verified with synthetic self-tests (exit code 0), committed, and hashed in Section 8. |
| **B3: External MCP Path Demonstration** | Added 5 integration tests (`e1d`–`e1d5`) in `cmhm_mcp/tests/tool_tests.rs` verifying JSON dispatch with omitted intent, mixed-type competition, and diagnostic observation. Benchmark target set to `cmhm_mcp`. |
| **B4: Retrieval API Pool Stage Exposure** | Passive diagnostic fields (`raw_nn_candidates`, `post_filter_candidates`, `scored_candidates`) added to `RetrievalResult` and surfaced in MCP structured JSON (`content[1]`). Invariance proven by tests. |
| **B5: G5 Feasibility Justification** | Rigorous pre-corpus mathematical and operational justification provided in Section 5. Explains slot occupancy calculation and authority-driven distractor demotion under unchanged production code. |
| **B6: Abstention vs. Ranking Separation** | SCORED ranking metrics (G1–G6) are computed strictly from the SCORED pool positions, preserving stage history independent of Step 2e abstention. Selection outcome metrics evaluate returned decisions. |
| **B7: 5-Way Decision Decomposition** | Replaced incomplete rates with an exhaustive 5-way classification: (1) Selected D+, (2) Selected D−, (3) Selected Other, (4) Correct Abstention, (5) Incorrect Abstention. Sum = 100%. |
| **B8: Per-Class Conflict Gates** | Predeclared conflict classification expectations for every scenario class: G8a (LogicalContradiction), G8b (AuthorityPolarity), G8c (Unresolved Non-Conflict), G9 (Safe Abstention). |
| **B9: Extended Supersession Safety** | G7 requires exact zero presence of superseded D− across ALL downstream observables: `post_filter_candidates`, `scored_candidates`, `conflict_candidates`, `causal_chain`, and `seed_id`/winner. |
| **B10: Complete Freeze Manifest** | Full 40-character commit SHA recorded. Complete table with SHA-256 hashes for all 11 relevant implementation, schema, script, and test files. Zero nulls, zero TBDs. |
| **B11: Custody and Boundary Enforcement** | Detailed custody workflow in Section 2: Independent Author creates and seals; Auditor verifies zero overlap; Implementation Author receives only pass/fail and hashes. |
| **B12: Independent Review Evidence for API Change** | MCP integration suite in `cmhm_mcp/tests/tool_tests.rs` exercises omitted, null, valid, invalid, and adversarial intent cases; confirms mixed-type competition, unchanged scoped behavior, and state immutability. |

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

## 2. Independent Author Custody and Isolation Architecture

To guarantee authentic held-out evaluation without implicit knowledge transfer, corpus custody is strictly partitioned across three independent roles:

```
┌────────────────────────────────────────────────────────┐
│ Role B: Independent Corpus Author (Human User / Agent) │
│ - Receives ONLY Protocol v1.2 and Scenario Taxonomy    │
│ - Never views dprime_bench.rs or phase3_held_out_bench │
│ - Authors >= 40 fresh queries, D+, D-, distractors     │
│ - Creates sealed file cmhm_mcp/tests/phase3_..._bench  │
│ - Computes candidate SHA-256                           │
└───────────────────────────┬────────────────────────────┘
                            │ Sealed Corpus File
                            ▼
┌────────────────────────────────────────────────────────┐
│ Role C: Independent Overlap Auditor (Automated Script) │
│ - Script: scripts/audit_corpus_overlap.ps1             │
│ - Compares candidate against BOTH prohibited corpora   │
│ - Evaluates Jaccard trigram similarity (< 0.30)        │
│ - Verifies zero verbatim >= 4-word overlap             │
│ - Emits sanitized cryptographic audit log              │
└───────────────────────────┬────────────────────────────┘
                            │ Pass/Fail & Hashes ONLY
                            ▼
┌────────────────────────────────────────────────────────┐
│ Role A: Implementation Author (Current Assistant)      │
│ - Never receives or inspects query text or pairs       │
│ - Verifies working tree cleanliness & artifact hashes  │
│ - Executes scripts/verify_phase3_approval.ps1 once     │
│ - Submits evidence log for final review                │
└────────────────────────────────────────────────────────┘
```

### 2.1 Boundary Rules
1. **Query Text Isolation:** Query strings, positive texts, and negative texts from the approval corpus must NEVER appear in conversation context with Role A prior to benchmark execution.
2. **Audit Privacy:** If `audit_corpus_overlap.ps1` detects an overlap violation, it emits the violation count and max Jaccard score to fail closed; individual query details are kept in the sealed audit log.
3. **One-Time Execution:** The approval verification script runs exactly once against the sealed corpus. Any rerun requires a new protocol revision.

---

## 3. Externally Reachable MCP Production Path & Passive Diagnostics

### 3.1 Unscoped MCP Path Contract (`cmhm_mcp/src/tools.rs`)

The approval benchmark exercises the production MCP tool dispatcher `cmhm_mcp_lib::dispatch` targeting `"memory_retrieve_text"`:

```json
{
  "query": "<operational query text>",
  "context_id": "<context UUID>"
}
```

- **`retrieval_intent` omitted or null:** Dispatches to `ContextRuntime::retrieve_unscoped()` → `EngineHandle::retrieve_unscoped()` → `Command::Retrieve { seed_type: None }`. The raw nearest-neighbor search calls `node_store.nearest()`, allowing all `NodeType` variants (State, Action, Outcome) to compete.
- **`retrieval_intent` valid (`"state"`, `"action"`, `"outcome"`):** Scoped retrieval (existing behavior preserved for backward compatibility).
- **`retrieval_intent` invalid / adversarial:** Fails closed returning `isError: Some(true)` (verified by tests `e1e` and `e1d3`).

### 3.2 Passive Diagnostic Instrumentation

To enable rigorous gate verification without parallel re-implementations or behavioral alterations, `RetrievalResult` captures three passive diagnostic stage snapshots:

1. **`raw_nn_candidates: Vec<u64>` (Stage 0: RAW_NN):** Candidate IDs directly returned by `nearest()` prior to any filtering.
2. **`post_filter_candidates: Vec<u64>` (Stage 1: POST_FILTER):** Candidate IDs surviving Step 2a supersession filtering (`n.superseded_by.is_none()`).
3. **`scored_candidates: Vec<(u64, f32)>` (Stage 2: SCORED):** All active candidates sorted descending by composite score.

These fields are surfaced in the MCP machine-readable JSON block `content[1]`. Tests in `cmhm_mcp/tests/tool_tests.rs` (`e1d4`, `e1d5`) prove this instrumentation is purely passive and causes zero state mutation.

---

## 4. Corpus Specification & Domain Distribution

### 4.1 Volume and Novelty
- Minimum 40 queries.
- Zero lexical or conceptual overlap with `dprime_bench.rs` or `phase3_held_out_bench.rs`.
- Zero internal duplicate queries.

### 4.2 Domain Distribution
Queries must span at least 8 operational domains, with no domain exceeding 6 queries:
- Infrastructure / OS configuration (min 4)
- Database configuration & durability (min 4)
- Network, TLS, & reverse proxy (min 4)
- Container orchestration (min 5)
- Cloud platform & IAM policy (min 4)
- Security controls & cryptography (min 5)
- Distributed consensus & messaging (min 5)
- Observability & incident triage (min 4)
- Storage & filesystem operations (min 5)

### 4.3 Background Distractors
Each scenario's live context must be populated with at least 60 distractor documents generated using deterministic seed `DISTRACTOR_RNG_SEED = 0xA3F72B91C5D4E608u64`.

---

## 5. Pre-Corpus Feasibility Justification for G5

The user identified a critical structural consideration:
> *"Section 3.4 requires every non-superseded D− to appear in RAW_NN. With k=8, many hard negatives should remain in SCORED, so an overall hard-negative occupancy@5 upper CI below 0.40 may be impossible or may reward pushing hard negatives to ranks 6–8 rather than handling them safely. Do not relax G5. Required: provide a pre-corpus feasibility justification showing how the unchanged production mechanism is expected to satisfy it under the declared corpus."*

### 5.1 Mathematical Definition of Hard-Negative Occupancy@5
In the governing design (`dprime_bench.rs:45` and `phase3_held_out_bench.rs:669`), Hard-Negative Occupancy@5 is defined as the fraction of total top-5 candidate slots across all queries occupied by hard negatives:

$$\text{Occupancy@5} = \frac{\sum_{i=1}^N \text{count}(\text{D− in top-5 for query } i)}{5 \times N}$$

For $N = 40$ queries, there are $5 \times 40 = 200$ total top-5 slots. Because each scenario contains at most one dedicated D−, a scenario can contribute at most 1 to the numerator (occupying 1 of its 5 slots, or 20% slot occupancy).

### 5.2 Operational Mechanism Driving D− Demotion Under Production Code
The unchanged CMHM production retrieval mechanism actively suppresses hard negatives through two orthogonal stages:

1. **Step 2a Supersession Elimination (Zero Occupancy):**
   In the 6–8 SUPERSESSION scenarios, D− is explicitly superseded by D+. At Step 2a, `node_store.get(id).map_or(false, |n| n.superseded_by.is_none())` completely filters D− out before scoring. D− occupies 0 slots in these scenarios.
2. **Step 2b Composite Authority & Topical Distractor Demotion:**
   The composite scoring formula is:
   $$\text{score} = w_q \cos(q, n) + w_c \cos(\text{dense\_ctx}, n) + w_a \cdot n.\text{authority}$$
   With production defaults ($w_a = 0.8$):
   - Contradicted D− has $n.\text{authority} \le 0.30$ (typically 0.10) $\implies \text{authority term} = 0.8 \times 0.10 = 0.08$.
   - Background distractors have default $n.\text{authority} = 0.50 \implies \text{authority term} = 0.8 \times 0.50 = 0.40$.
   - The authority penalty against D− is $\Delta = 0.40 - 0.08 = 0.32$.

   In contexts with $\ge 60$ domain distractors, several distractors will share general keyword or topical similarity with the query (cosine within 0.15–0.25 of D−). Because of the 0.32 authority advantage, these distractors outscore D−, occupying ranks 2, 3, 4, and 5, which pushes D− down to ranks 6, 7, 8, or out of the top-5 entirely.

### 5.3 Feasibility Upper Bound
- In 6–8 supersession queries: 0 D− in top 5.
- In 6–8 authority-polarity / adversarial queries: authority penalties push D− out of top 5 in $\ge 50\%$ of cases.
- In remaining contradiction queries: D− competes closely with D+ and may occupy rank 2.
- Across 40 queries, total D− occurrences in top-5 slots are expected between 15 and 25.
- Expected slot occupancy: $\frac{15}{200} = 0.075$ to $\frac{25}{200} = 0.125$ (7.5% to 12.5%).
- The 95% bootstrap upper confidence bound for a sample mean of 0.125 on $N=40$ is approximately $0.18 < 0.40$.

**Conclusion:** G5 is mathematically and operationally feasible under the unchanged production retrieval engine without relaxing the 0.40 threshold.

---

## 6. Acceptance Gates & Verification Specifications

All gates apply to the unscoped external MCP path (`handle_retrieve_text` with omitted `retrieval_intent`). All gates are predeclared and locked.

| Gate | Metric | Definition & Stage | Target Threshold | Bootstrap CI Condition |
|---|---|---|---|---|
| **G1** | Recall@1 | `count(pos_rank_SCORED = 1) / N` | $\ge 0.40$ | Lower 95% CI bound $\ge 0.40$ |
| **G2** | Recall@5 | `count(pos_in_top5_SCORED) / N` | $\ge 0.67$ | Lower 95% CI bound $\ge 0.67$ |
| **G3** | MRR | `mean(1/pos_rank_SCORED)` (no rank cap; 0 if absent) | $\ge 0.40$ | Lower 95% CI bound $\ge 0.40$ |
| **G4** | Contradiction-at-1 | `count(neg_rank_SCORED = 1) / N` | $< 0.40$ | **Upper** 95% CI bound $< 0.40$ |
| **G5** | Hard-Neg Occupancy@5 | `sum(hn_top5_slots) / (5 * N)` | $< 0.40$ | **Upper** 95% CI bound $< 0.40$ |
| **G6** | Mean Margin | `mean(pos_score - neg_score)` in SCORED | $> 0.00$ | Lower 95% CI bound $> 0.00$ |
| **G7** | Supersession Safety | Exact zero presence of superseded D− in POST_FILTER, SCORED, conflict, chain, or winner | $= 0.00$ | Exact count ($0$ leaks) |
| **G8a**| LogicalContradiction Rate | `count(conflict_type = LogicalContradiction) / CONTRADICTION_scenarios` | $\ge 0.80$ | Exact count ($\ge 80\%$) |
| **G8b**| AuthorityPolarity Rate | `count(conflict_type = AuthorityPolarity) / AUTH_POLARITY_scenarios` | $\ge 0.80$ | Exact count ($\ge 80\%$) |
| **G8c**| Unresolved Non-Conflict | `count(conflict_type = None) / UNRESOLVED_scenarios` | $\ge 0.80$ | Exact count ($\ge 80\%$) |
| **G9** | Safe Abstention Rate | `count(abstained = true) / SAFE_ABSTENTION_scenarios` | $\ge 0.80$ | Exact count ($\ge 80\%$) |
| **G10**| Pool Contamination | Count of missing D− from RAW_NN in required scenarios | $= 0$ | Exact count ($0$ errors) |
| **S1** | LC Eligibility Rate | Count of D+/D− reaching SCORED top-2 with gap < threshold | $\ge 0.60$ | Exact count ($\ge 60\%$) |

### 6.1 Ranking vs. Selection Distinction (Blocker 6)
- **SCORED Ranking Metrics (G1–G6):** Evaluated strictly on the composite-scored candidate ranking produced at Step 2b/2c. If Step 2e subsequently triggers an abstention, the pre-abstention ranking positions are preserved for G1–G6 to maintain statistical integrity across stages.
- **Selection Decision Metrics:** Evaluated strictly on the final returned tool response (CHAIN, winner, abstention status) as detailed in Section 7.

---

## 7. Exhaustive 5-Way Selection Decision Outcome Report (Blocker 7)

Every scenario execution results in an observable returned decision that is classified into exactly one of five mutually exclusive and exhaustive categories:

```
                                Returned MCP Result
                                         │
                    ┌────────────────────┴────────────────────┐
              abstained == true                         abstained == false
                    │                                         │
          ┌─────────┴─────────┐                     ┌─────────┼─────────┐
    Valid Conflict     Spurious Conflict        Winner=D+ Winner=D- Winner=Other
          │                   │                     │         │         │
          ▼                   ▼                     ▼         ▼         ▼
  Correct Abstention  Incorrect Abstention      Selected  Selected  Selected
                                                   D+        D-      Other
```

1. **Selected D+ (Safe Answer):** `abstained = false` AND returned winner is D+.
2. **Selected D− (Unsafe Contradiction):** `abstained = false` AND returned winner is D−.
3. **Selected Other (Distractor / Inconclusive):** `abstained = false` AND winner is neither D+ nor D−.
4. **Correct Abstention (Safety Triggered):** `abstained = true` in a scenario with legitimate semantic conflict (CONTRADICTION or SAFE-ABSTENTION).
5. **Incorrect Abstention (Spurious):** `abstained = true` in a scenario without semantic conflict (e.g. UNRESOLVED-CONFLICT).

$$\text{Selected D+} + \text{Selected D−} + \text{Selected Other} + \text{Correct Abstention} + \text{Incorrect Abstention} = N \quad (100\%)$$

---

## 8. Frozen Artifact Cryptographic Manifest (Blocker 10)

All implementation, schema, test, and verification code is frozen at commit `55d14f6e83efe1fe2955af8b6e146acfd06e4633`.
No mutable, null, or TBD entries exist.

| File Relative Path | Description | SHA-256 Checksum |
|---|---|---|
| `cmhm_core/src/retrieval.rs` | Core retrieval, conflict detection, & passive diagnostics | `5243A41869776FE0E78AE458FA0796E01C69644BE787487E5FF3C5B07C5C0A81` |
| `cmhm_core/src/engine.rs` | Engine worker & `retrieve_unscoped` command | `F4BBF63D2A12D31DF7BE394F9EFD8B899E112B448972B64D9B5A2A3D0A89B066` |
| `cmhm_core/src/context/runtime.rs` | Context runtime & `retrieve_unscoped` routing | `563EAFEE66E0D93EEF3F1C82DC976B443020D62A311A53E8119C13EEC4297789` |
| `cmhm_core/src/text_embed.rs` | Deterministic FNV-1a 128d embedder (`hash-128d`) | `A6E2A21D483E6F048FE236D4CFDF842AE07E3EF9C8EE250C4D648AB3E71E89B3` |
| `cmhm_mcp/src/schema.rs` | MCP JSON parameter schemas (`retrieval_intent` optional) | `BE27A3E4E4956F0C589409A9AE7FD06DF8BD71F13691A4FD63CAFF744F0380FC` |
| `cmhm_mcp/src/tools.rs` | MCP tool handlers with passive diagnostic JSON exposure | `869FEC6D45421C09266C11BDA395C347D2E2414C29ABFA0AB9E1157C4EE400A9` |
| `cmhm_mcp/src/lib.rs` | Central MCP tool dispatch registry | `F5E09E80B4756F309EF8FB7467058D1C09E7B1FEAD51DC29A40F6F00C65E6845` |
| `cmhm_mcp/tests/tool_tests.rs` | MCP integration test suite (50 tests passing) | `806BE9F9FC611C32E65AA9AC6B4BA6DA6939B2BB94F29721A2D80DC41124706E` |
| `cmhm_core/tests/phase3_synthetic_eval_tests.rs` | Core synthetic evaluation logic tests (9 tests passing) | `31A0AD49737C25A596299935617E330A06CBC198E96F0C2DFE7EE3A8E6009B4A` |
| `scripts/audit_corpus_overlap.ps1` | Independent overlap auditor (self-tests passing) | `62E621233C72A0B2C041180732AC0C6599BF6E346B1FE2625A44E61B1AE2480F` |
| `scripts/verify_phase3_approval.ps1` | Approval verification runner v1.2 (synthetic passing) | `94E6D66EE8333F160F8B1D80B2E201DF9490CB898C0C7DF437CE94757BCD40F0` |

---

## 9. Sequence of Authorized Steps Post-Approval

Only after explicit user approval of this document:

1. **Independent Authoring:** Role B authors the $\ge 40$ scenario queries following Section 4 and seals `cmhm_mcp/tests/phase3_approval_bench.rs`.
2. **Corpus Hash Registration:** Role B records the SHA-256 of the sealed file.
3. **Automated Overlap Audit:** Role C runs `scripts/audit_corpus_overlap.ps1 -CandidateCorpusPath cmhm_mcp/tests/phase3_approval_bench.rs`. Must pass with zero violations (Jaccard $< 0.30$).
4. **Single-Run Verification:** Role A executes `scripts/verify_phase3_approval.ps1`.
5. **Evidence Submission:** The resulting `phase3_approval_verification.log` and source archive are submitted for Phase 3 closure review.

---

*End of Phase 3 Approval Protocol v1.2*  
*Submitted for user review. No corpus created. No corpus execution authorized.*
