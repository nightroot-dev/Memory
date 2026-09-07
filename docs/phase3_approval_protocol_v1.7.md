# Phase 3 Approval Protocol v1.7

**Version:** 1.7  
**Supersedes:** v1.6 (`b3ee383e796b26291cb6a474ffaf12110a8a7ae566c36fdec8d03e90dea4e22a`)  
**Scaffold freeze (v1.6):** `984bef7525454c601f6fcda94a32359fba27577d`  
**Scaffold freeze (v1.7):** to be set when conformance test committed  
**Status:** GOVERNING DOCUMENT -- all prior protocol versions superseded

---

## 1. Purpose and Scope

This protocol governs Phase 3 approval for the CMHM (Contextual Hierarchical Memory Model) system. Phase 3 verifies that the retrieval-safety properties claimed by Phase 2 hold on a fresh, independent corpus under pre-declared, immutable acceptance gates.

Phase 3 does not re-measure Phase 2 metrics; it measures operational retrieval safety across six independently authored scenario classes using the production `retrieve_unscoped` path and `hash-128d` model identifier.

---

## 2. Role Assignments and Custody Model

Role A (coding assistant / implementation author):
- Authored the implementation frozen at bc66340
- Authors the benchmark scaffold (structural harness only)
- Mechanically transcribes Role B scenarios into SCENARIOS
- Runs pre-flight, conformance check, and overlap audit only
- Strictly barred from authoring, viewing, or inspecting scenario content before or after any approval run
- Issues the explicit Stage 2 go-ahead after Stage 1.5 passes

Role B (user / independent corpus custodian -- Charan):
- Authors all scenario content (query, pos_text, neg_text, all authority values, provenance strings, keys, polarities)
- Verifies Role A mechanical transcription against plaintext
- Issues corpus authorization (only party who can approve)
- Executes Stage 2 on Role A go-ahead

Reviewer (independent AI, cloud sandbox):
- Verifies protocol documents and hashes
- Recused from Role B (corpus authoring) due to source-level audit exposure to phase3_held_out_bench.rs
- Retains post-run review and hash verification role

CRITICAL CUSTODY CONSTRAINT: Role A does not see corpus content before or after any approval run. This holds even when the corpus is spent.

RECORDED ROLE A CONTAMINATION (attempt 1, post-run): Role A read and cited corpus authority value ranges in post-run diagnostic output. This contamination is irreversible and is recorded for custody purposes. Role A is further excluded from any future corpus authoring or selection process.

---

## 3. Resolution Table

| # | Finding | Status | Resolution |
|---|---|---|---|
| 1 | v1.4 diagram labels stale (Role A/B inverted) | Resolved | Fixed in v1.5 Section 2 |
| 2 | tools.rs manifest hash stale at v1.4 | Resolved | Correct hash in v1.5 Section 8 |
| 3 | CRLF archive entries caused non-reproducible manifest hashes | Resolved | .gitattributes * text=lf eol=lf added; archive regenerated LF-clean |
| 4 | Stale v1.4 labels in v1.5 Section 2 diagram | Resolved | Both labels corrected in v1.6 Section 2 |
| 5 | Scaffold modified post-freeze (bc66340 did not include bench file) | Resolved | v1.6 Section 10.1: scaffold separately frozen at 984bef7 |
| 6 | Role B recusal (reviewer source-level audit exposure) | Resolved | v1.6 Section 10.2: Charan confirmed as Role B |
| 7 | 140 authored scenarios exceeded "exactly 40" + domain ceiling | Resolved | 40 selected (orig IDs 1-37, 57, 62, 67); all class minimums and domain cap satisfied |
| 8 | Scaffold g5_slots = 200 hardcoded (not 5 * N) | Resolved | Fixed to 5 * all_n in commit 1b07c83; no gate logic changed |
| 9 | Attempt-1 corpus non-conformant to Section 4.4 in three class categories | Resolved | Recorded in Section 11; Section 12 conformance validator prevents recurrence |
| 10 | Harness gap: no Section 4.4 conformance check before Stage 2 | Resolved | Section 12 conformance validator added as Stage 1.5; runs before attempt is consumed |
| 11 | Role A post-run breach: read corpus auth ranges; issued tuning guidance | Recorded | Guidance retracted; breach recorded in Sections 2 and 11; irreversible |
| 12 | Attempt-1 ruling: INVALID ON ITS FACE -- non-conformant corpus; gate failures attributable to corpus defect, not engine | Recorded | Full record in Section 11; engine neither approved nor disapproved |

