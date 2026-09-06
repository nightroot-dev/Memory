# Phase 3 Approval Protocol v1.3 — Living Field Architecture

**Version:** 1.3  
**Supersedes:** v1.2 (`docs/phase3_approval_protocol_v1.2.md`, commit `a5cdc22`)  
**Status:** SUBMITTED FOR REVIEW — awaiting explicit user approval before corpus creation  
**Frozen Implementation Revision:** `bc6634047cb900f90fd9832a6cf83793e885d1dc` (Commit `bc66340`)  
**Frozen Implementation Archive:** `phase3-bc66340.zip` (2,818,768 bytes, SHA-256: `049AA1EBD657C8BCC2F30060C12E5931A402A8A346A9F56667AE32DEF62A6762`)  

> **MANDATORY GOVERNING CONSTRAINT:** No approval corpus exists. No approval corpus
> may be created, authored, inspected, or executed until the user explicitly approves
> this document. All implementation code, diagnostic instrumentation, tests, overlap
> auditor, and verifier scripts are frozen at commit `bc66340` and authenticated below.

---

## Detailed Resolution of Review Defect Points (v1.2 → v1.3)

| Defect | Resolution in Protocol v1.3 |
|---|---|
| **1. Revision Identity & Manifest** | Implementation is frozen at commit `bc66340`. An immutable zip archive (`phase3-bc66340.zip`) has been created and hashed. The manifest in Section 8 is generated directly from files by `scripts/generate_freeze_manifest.ps1`. |
| **2. Independent Custody & Execution** | Role B (Independent Author & Execution Custodian) executes both the overlap audit and the benchmark. Role A (Assistant) is strictly outside the custody boundary and receives only sanitized logs and hashes. |
| **3. Measurable Novelty Rules** | Replaced "zero conceptual overlap" with 3 frozen, measurable rules: Jaccard word-trigram $< 0.30$, zero verbatim sequences $\ge 6$ words, zero exact normalized query matches, plus an independent qualitative checklist. |
| **4. Valid G5 Feasibility Argument** | Removed distractor crowding / low-authority assumptions. Formally derived slot occupancy ($\frac{\sum \text{HN in top-5}}{200}$) under equal and adversarial authority, demonstrating that $\le 1$ D− per query mathematically bounds occupancy $\le 0.20 \ll 0.40$. |
| **5. Unambiguous Gate Populations** | Section 6 provides an exact table specifying denominator, numerator, and threshold for every gate. G8a denominator is explicitly defined as eligible contradiction scenarios, with S1 separately gating eligibility. |
| **6. Scenario-Class Matrix** | Complete 6-class matrix (Section 4.3) declaring counts, authority, provenance, RAW_NN presence, filter survival, expected conflict type, expected abstention, expected winner, and evaluating gates. |
| **7. Predeclared Selection Correctness** | Correctness is determined by an explicit, predeclared `expected_abstain: bool` field per scenario, eliminating post-hoc inference. |
| **8. Passive Diagnostic Equivalence Criterion** | Verified by dedicated test `synth_10_passive_diagnostic_equivalence`, proving functional outputs (IDs, scores, conflict type, abstention, chain) match ground truth float-for-float with zero side effects. |
| **9. Multi-Stage Lifecycle & One-Run Rule** | Defined 4 distinguishable stages (Pre-Flight, Audit, Execution, Gate Verification). Specifies precisely that launching Stage 2 permanently consumes the single approval attempt. |

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

## 2. Independent Custody and Operator Boundary Architecture

To eliminate implicit knowledge transfer and ensure genuine held-out evaluation, custody and execution are strictly partitioned:

```
┌────────────────────────────────────────────────────────────────────────┐
│ Role B: Independent Corpus Author & Execution Custodian                │
│ (Human User or Independent Isolated Operator)                          │
│                                                                        │
│ 1. Receives ONLY Protocol v1.3 and the Scenario-Class Taxonomy.        │
│ 2. Never accesses dprime_bench.rs or phase3_held_out_bench.rs.         │
│ 3. Authors >= 40 fresh scenario queries following Section 4.           │
│ 4. Commits sealed benchmark file cmhm_mcp/tests/phase3_approval_bench  │
│ 5. Executes scripts/audit_corpus_overlap.ps1 (Stage 1).                │
│ 6. If audit passes, executes scripts/verify_phase3_approval.ps1 (Stg 2)│
│ 7. Retains sealed logs; provides only sanitized evidence log to Role A.│
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Sanitized Evidence Log & Hashes ONLY
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Role A: Implementation Author (Coding Assistant)                       │
│                                                                        │
│ 1. Implements engine code, passive diagnostics, tests, and scripts.    │
│ 2. Strictly barred from authoring, viewing, or inspecting queries.     │
│ 3. Does not execute the approval benchmark.                            │
│ 4. Receives only final sanitized evidence log and archive for review.  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Measurable Novelty Rules & Overlap Audit

The candidate corpus must satisfy three frozen, automated novelty criteria enforced by `scripts/audit_corpus_overlap.ps1`:

1. **Lexical Jaccard Trigram Threshold:**
   Text is normalized (lowercase, alphanumeric + single space) and converted to sliding word-level 3-grams. The Jaccard similarity between any candidate string (query, positive, or hard negative) and any reference string from `dprime_bench.rs` or `phase3_held_out_bench.rs` must be:
   $$J(A, B) = \frac{|A \cap B|}{|A \cup B|} < 0.30$$
   Any pair with $J(A, B) \ge 0.30$ is an **OVERLAP VIOLATION** and fails closed.
2. **Verbatim Sequence Ceiling:**
   Zero shared sequences of $\ge 6$ consecutive words between any candidate string and any reference string.
3. **Exact Query Collision Prohibition:**
   Zero candidate queries identical to any reference query under normalization.
4. **Internal Duplicate Prohibition:**
   Candidate corpus must contain zero duplicate queries.
5. **Blinded Qualitative Review:**
   Role B certifies via a predeclared checklist that each scenario tests an independent operational failure mode, distinct root cause, and distinct mitigation path from any scenario in the prohibited corpora.

---

## 4. Scenario-Class Taxonomy & Complete Matrix

### 4.1 Corpus Volume and Predeclared Counts
The corpus must contain exactly 40 dedicated scenarios spanning at least 8 operational domains, with no domain exceeding 6 queries. Multi-label credit sharing is prohibited; every scenario belongs to exactly one primary class:

| Domain | Min Scenarios |
|---|---|
| Infrastructure / OS configuration | 4 |
| Database configuration & durability | 4 |
| Network, TLS, & reverse proxy | 4 |
| Container orchestration (K8s/Docker) | 5 |
| Cloud platform & IAM policy | 4 |
| Security controls & cryptography | 5 |
| Distributed consensus & messaging | 5 |
| Observability & incident triage | 4 |
| Storage & filesystem operations | 5 |

### 4.2 Background Distractors
Each scenario context must contain $\ge 60$ background distractor nodes generated deterministically using seed `DISTRACTOR_RNG_SEED = 0xA3F72B91C5D4E608u64`.

### 4.3 Complete Scenario-Class Matrix

| Class Name | Min Count | D− Authority / Provenance | D+ Authority / Provenance | D− in RAW_NN? | D− survives Step 2a? | Expected Conflict Type | Expected Abstain | Expected Winner | Evaluating Gates |
|---|---|---|---|---|---|---|---|---|---|
| **CONTRADICTION** | 10 | 0.70–0.85 / `contradicted` (same assertion_key, polarity -1) | 0.85 / `user_explicit` (same assertion_key, polarity +1) | **Yes** | **Yes** | `LogicalContradiction` | `true` | None (abstained) | G1–G6, G8a, S1, G10 |
| **SUPERSESSION** | 6 | 0.85 / `contradicted` (superseded by D+) | 0.90 / `user_explicit` (`supersedes = Some(D-_id)`) | **Yes** | **NO** (pruned) | `None` | `false` | $D^+$ | G1–G6, G7 |
| **AUTHORITY-POLARITY**| 6 | 0.75 / `contradicted` (different/no assertion key) | 0.85 / `user_explicit` or `corrected` | **Yes** | **Yes** | `AuthorityPolarity` | `true` | None (abstained) | G1–G6, G8b, G10 |
| **ADVERSARIAL-AUTHORITY**| 6 | 0.95 / `contradicted` (elevated authority) | 0.80 / `user_explicit` (standard authority) | **Yes** | **Yes** | `AuthorityPolarity` | `true` | None ($D^-$ never wins) | G1–G6, G8b, G10 |
| **SAFE-ABSTENTION** | 6 | 0.80 / `contradicted` (near-equal score gap < 0.02) | 0.85 / `user_explicit` | **Yes** | **Yes** | `LogicalContradiction` or `AuthorityPolarity` | `true` | None (abstained) | G1–G6, G9, G10 |
| **UNRESOLVED-CONFLICT**| 6 | 0.80 / `user_explicit` (identical provenance, no polarity clash) | 0.85 / `user_explicit` | **Yes** | **Yes** | `None` | `false` | $D^+$ (higher score) | G1–G6, G8c, G10 |

---

## 5. Mathematical & Operational Feasibility Justification for G5

### 5.1 Formal Definition of Hard-Negative Occupancy@5
Hard-Negative Occupancy@5 measures the proportion of total top-5 candidate slots across all queries occupied by designated hard negatives:

$$\text{Occupancy@5} = \frac{\sum_{i=1}^N \sum_{k=1}^5 \mathbb{I}(\text{candidate at rank } k \text{ of query } i \in \mathcal{D}^-)}{5 \times N}$$

For $N = 40$ queries, the denominator is exactly $5 \times 40 = 200$ slots. $\mathcal{D}^-$ represents the set of all designated hard negatives across the entire corpus.

### 5.2 Mathematical Derivation Under Equal and Adversarial Authority
The feasibility of G5 does not depend on distractor crowding or low-authority hard negatives:

1. **Dedicated Negative vs. Cross-Domain Scenarios:**
   Each query $q_i$ has exactly one dedicated hard negative $D_i^-$. The remaining 39 hard negatives ($D_j^-$ for $j \ne i$) belong to completely disjoint operational domains (e.g. Kubernetes CNI vs PostgreSQL replication) and have low cosine similarity to $q_i$. Therefore, cross-domain hard negatives virtually never enter the top-5 of $q_i$.
2. **Supersession Scenarios (min 6):**
   In the 6 SUPERSESSION scenarios, Step 2a prunes $D_i^-$ before scoring:
   $$\sum_{k=1}^5 \mathbb{I}(\text{candidate at rank } k \in \mathcal{D}^-) = 0$$
3. **Worst-Case Contradiction & Adversarial Scenarios (34 scenarios):**
   Even when $D_i^-$ has equal authority ($0.85$) or adversarial authority ($0.95$) and achieves rank 1 or 2, $D_i^-$ occupies **at most 1 slot out of the 5 slots** for that query ($20\%$ slot occupancy for query $i$).
4. **Aggregate Slot Occupancy:**
   Across all 40 queries, even if $D_i^-$ appears in the top-5 for every single one of the 34 non-superseded scenarios:
   $$\text{Total HN Slots} \le 6 \times 0 + 34 \times 1 = 34 \text{ slots}$$
   $$\text{Mean Occupancy} = \frac{34}{200} = 0.1700 \quad (17.0\%)$$
5. **Bootstrap Distribution Under 10,000 Resamples:**
   Each query contributes a value $X_i \in \{0.0, 0.20\}$ (or occasionally $0.40$ if a cross-domain negative collides). With 6 zeros and 34 values of 0.20, sample variance $s^2 \approx 0.0053$ ($s \approx 0.073$).
   The standard error of the mean on $N=40$ is:
   $$\text{SE} = \frac{0.073}{\sqrt{40}} \approx 0.0115$$
   The upper 95% bootstrap confidence bound is approximately:
   $$\text{Upper 95\% CI} \approx 0.170 + 1.96 \times 0.0115 \approx 0.193 \ll 0.400$$

**Conclusion:** G5 ($< 0.40$) is structurally sound and mathematically satisfied by design, even when hard negatives possess elevated adversarial authority. The 0.40 threshold is strictly maintained.

---

## 6. Acceptance Gates, Populations, and Stage Invariants

### 6.1 Gate Definitions & Exact Populations

All gates evaluate the external MCP production path (`handle_retrieve_text` with omitted `retrieval_intent`).

| Gate | Metric | Exact Formula & Numerator | Denominator Population | Target Threshold | Bootstrap CI Condition |
|---|---|---|---|---|---|
| **G1** | Recall@1 | `count(pos_rank_SCORED = 1)` | Full corpus ($N=40$) | $\ge 0.40$ | Lower 95% CI bound $\ge 0.40$ |
| **G2** | Recall@5 | `count(pos_in_top5_SCORED)` | Full corpus ($N=40$) | $\ge 0.67$ | Lower 95% CI bound $\ge 0.67$ |
| **G3** | MRR | $\sum_{i=1}^N \frac{1}{\text{pos\_rank}_i}$ (no rank cap; 0 if absent) | Full corpus ($N=40$) | $\ge 0.40$ | Lower 95% CI bound $\ge 0.40$ |
| **G4** | Contradiction-at-1 | `count(neg_rank_SCORED = 1)` | Full corpus ($N=40$) | $< 0.40$ | **Upper** 95% CI bound $< 0.40$ |
| **G5** | Hard-Neg Occupancy@5 | Total designated hard negatives in top-5 slots | $5 \times N = 200$ slots | $< 0.40$ | **Upper** 95% CI bound $< 0.40$ |
| **G6** | Mean Margin | $\sum (S(D_i^+) - S(D_i^-))$ (absent score = 0.0) | Full corpus ($N=40$) | $> 0.00$ | Lower 95% CI bound $> 0.00$ |
| **G7** | Supersession Safety | Count of superseded D− appearing in post_filter, scored, conflict, chain, or winner | SUPERSESSION scenarios ($N \ge 6$) | $= 0.00$ | Exact count ($0$ violations) |
| **G8a**| LogicalContradiction Rate | `count(conflict_type = LogicalContradiction)` | **Eligible** CONTRADICTION ($S1$) | $\ge 0.80$ | Exact count ($\ge 80\%$) |
| **S1** | LC Eligibility Rate | Count of D+/D− reaching SCORED top-2 with gap < 0.02 | **All** CONTRADICTION ($N \ge 10$) | $\ge 0.60$ | Exact count ($\ge 60\%$) |
| **G8b**| AuthorityPolarity Rate | `count(conflict_type = AuthorityPolarity)` | AUTH_POLARITY + ADVERSARIAL ($N \ge 12$) | $\ge 0.80$ | Exact count ($\ge 80\%$) |
| **G8c**| Unresolved Non-Conflict | `count(conflict_type = None)` | UNRESOLVED-CONFLICT ($N \ge 6$) | $\ge 0.80$ | Exact count ($\ge 80\%$) |
| **G9** | Safe Abstention Rate | `count(abstained = true)` | SAFE-ABSTENTION ($N \ge 6$) | $\ge 0.80$ | Exact count ($\ge 80\%$) |
| **G10**| Pool Contamination | Count of scenarios where D− is absent from RAW_NN | All 34 non-superseded scenarios | $= 0$ | Exact count ($0$ errors) |

### 6.2 Ranking vs. Selection Separation Invariant
- **SCORED Ranking Metrics (G1–G6):** Evaluated strictly on the candidate ranking produced at Step 2b/2c. When Step 2e subsequently triggers an abstention, the pre-abstention ranking positions are preserved for G1–G6.
- **Selection Decision Metrics:** Evaluated on the final returned tool response.

---

## 7. Predeclared 5-Way Selection Decision Outcome Categorization

Each scenario in the benchmark schema carries an explicit predeclared contract: `expected_abstain: bool` and `expected_winner: Option<ExpectedWinner>`.

Returned decisions are categorized into five mutually exclusive buckets:
1. **`selected_d_plus`:** `abstained = false` AND `winner_id = Some(pos_id)`
2. **`selected_d_minus`:** `abstained = false` AND `winner_id = Some(neg_id)`
3. **`selected_other`:** `abstained = false` AND `winner_id` is neither $D^+$ nor $D^-$
4. **`correct_abstention`:** `abstained = true` AND `scenario.expected_abstain = true`
5. **`incorrect_abstention`:** `abstained = true` AND `scenario.expected_abstain = false`

$$\text{selected\_d\_plus} + \text{selected\_d\_minus} + \text{selected\_other} + \text{correct\_abstention} + \text{incorrect\_abstention} = N \quad (100\%)$$

---

## 8. Frozen Artifact Cryptographic Manifest

Implementation freeze commit: `bc6634047cb900f90fd9832a6cf83793e885d1dc`  
Archive: `phase3-bc66340.zip` (SHA-256: `049AA1EBD657C8BCC2F30060C12E5931A402A8A346A9F56667AE32DEF62A6762`)  
Manifest generated directly by `scripts/generate_freeze_manifest.ps1`:

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
| `cmhm_core/tests/phase3_synthetic_eval_tests.rs` | Core synthetic evaluation logic & diagnostic tests (10 passing) | `5D0C483C9258BAD34DC62364A16C8833BD798AA2C025563A1069F43850416AC0` |
| `scripts/audit_corpus_overlap.ps1` | Independent overlap auditor (self-tests passing) | `E3C8BE2FF91E5083864769F1189070846BD5808B7204161C3956A630CB7CCB59` |
| `scripts/verify_phase3_approval.ps1` | Approval verification runner v1.3 (synthetic passing) | `519EA3AC53FE3E73768B950352AEDCF0437D3044C219E45EC45958BE2EEA94D2` |

---

## 9. Multi-Stage Lifecycle & The One-Run Rule

Execution follows four strictly distinguished stages:

```
[Stage 0: Pre-Flight] ──► [Stage 1: Overlap Audit] ──► [Stage 2: Single Run] ──► [Stage 3: Gate Verification]
  Clean repo & synth       Audit against prohibited      Single test execution     Evaluate G1-G10 & S1
  failures do not          corpora. Any violation        permanently consumes      Exit 0 = Approved
  consume attempt.         fails & TERMINATES attempt.   the approval attempt.     Exit 1 = Rejected
```

1. **Stage 0 (Pre-Flight Verification):** Verifies clean working tree and executes synthetic self-tests (`-SyntheticOnly`). Build or environment fixes here do not touch the corpus and do not consume an attempt.
2. **Stage 1 (Cryptographic Freeze & Overlap Audit):** Role B commits `phase3_approval_bench.rs` and executes `scripts/audit_corpus_overlap.ps1`. If any overlap violation is detected, **the candidate corpus is rejected and the attempt is terminated.** Editing the corpus in place is prohibited; a new protocol revision is required.
3. **Stage 2 (Single Benchmark Execution):** Role B launches `scripts/verify_phase3_approval.ps1`. **Launching this step permanently consumes the single approval attempt.** If a panic, timeout, assertion failure, or partial result occurs, the run fails closed. No retries or parameter adjustments are permitted.
4. **Stage 3 (Gate Verification & Evidence Packaging):** Verifier parses structured output, evaluates all gates, prints the 5-way outcome decomposition, and exits 0 only on unanimous gate passage.

---

*End of Phase 3 Approval Protocol v1.3*  
*Submitted for user review. No corpus created. No corpus execution authorized.*
