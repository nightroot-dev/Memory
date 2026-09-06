# Phase 3 Approval Protocol v1.4 — Living Field Architecture

**Version:** 1.4  
**Supersedes:** v1.3 (`docs/phase3_approval_protocol_v1.3.md`, commit `663ba77`)  
**Status:** SUBMITTED FOR REVIEW — awaiting explicit user approval before corpus creation  
**Frozen Implementation Revision:** `bc6634047cb900f90fd9832a6cf83793e885d1dc` (Commit `bc66340`)  
**Frozen Implementation Archive:** `phase3-bc66340.zip` (2,818,768 bytes, SHA-256: `049AA1EBD657C8BCC2F30060C12E5931A402A8A346A9F56667AE32DEF62A6762`)  
**Pre-Flight Evidence Log:** `phase3_preflight_evidence.log` (25,395 bytes, SHA-256: `823C4EB4CEA4F4B696C631C99EEAC7E3D80102B9695F2F6453C6D1B4568F8C28`)  

> **MANDATORY GOVERNING CONSTRAINT:** No approval corpus exists. No approval corpus
> may be created, authored, inspected, audited, or executed until the user explicitly
> approves this document. All implementation code, diagnostic instrumentation, tests,
> overlap auditor, and verifier scripts are frozen at commit `bc66340` and authenticated below.

---

## Detailed Resolution of Review Findings (v1.3 → v1.4)

| Finding | Reviewer Finding on Protocol v1.3 | Full Resolution in Protocol v1.4 |
|---|---|---|
| **1. G5 Feasibility Framing** | Pseudo-derivation claimed G5 is "mathematically proven" by asserting cross-domain negatives won't enter top-5. This was an unproven pairwise-similarity assumption. | **Dropped "mathematically proven" claim entirely.** Section 5 explicitly classifies G5 as an **empirical acceptance gate with NO feasibility pre-guarantee**, evaluated strictly post-hoc from the single benchmark run. Role B is instructed to distribute scenarios across distinct operational domains to avoid artificial cross-query topical clustering. However, whether cross-domain negatives enter top-5 is an empirical property of the candidate embeddings and retrieval pipeline. If negative occupancy $\ge 0.40$ (upper 95% bootstrap CI), Gate G5 fails closed and the run is rejected. Threshold $0.40$ is strictly maintained. |
| **2. Passive Diagnostic Equivalence Verifiability** | Claimed test `synth_10_passive_diagnostic_equivalence` was in an inaccessible archive and unverifiable. | **Directly verifiable.** Archive `phase3-bc66340.zip` is now verified in repository root (`C:\Games\Projects\Memory\phase3-bc66340.zip`). The exact source code, structural assertions, and execution output of `synth_10_passive_diagnostic_equivalence` are reproduced verbatim in Section 1 and authenticated in `phase3_preflight_evidence.log`. |
| **3. Custody Boundary & Scaffolding** | Custody boundary had a gap regarding benchmark schema authoring and qualitative review judgments. | **Explicit authoring boundary.** Section 2 and Section 4.1 specify that the Rust scaffolding (struct/enum types `ApprovalScenario`, `ScenarioClass`, `ExpectedWinner`, runner harness, and `PHASE3_RESULT|...` formatter) was authored by Role A and frozen at `bc66340` with **zero scenario data, zero query strings, and zero hints**. Role B receives only this structural scaffolding, the frozen protocol, and the scenario taxonomy. Role B fills in **data only** (the `SCENARIOS` slice), never touching structure, types, scoring, or execution logic. Role B independently performs and certifies the Blinded Qualitative Review checklist (Section 3, Rule 5) without consultation from Role A. |
| **4. Manifest & Archive Independence** | Archive `phase3-bc66340.zip` was not present in the reviewer's upload location; hashes and test counts were asserted rather than verifiable. | **Verified locally & independently.** `phase3-bc66340.zip` is placed in `C:\Games\Projects\Memory\phase3-bc66340.zip` (2,818,768 bytes, SHA-256: `049AA1EBD657C8BCC2F30060C12E5931A402A8A346A9F56667AE32DEF62A6762`). `phase3_preflight_evidence.log` (25,395 bytes, SHA-256: `823C4EB4CEA4F4B696C631C99EEAC7E3D80102B9695F2F6453C6D1B4568F8C28`) records line-by-line raw outputs for all 10 core synthetic tests, 50 MCP tool tests, and 4 overlap auditor self-tests. |

---

## 1. Passive Diagnostic Equivalence Proof & Test Verification

Protocol Step 2 requires exposing candidate pool stages (`raw_nn_candidates`, `post_filter_candidates`, `scored_candidates`) for diagnostic observability. To prove this instrumentation is strictly passive with **zero side effects** on functional retrieval outcomes, test `synth_10_passive_diagnostic_equivalence` was implemented in `cmhm_core/tests/phase3_synthetic_eval_tests.rs` (lines 394–457) at frozen commit `bc66340`:

```rust
// cmhm_core/tests/phase3_synthetic_eval_tests.rs: lines 394-457
#[test]
fn synth_10_passive_diagnostic_equivalence() {
    let (mut rt, _dir) = make_rt("s10");
    let ctx = make_ctx(&mut rt, "synth10");

    // Populate a controlled scenario:
    // Node 1: Base state
    let _n1 = learn_emb(&mut rt, &ctx, 10, 0.9, 0.80, "user_explicit", None, None, None);
    // Node 2: Superseded state (superseded by Node 3)
    let n2 = learn_emb(&mut rt, &ctx, 20, 0.9, 0.70, "user_explicit", None, None, None);
    // Node 3: Active successor superseding Node 2
    let n3 = learn_emb(&mut rt, &ctx, 20, 0.9, 0.90, "user_explicit", Some(n2), None, None);
    // Node 4: Distractor
    let n4 = learn_emb(&mut rt, &ctx, 30, 0.9, 0.50, "distractor", None, None, None);

    let (result, _) = rt.retrieve_unscoped(Some(&ctx), emb(20), "hash-128d")
        .expect("retrieve_unscoped");

    // Diagnostic validation:
    // 1. RAW_NN contains all nearest neighbors (including superseded node n2)
    assert!(
        result.raw_nn_candidates.contains(&n2),
        "RAW_NN diagnostic stage must capture superseded node {n2} before filtering"
    );
    assert!(
        result.raw_nn_candidates.contains(&n3),
        "RAW_NN diagnostic stage must capture active node {n3}"
    );

    // 2. POST_FILTER must strictly exclude superseded node n2
    assert!(
        !result.post_filter_candidates.contains(&n2),
        "POST_FILTER diagnostic stage must exclude superseded node {n2}"
    );
    assert!(
        result.post_filter_candidates.contains(&n3),
        "POST_FILTER diagnostic stage must retain active successor {n3}"
    );

    // 3. SCORED candidates must be sorted descending by composite score
    assert!(!result.scored_candidates.is_empty(), "SCORED candidates must not be empty");
    let mut prev_score = f32::INFINITY;
    for &(id, score) in &result.scored_candidates {
        assert_ne!(id, n2, "Superseded node {n2} must never appear in SCORED stage");
        assert!(score <= prev_score + 1e-6, "SCORED stage must be descending: {score} > {prev_score}");
        prev_score = score;
    }

    // 4. Functional invariants: highest scored candidate must be the selected seed
    let (top_id, top_score) = result.scored_candidates[0];
    assert_eq!(
        result.seed_id, Some(top_id),
        "Selected seed_id must match top SCORED candidate exactly"
    );
    assert_eq!(
        result.seed_id, Some(n3),
        "Node {n3} must win over superseded {n2} and distractor {n4}"
    );

    println!(
        "[SYNTH-10 PASS] Diagnostic instrumentation verified strictly passive and mathematically equivalent (seed_id={:?}, score={:.4})",
        result.seed_id, top_score
    );
}
```

**Execution Result (authenticated in `phase3_preflight_evidence.log`):**
```
test synth_10_passive_diagnostic_equivalence ... ok
[SYNTH-10 PASS] Diagnostic instrumentation verified strictly passive and mathematically equivalent (seed_id=Some(7), score=1.4248)
```

---

## 2. Custody Boundary Architecture & Authoring Order

To eliminate implicit knowledge transfer and ensure genuine held-out evaluation, custody and execution are strictly partitioned:

```
┌────────────────────────────────────────────────────────────────────────┐
│ Role A: Implementation Author (Coding Assistant)                       │
│                                                                        │
│ 1. Implements core engine, passive diagnostics, tests, and scripts.    │
│ 2. Authors empty Rust benchmark scaffolding (types and harness only,   │
│    zero scenario data, zero query strings, zero hints) frozen at       │
│    bc66340 before Role B authoring begins.                             │
│ 3. Strictly barred from authoring, viewing, or inspecting queries.     │
│ 4. Does not execute the approval benchmark.                            │
│ 5. Receives only final sanitized evidence log and archive for review.  │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Empty Scaffolding + Frozen Protocol v1.4
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Role B: Independent Corpus Author & Execution Custodian                │
│ (Human User or Independent Isolated Operator)                          │
│                                                                        │
│ 1. Receives ONLY Protocol v1.4, the Taxonomy, and Empty Scaffolding.   │
│ 2. Never accesses dprime_bench.rs or phase3_held_out_bench.rs.         │
│ 3. Fills in DATA ONLY (queries, assertions, expected outcomes into the │
│    SCENARIOS slice) following Section 4; never touches structure.      │
│ 4. Independently performs and certifies the Blinded Qualitative Review │
│    checklist (Section 3, Rule 5) without consultation from Role A.     │
│ 5. Commits sealed benchmark file cmhm_mcp/tests/phase3_approval_bench  │
│ 6. Executes scripts/audit_corpus_overlap.ps1 (Stage 1).                │
│ 7. If audit passes, executes scripts/verify_phase3_approval.ps1 (Stg 2)│
│ 8. Retains sealed logs; provides only sanitized evidence log to Role A.│
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
   Role B independently certifies via a predeclared checklist that each scenario tests an independent operational failure mode, distinct root cause, and distinct mitigation path from any scenario in the prohibited corpora. Role A is not consulted and does not participate in this review.

---

## 4. Scenario-Class Taxonomy & Benchmark Scaffolding

### 4.1 Frozen Rust Scaffolding Contract
Role A authored the following empty structural scaffolding (frozen at `bc66340`), which defines the types, execution loop, scoring computation, and structured output formatting for `cmhm_mcp/tests/phase3_approval_bench.rs`. Role B populates ONLY the `SCENARIOS` slice:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioClass {
    Contradiction,
    Supersession,
    AuthorityPolarity,
    AdversarialAuthority,
    SafeAbstention,
    UnresolvedConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedConflict {
    LogicalContradiction,
    AuthorityPolarity,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedWinner {
    Pos,
    Neg,
    None,
}

pub struct ApprovalScenario {
    pub id: usize,
    pub domain: &'static str,
    pub class: ScenarioClass,
    pub query: &'static str,
    pub pos_text: &'static str,
    pub pos_auth: f32,
    pub pos_prov: &'static str,
    pub pos_key: Option<&'static str>,
    pub pos_polarity: Option<i8>,
    pub neg_text: &'static str,
    pub neg_auth: f32,
    pub neg_prov: &'static str,
    pub neg_key: Option<&'static str>,
    pub neg_polarity: Option<i8>,
    pub supersedes_neg: bool,
    pub expected_conflict: ExpectedConflict,
    pub expected_abstain: bool,
    pub expected_winner: ExpectedWinner,
}

// ROLE B POPULATES ONLY THIS SLICE (MINIMUM 40 SCENARIOS).
// ZERO STRUCTURAL OR HARNESS LOGIC CHANGES ARE PERMITTED.
pub static SCENARIOS: &[ApprovalScenario] = &[
    // Role B fills scenario instances here
];
```

### 4.2 Corpus Volume and Predeclared Counts
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

### 4.3 Background Distractors
Each scenario context must contain $\ge 60$ background distractor nodes generated deterministically using seed `DISTRACTOR_RNG_SEED = 0xA3F72B91C5D4E608u64`.

### 4.4 Complete Scenario-Class Matrix

| Class Name | Min Count | D− Authority / Provenance | D+ Authority / Provenance | D− in RAW_NN? | D− survives Step 2a? | Expected Conflict Type | Expected Abstain | Expected Winner | Evaluating Gates |
|---|---|---|---|---|---|---|---|---|---|
| **CONTRADICTION** | 10 | 0.70–0.85 / `contradicted` (same assertion_key, polarity -1) | 0.85 / `user_explicit` (same assertion_key, polarity +1) | **Yes** | **Yes** | `LogicalContradiction` | `true` | None (abstained) | G1–G6, G8a, S1, G10 |
| **SUPERSESSION** | 6 | 0.85 / `contradicted` (superseded by D+) | 0.90 / `user_explicit` (`supersedes = Some(D-_id)`) | **Yes** | **NO** (pruned) | `None` | `false` | $D^+$ | G1–G6, G7 |
| **AUTHORITY-POLARITY**| 6 | 0.75 / `contradicted` (different/no assertion key) | 0.85 / `user_explicit` or `corrected` | **Yes** | **Yes** | `AuthorityPolarity` | `true` | None (abstained) | G1–G6, G8b, G10 |
| **ADVERSARIAL-AUTHORITY**| 6 | 0.95 / `contradicted` (elevated authority) | 0.80 / `user_explicit` (standard authority) | **Yes** | **Yes** | `AuthorityPolarity` | `true` | None ($D^-$ never wins) | G1–G6, G8b, G10 |
| **SAFE-ABSTENTION** | 6 | 0.80 / `contradicted` (near-equal score gap < 0.02) | 0.85 / `user_explicit` | **Yes** | **Yes** | `LogicalContradiction` or `AuthorityPolarity` | `true` | None (abstained) | G1–G6, G9, G10 |
| **UNRESOLVED-CONFLICT**| 6 | 0.80 / `user_explicit` (identical provenance, no polarity clash) | 0.85 / `user_explicit` | **Yes** | **Yes** | `None` | `false` | $D^+$ (higher score) | G1–G6, G8c, G10 |