---

## 4. Corpus Requirements

### 4.1 Size and Domain Distribution

The approval corpus must contain exactly N scenarios (N >= 40) spanning at least 8 operational domains, with no domain exceeding 6 queries.

### 4.2 Class Count Minimums

| Class | Minimum |
|---|---|
| Contradiction | >= 10 |
| Supersession | >= 6 |
| AuthorityPolarity | >= 6 |
| AdversarialAuthority | >= 6 |
| SafeAbstention | >= 6 |
| UnresolvedConflict | >= 6 |
| Total | >= 40 |

### 4.3 Prohibited Corpora

Permanently prohibited from any reuse of query text, positive text, or negative text:

1. cmhm_core/tests/dprime_bench.rs -- 15-query rejection set
2. cmhm_core/tests/phase3_held_out_bench.rs -- 32-query compromised corpus

SOFT PROHIBITION (attempt-1 spent corpus): The 40 query strings from attempt-1 corpus (commit e989a91) are not formally prohibited by the overlap auditor, but corpus v2 must be authored fresh and uncontaminated by attempt-1 diagnostics. Role B must not reuse attempt-1 query texts.

### 4.4 Complete Scenario-Class Matrix

This table is the binding class specification. Every cell is enforced by the Section 12 conformance validator before Stage 2.

| Class Name | Min Count | D- Authority / Provenance | D+ Authority / Provenance | D- in RAW_NN? | D- survives Step 2a? | Expected Conflict Type | Expected Abstain | Expected Winner | Evaluating Gates |
|---|---|---|---|---|---|---|---|---|---|
| CONTRADICTION | 10 | 0.70-0.85 / contradicted (same assertion_key, polarity -1) | >=0.80 / user_explicit (same assertion_key, polarity +1) | Yes | Yes | LogicalContradiction | true | None (abstained) | G1-G6, G8a, S1, G10 |
| SUPERSESSION | 6 | < D+ auth / contradicted (superseded by D+) | >= (D- auth + 0.05) / user_explicit (supersedes_neg: true) | Yes | NO (pruned) | None | false | D+ | G1-G6, G7 |
| AUTHORITY-POLARITY | 6 | 0.70-0.80 / contradicted (DIFFERENT or absent assertion_key) | 0.80-0.90 / user_explicit or corrected | Yes | Yes | AuthorityPolarity | true | None (abstained) | G1-G6, G8b, G10 |
| ADVERSARIAL-AUTHORITY | 6 | 0.90-1.00 / user_explicit (elevated, adversarial) | 0.75-0.85 / corrected (standard authority) | Yes | Yes | AuthorityPolarity | true | None (D- never wins) | G1-G6, G8b, G10 |
| SAFE-ABSTENTION | 6 | 0.75-0.87 / contradicted (|pos_auth - neg_auth| <= 0.10) | 0.75-0.90 / user_explicit | Yes | Yes | LogicalContradiction or AuthorityPolarity | true | None (abstained) | G1-G6, G9, G10 |
| UNRESOLVED-CONFLICT | 6 | 0.75-0.85 / user_explicit (identical provenance, no polarity clash) | 0.80-1.00 / user_explicit | Yes | Yes | None | false | D+ (higher score) | G1-G6, G8c, G10 |

Class definition notes:

CONTRADICTION: D+ and D- share the same assertion_key. D+ polarity +1; D- polarity -1. D- auth 0.70-0.85 ensures composite score proximity with D+ for S1 eligibility (gap < 0.02 in SCORED top-2).

SUPERSESSION: supersedes_neg = true. D- enters RAW_NN but is eliminated by Step 2a supersession filter before SCORED ranking.

AUTHORITY-POLARITY: D- must have a DIFFERENT assertion_key from D+ (or neg_key = None). Same key with contradicted provenance routes to LogicalContradiction, not AuthorityPolarity -- this was the defect in attempt-1 corpus.

ADVERSARIAL-AUTHORITY: D- carries elevated authority (0.90-1.00) exceeding D+ (0.75-0.85). Engine must abstain rather than select the high-authority-but-wrong D-.

SAFE-ABSTENTION: Both D+ and D- have similar authority (|gap| <= 0.10); neg_prov = "contradicted" required.

UNRESOLVED-CONFLICT: Both user_explicit provenance, same polarity (no clash). Engine selects D+ cleanly without abstaining.

---

## 5. Empirical Nature of Gate G5

Gate G5 is an empirical evaluation gate with NO feasibility pre-guarantee. Evaluated via 10,000 bootstrap resamples. Upper 95% bootstrap confidence bound must be strictly < 0.40. The threshold will not be relaxed, reinterpreted, or weakened.

Occupancy@5 = (total D- in top-5 slots) / (5 * N)

---

## 6. Acceptance Gates, Populations, and Stage Invariants

All gates evaluate the external MCP production path (handle_retrieve_text with omitted retrieval_intent).

| Gate | Metric | Numerator | Denominator | Threshold | CI Condition |
|---|---|---|---|---|---|
| G1 | Recall@1 | count(pos_rank_SCORED = 1) | Full corpus N=40 | >= 0.40 | Lower 95% CI >= 0.40 |
| G2 | Recall@5 | count(pos_in_top5_SCORED) | Full corpus N=40 | >= 0.67 | Lower 95% CI >= 0.67 |
| G3 | MRR | Sum 1/pos_rank (no cap; 0 if absent) | Full corpus N=40 | >= 0.40 | Lower 95% CI >= 0.40 |
| G4 | Contradiction-at-1 | count(neg_rank_SCORED = 1) | Full corpus N=40 | < 0.40 | UPPER 95% CI < 0.40 |
| G5 | Hard-Neg Occupancy@5 | Total D- in top-5 slots | 5 * N slots | < 0.40 | UPPER 95% CI < 0.40 |
| G6 | Mean Margin | Sum (S(D+) - S(D-)) (absent = 0.0) | Full corpus N=40 | > 0.00 | Lower 95% CI > 0.00 |
| G7 | Supersession Safety | Count superseded D- in post_filter/scored/conflict/chain/winner | SUPERSESSION (N >= 6) | = 0.00 | Exact (0 violations) |
| G8a | LogicalContradiction Rate | count(conflict_type = LogicalContradiction) | Eligible CONTRADICTION (S1) | >= 0.80 | Exact (>= 80%) |
| S1 | LC Eligibility Rate | D+/D- reach SCORED top-2 with gap < 0.02 | All CONTRADICTION (N >= 10) | >= 0.60 | Exact (>= 60%) |
| G8b | AuthorityPolarity Rate | count(conflict_type = AuthorityPolarity) | AUTH_POLARITY + ADVERSARIAL (N >= 12) | >= 0.80 | Exact (>= 80%) |
| G8c | Unresolved Non-Conflict | count(conflict_type = None) | UNRESOLVED-CONFLICT (N >= 6) | >= 0.80 | Exact (>= 80%) |
| G9 | Safe Abstention Rate | count(abstained = true) | SAFE-ABSTENTION (N >= 6) | >= 0.80 | Exact (>= 80%) |
| G10 | Pool Contamination | Count where D- absent from RAW_NN | All 34 non-superseded scenarios | = 0 | Exact (0 errors) |

GOVERNING CONSTRAINT: No gate may be removed, weakened, or reinterpreted after observing a failure.

---

## 7. Predeclared 5-Way Outcome Categorization

1. selected_d_plus: abstained = false AND winner_id = Some(pos_id)
2. selected_d_minus: abstained = false AND winner_id = Some(neg_id)
3. selected_other: abstained = false AND winner_id is neither D+ nor D-
4. correct_abstention: abstained = true AND scenario.expected_abstain = true
5. incorrect_abstention: abstained = true AND scenario.expected_abstain = false

---

## 8. Frozen Artifact Cryptographic Manifest

Implementation freeze commit: bc6634047cb900f90fd9832a6cf83793e885d1dc  
Archive: phase3-bc66340-lf.zip (2,812,990 bytes, SHA-256: 7D91A8B38F5D96123B6A5C312EDD99B2D5D16D890FAD079380B062EFEFC5DF70)  
Line-ending policy: LF only. Hashes are raw-byte hashes of extracted archive entries.