---

## 5. Empirical Nature of Gate G5 and Strict Post-Hoc Evaluation

### 5.1 Formal Definition of Hard-Negative Occupancy@5
Hard-Negative Occupancy@5 measures the proportion of total top-5 candidate slots across all queries occupied by designated hard negatives:

$$\text{Occupancy@5} = \frac{\sum_{i=1}^N \sum_{k=1}^5 \mathbb{I}(\text{candidate at rank } k \text{ of query } i \in \mathcal{D}^-)}{5 \times N}$$

For $N = 40$ queries, the denominator is exactly $5 \times 40 = 200$ slots. $\mathcal{D}^-$ represents the set of all designated hard negatives across the entire corpus.

### 5.2 Empirical Character & Zero Feasibility Pre-Guarantee
1. **Empirical Gate:** Gate G5 is an **empirical evaluation gate with NO feasibility pre-guarantee**. The protocol does not assert or claim a mathematical proof that cross-domain negatives will not enter the top-5.
2. **Corpus Construction Hygiene:** As an operational guideline, Role B is instructed to distribute scenarios across distinct, disjoint operational domains (Section 4.2) to avoid artificial cross-query topical clustering.
3. **Strict Post-Hoc Verification:** Whether cross-domain hard negatives enter the top-5 for query $i$ is an empirical property of the candidate query texts, the deterministic embedding model (`hash-128d`), and the composite scoring engine.
4. **Conservative Upper Bound Enforcement:** Evaluated via 10,000 bootstrap resamples on the 200 candidate slots. Gate G5 requires the **upper 95% bootstrap confidence bound** to be strictly $< 0.40$. If cross-domain collisions, dedicated negatives, or dense clustering cause the upper bound to equal or exceed $0.40$, Gate G5 fails closed and the run is rejected. The threshold will not be relaxed, reinterpreted, or weakened.

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

---

## 7. Predeclared 5-Way Selection Decision Outcome Categorization

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
Archive: `phase3-bc66340.zip` (2,818,768 bytes, SHA-256: `049AA1EBD657C8BCC2F30060C12E5931A402A8A346A9F56667AE32DEF62A6762`)  
Manifest generated directly by `scripts/generate_freeze_manifest.ps1`:

| File Relative Path | Size (Bytes) | SHA-256 Checksum |
|---|---|---|
| `cmhm_core/src/retrieval.rs` | 21,380 | `5243A41869776FE0E78AE458FA0796E01C69644BE787487E5FF3C5B07C5C0A81` |
| `cmhm_core/src/engine.rs` | 57,100 | `F4BBF63D2A12D31DF7BE394F9EFD8B899E112B448972B64D9B5A2A3D0A89B066` |
| `cmhm_core/src/context/runtime.rs` | 30,015 | `563EAFEE66E0D93EEF3F1C82DC976B443020D62A311A53E8119C13EEC4297789` |
| `cmhm_core/src/text_embed.rs` | 7,489 | `A6E2A21D483E6F048FE236D4CFDF842AE07E3EF9C8EE250C4D648AB3E71E89B3` |
| `cmhm_mcp/src/schema.rs` | 17,171 | `BE27A3E4E4956F0C589409A9AE7FD06DF8BD71F13691A4FD63CAFF744F0380FC` |
| `cmhm_mcp/src/tools.rs` | 60,975 | `869FEC6D45421C09266C11BDA395C347D2E2414C29ABFA0AB9E1157C4EE400A9` |
| `cmhm_mcp/src/lib.rs` | 1,451 | `F5E09E80B4756F309EF8FB7467058D1C09E7B1FEAD51DC29A40F6F00C65E6845` |
| `cmhm_mcp/tests/tool_tests.rs` | 58,624 | `806BE9F9FC611C32E65AA9AC6B4BA6DA6939B2BB94F29721A2D80DC41124706E` |
| `cmhm_core/tests/phase3_synthetic_eval_tests.rs` | 18,811 | `5D0C483C9258BAD34DC62364A16C8833BD798AA2C025563A1069F43850416AC0` |
| `scripts/audit_corpus_overlap.ps1` | 12,533 | `E3C8BE2FF91E5083864769F1189070846BD5808B7204161C3956A630CB7CCB59` |
| `scripts/verify_phase3_approval.ps1` | 19,385 | `519EA3AC53FE3E73768B950352AEDCF0437D3044C219E45EC45958BE2EEA94D2` |

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

*End of Phase 3 Approval Protocol v1.4*  
*Submitted for user review. No corpus created. No corpus execution authorized.*