| File | Size | SHA-256 |
|---|---|---|
| cmhm_core/src/retrieval.rs | 21,380 | 5243A41869776FE0E78AE458FA0796E01C69644BE787487E5FF3C5B07C5C0A81 |
| cmhm_core/src/engine.rs | 57,100 | F4BBF63D2A12D31DF7BE394F9EFD8B899E112B448972B64D9B5A2A3D0A89B066 |
| cmhm_core/src/context/runtime.rs | 30,015 | 563EAFEE66E0D93EEF3F1C82DC976B443020D62A311A53E8119C13EEC4297789 |
| cmhm_core/src/text_embed.rs | 7,489 | A6E2A21D483E6F048FE236D4CFDF842AE07E3EF9C8EE250C4D648AB3E71E89B3 |
| cmhm_mcp/src/schema.rs | 17,171 | BE27A3E4E4956F0C589409A9AE7FD06DF8BD71F13691A4FD63CAFF744F0380FC |
| cmhm_mcp/src/tools.rs | 59,596 | 0B4A5F85F4A714B7722032A8F291141186142D9E3E1625871C31953575601449 |
| cmhm_mcp/src/lib.rs | 1,451 | F5E09E80B4756F309EF8FB7467058D1C09E7B1FEAD51DC29A40F6F00C65E6845 |
| cmhm_mcp/tests/tool_tests.rs | 58,624 | 806BE9F9FC611C32E65AA9AC6B4BA6DA6939B2BB94F29721A2D80DC41124706E |
| cmhm_core/tests/phase3_synthetic_eval_tests.rs | 18,811 | 5D0C483C9258BAD34DC62364A16C8833BD798AA2C025563A1069F43850416AC0 |
| scripts/audit_corpus_overlap.ps1 | 12,533 | E3C8BE2FF91E5083864769F1189070846BD5808B7204161C3956A630CB7CCB59 |
| scripts/verify_phase3_approval.ps1 | 19,385 | 519EA3AC53FE3E73768B950352AEDCF0437D3044C219E45EC45958BE2EEA94D2 |

Post-freeze harness changes (outside manifest, do not affect the implementation under test):

| File | Change | Commit |
|---|---|---|
| cmhm_mcp/tests/phase3_approval_bench.rs | Scaffold authored; corpus transcribed; g5_slots defect fixed | 984bef7, 1b07c83 |
| scripts/audit_corpus_overlap.ps1 | PS empty-List unrolling bug fixed; pos_text/neg_text field aliases added | e989a91 |

---

## 9. Multi-Stage Lifecycle and The One-Run Rule

Stage 0 (Pre-Flight): Clean working tree + synthetic self-tests. Failures do not consume an attempt.

Stage 1 (Overlap Audit): scripts/audit_corpus_overlap.ps1 against prohibited corpora. Any violation rejects corpus and terminates the attempt before Stage 2.

Stage 1.5 (Conformance Validation): phase3_conformance_check Rust test (Section 12). Validates every scenario against the Section 4.4 class-spec matrix. Any violation fails CLOSED before Stage 2. The attempt is NOT consumed by a Stage 1.5 failure. This stage was absent in attempt 1 -- its absence is what enabled a non-conformant corpus to reach Stage 2.

Stage 2 (Single Benchmark Execution): scripts/verify_phase3_approval.ps1. PERMANENTLY CONSUMES the single approval attempt. No retries permitted.

Stage 3 (Gate Verification): Verifier evaluates all gates; exits 0 only on unanimous passage.

---

## 10. Protocol v1.6 Addendum -- Scaffold Freeze Identity and Role B Ruling (Carried Forward)

### 10.1 Scaffold Freeze Identity

phase3_approval_bench.rs did not exist at implementation freeze bc66340. It is separately frozen:

| Artifact | Value |
|---|---|
| File | cmhm_mcp/tests/phase3_approval_bench.rs |
| Scaffold freeze commit (v1.6) | 984bef7525454c601f6fcda94a32359fba27577d |
| SHA-256 (v1.6) | 22D9DD22D2CACF0EA444730BA196297ED2C895008973A4C895641F66097DA187 |
| Size (v1.6) | 25,490 bytes |

The v1.7 scaffold freeze identity will be set to the commit that adds the Section 12 conformance test and clears the SCENARIOS slice for attempt 2.

### 10.2 Role B Ruling

The independent reviewer is disqualified from Role B due to source-level audit exposure to phase3_held_out_bench.rs queries. Charan (the user) is confirmed as Role B for all corpus authoring.

---

## 11. Attempt 1 Record (Permanent and Immutable)

Nothing in this section may be altered in any subsequent protocol revision.

### 11.1 Attempt Facts

| Field | Value |
|---|---|
| Attempt number | 1 |
| Corpus commit | e989a913f56f483a63486e7d795318ace8b77e3a |
| Corpus size | 40 scenarios (orig IDs 1-37, 57, 62, 67 from Role B 140-scenario submission) |
| Overlap audit | PASS -- Max Jaccard 0.0556 < 0.30; zero verbatim violations |
| Gate results | G1 PASS G2 PASS G3 PASS G4 PASS G5 PASS G6 PASS G7 PASS S1 FAIL G8a PASS G8b FAIL G8c PASS G9 FAIL G10 PASS |
| Outcome decomposition | selected_d_plus=35, selected_d_minus=5, correct_abstention=0, incorrect_abstention=0 |

### 11.2 Ruling: INVALID ON ITS FACE

Not rejected on the merits.

The corpus violated the frozen Section 4.4 class-spec matrix before Stage 2 executed:

| Class | Field | Required | Actual |
|---|---|---|---|
| Contradiction | D- authority | 0.70-0.85 | 0.38-0.55 |
| AuthorityPolarity | D- provenance | contradicted | user_explicit |
| AuthorityPolarity | D- key rule | different or absent from D+ | same key as D+ |
| SafeAbstention | D- provenance | contradicted | user_explicit |

The S1/G8b/G9 failures were structurally guaranteed by these defects:

S1 (0/10): D- auth 0.38-0.55 places composite scores far outside the conflict-detection band. S1 eligibility (gap < 0.02) was unreachable regardless of engine behavior.

G8b (0/12): Same assertion_key + user_explicit provenance routes the engine to LogicalContradiction or no-conflict classification, not AuthorityPolarity. The AuthorityPolarity detection path was never reachable.

G9 (0/6): user_explicit provenance on D- gives the engine no authority-provenance conflict signal. SafeAbstention abstention was unreachable.

ENGINE VERDICT: The engine is neither approved nor disapproved by attempt 1. Gates G1-G7, G8a, G8c, G10 passed, confirming correct retrieval ranking and supersession machinery. No inference may be drawn about conflict detection or abstention behavior.

### 11.3 Root Cause

The conformance deviations originated in Role B plaintext authoring -- the predictable result of authoring a dense spec matrix by hand without a validator. The structural lesson: the harness must validate conformance before consuming the attempt. The harness gap is closed by Section 12.

### 11.4 Role A Post-Run Breach

Role A read the corpus Rust file post-run and cited specific authority value ranges in diagnostic output. Role A then issued a proposed corpus fix ("neg authority values should be closer to pos values") derived from this run's diagnostics -- exactly the tuning the protocol prohibits.

Both actions are permanently recorded:
- Role A is further contaminated for any future corpus authoring or content-inspection role.
- The tuning guidance is retracted unconditionally and has no standing in any future corpus design.

---

## 12. Pre-Run Conformance Validator Specification

### 12.1 Purpose

Closes harness gap (finding 10). Validates every scenario against the Section 4.4 matrix before Stage 2 executes. A conformance failure terminates the attempt before the single run is consumed.

### 12.2 Implementation

A Rust test function `phase3_conformance_check` in cmhm_mcp/tests/phase3_approval_bench.rs.

The verify script calls it as Stage 1.5:

  cargo test --package cmhm_mcp --test phase3_approval_bench phase3_conformance_check -- --nocapture

Any CONFORMANCE_CHECK|FAIL line or non-zero exit causes the verify script to print:

  FATAL: Conformance check failed -- attempt NOT consumed

and exit non-zero.

### 12.3 Per-Scenario Validation Rules

Failures panic with: CONFORMANCE FAIL id=N class=X field=Y: VALUE -- EXPECTED

CONTRADICTION:
  pos_prov == "user_explicit"
  pos_polarity == Some(1)
  pos_auth >= 0.80
  neg_auth in [0.70, 0.85]
  neg_prov == "contradicted"
  neg_polarity == Some(-1)
  neg_key == pos_key (same assertion key)
  supersedes_neg == false

SUPERSESSION:
  pos_auth > neg_auth
  pos_prov == "user_explicit"
  neg_prov == "contradicted"
  neg_polarity == pos_polarity (same sign; no polarity clash)
  supersedes_neg == true

AUTHORITY-POLARITY:
  neg_auth in [0.70, 0.80]
  neg_prov == "contradicted"
  neg_key != pos_key OR neg_key == None
  supersedes_neg == false

ADVERSARIAL-AUTHORITY:
  neg_auth in [0.90, 1.00]
  pos_auth in [0.75, 0.85]
  neg_auth > pos_auth
  pos_prov == "corrected"
  supersedes_neg == false

SAFE-ABSTENTION:
  neg_auth in [0.75, 0.87]
  pos_auth in [0.75, 0.90]
  |pos_auth - neg_auth| <= 0.10
  neg_prov == "contradicted"
  supersedes_neg == false

UNRESOLVED-CONFLICT:
  pos_prov == "user_explicit"
  neg_prov == "user_explicit"
  pos_auth > neg_auth
  NOT (pos_polarity == Some(1) AND neg_polarity == Some(-1))
  NOT (pos_polarity == Some(-1) AND neg_polarity == Some(1))
  supersedes_neg == false

### 12.4 Global Corpus Validation Rules

1. SCENARIOS.len() >= 40
2. All id values unique
3. Per-class minimums: Contradiction >= 10; all others >= 6
4. No domain string appears > 6 times
5. All query strings unique (case-insensitive)
6. Validator self-integrity: total checked == SCENARIOS.len()

### 12.5 Validator Output Format

On success:
  CONFORMANCE_CHECK|PASS|N=40|classes=Contradiction:N,...|domains_ok=true

On any failure:
  CONFORMANCE_CHECK|FAIL|id=N|class=X|field=Y|value=V|expected=E

---

## 13. Corpus v2 Requirements

### 13.1 Fresh Corpus Requirement

Corpus v2 for attempt 2 must:
- Satisfy all Sections 4.1, 4.2, 4.3, and 4.4 requirements
- Have zero query-text reuse from attempt-1 corpus (commit e989a91)
- Be authored by Role B with no reference to attempt-1 gate diagnostics, score values, or Role A retracted guidance
- Be authored using only the Section 4.4 matrix and the cheat-sheet below as spec references

### 13.2 Authoring Cheat-Sheet

| Class | D- auth | D- prov | Key rule | D+ prov | Other |
|---|---|---|---|---|---|
| Contradiction | 0.70-0.85 | contradicted | SAME as D+ | user_explicit | D- polarity -1; D+ polarity +1 |
| Supersession | < D+ auth | contradicted | any | user_explicit | supersedes_neg: true |
| AuthorityPolarity | 0.70-0.80 | contradicted | DIFFERENT from D+ (or None) | user_explicit or corrected | No polarity clash needed |
| AdversarialAuthority | 0.90-1.00 | user_explicit | any | corrected | D+ auth 0.75-0.85; D- auth > D+ auth |
| SafeAbstention | 0.75-0.87 | contradicted | any | user_explicit | |D+ auth - D- auth| <= 0.10 |
| UnresolvedConflict | 0.75-0.85 | user_explicit | any | user_explicit | D+ auth > D- auth; no polarity clash |

The Section 12 conformance validator enforces every cell before consuming the attempt.

### 13.3 Scaffold Update Procedure for Attempt 2

Before attempt 2:
1. SCENARIOS slice cleared to placeholder comment
2. phase3_conformance_check test function added to scaffold
3. verify_phase3_approval.ps1 updated to call Stage 1.5 before Stage 2
4. Role A commits and reports new scaffold freeze identity (commit hash + SHA-256)
5. Role B verifies cleared scaffold and conformance test code
6. Cycle: Role B authors -> Role A transcribes -> Stage 0 -> Stage 1 -> Stage 1.5 -> Stage 2 (on Role A go-ahead)