# Phase 3 Approval Protocol v1.9

**Version:** 1.9
**Supersedes:** v1.8 (A9E8D8F17795D9457B3660881E715BBF77FEE869E34A0A4CCC935B196BDC70AA)
**Status:** DRAFT — PENDING REVIEWER APPROVAL before governing

---

## 1. Purpose and Scope

*(Unchanged from v1.8)*

This protocol governs Phase 3 approval for the CMHM (Contextual Hierarchical Memory Model) system. Phase 3 verifies that the retrieval-safety properties claimed by Phase 2 hold on a fresh, independent corpus under pre-declared, immutable acceptance gates.

Phase 3 does not re-measure Phase 2 metrics; it measures operational retrieval safety across six independently authored scenario classes using the production retrieve_unscoped path and hash-128d model identifier.

---

## 2. Role Assignments and Custody Model

*(Unchanged from v1.8)*

Role A (coding assistant / implementation author):
- Authored the implementation frozen at bc66340
- Authors the benchmark scaffold (structural harness only)
- Mechanically transcribes Role B scenarios into SCENARIOS
- Runs pre-flight, conformance check, and overlap audit only
- Strictly barred from authoring, viewing, or inspecting scenario content before or after any approval run
- Issues the explicit Stage 2 go-ahead after Stage 1.5 passes

Role B (user / independent corpus custodian — Charan):
- Authors all scenario content (query, pos_text, neg_text, all authority values, provenance strings, keys, polarities)
- Verifies Role A mechanical transcription against plaintext
- Issues corpus authorization (only party who can approve)
- Executes Stage 2 on Role A go-ahead

Reviewer (independent AI, cloud sandbox):
- Verifies protocol documents and hashes
- Recused from Role B (corpus authoring) due to source-level audit exposure to phase3_held_out_bench.rs
- Retains post-run review and hash verification role

CRITICAL CUSTODY CONSTRAINT: Role A does not see corpus content before or after any approval run. This holds even when the corpus is spent.

RECORDED ROLE A CONTAMINATION (attempt 1, post-run): Role A read corpus Rust file post-run and cited specific authority value ranges in diagnostic output. Role A also issued a proposed corpus fix derived from this run's diagnostics — exactly the tuning the protocol prohibits. Both actions are permanently recorded. The tuning guidance is retracted unconditionally.

---

## 3. Resolution Table

| # | Finding | Status | Resolution |
|---|---|---|---|
| 1 | v1.4 diagram labels stale (Role A/B inverted) | Resolved | Fixed in v1.5 Section 2 |
| 2 | tools.rs manifest hash stale at v1.4 | Resolved | Correct hash in v1.5 Section 8 |
| 3 | CRLF archive entries caused non-reproducible manifest hashes | Resolved | .gitattributes added; archive regenerated LF-clean |
| 4 | Stale v1.4 labels in v1.5 Section 2 diagram | Resolved | Both labels corrected in v1.6 Section 2 |
| 5 | Scaffold modified post-freeze (bc66340 did not include bench file) | Resolved | v1.6 Section 10.1: scaffold separately frozen at 984bef7 |
| 6 | Role B recusal (reviewer source-level audit exposure) | Resolved | v1.6 Section 10.2: Charan confirmed as Role B |
| 7 | 140 authored scenarios exceeded exactly-40 + domain ceiling | Resolved | 40 selected; all class minimums and domain cap satisfied |
| 8 | Scaffold g5_slots = 200 hardcoded (not 5 * N) | Resolved | Fixed to 5 * all_n in commit 1b07c83; no gate logic changed |
| 9 | Attempt-1 corpus non-conformant to Section 4.4 | Resolved | Recorded in Section 11; Section 12 validator prevents recurrence |
| 10 | Harness gap: no Section 4.4 conformance check before Stage 2 | Resolved | Section 12 conformance validator added as Stage 1.5 |
| 11 | Role A post-run breach: read corpus auth ranges; issued tuning guidance | Recorded | Guidance retracted; breach recorded in Sections 2 and 11 |
| 12 | Attempt-1 ruling: INVALID ON ITS FACE | Recorded | Full record in Section 11 |
| 13 | v1.7 Section 4.4: nine cells changed without disclosure or rationale | Resolved | Complete change log in Section 11.5 |
| 14 | v1.7 AdversarialAuthority D- prov silently changed | Resolved | Explained in Section 11.5; substantively correct |
| 15 | v1.7 Section 12.3 validator incomplete | Resolved | Section 12.3 rewritten to full 1:1 correspondence with Section 4.4 |
| 16 | Attempt-2 REJECTED on merits: conflict-detection and abstention subsystems never fired | Resolved | Recorded in Section 15; root cause in Section 14.1; remediated in Section 14 |
| 17 | conflict_detection_threshold = 0.02 never fires under hash-128d embeddings | Resolved | Section 14: score-gap proximity removed; per-signal metadata contract specified |
| 18 | abstain_on_conflict = false (default) prevents all safe abstention | Resolved | Section 14.6: production default changed to true |

---

## 4. Corpus Requirements

*(Unchanged from v1.8)*

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

1. cmhm_core/tests/dprime_bench.rs — 15-query rejection set
2. cmhm_core/tests/phase3_held_out_bench.rs — 32-query compromised corpus
3. Attempt-1 corpus (commit e989a91) — consumed; zero query reuse (enforced by auditor Source 3)
4. Attempt-2 corpus (commit 11c0a07) — consumed diagnostic corpus; zero query reuse (enforced by auditor Source 4, see Section 16.1 prerequisite 5)

### 4.4 Complete Scenario-Class Matrix

*(Unchanged from v1.8 — this section may not be altered in response to gate results)*

| Class | Min | D- Auth / Prov | D+ Auth / Prov | Key Rule | Polarity | supersedes_neg | Expected Conflict | Abstain | Evaluating Gates |
|---|---|---|---|---|---|---|---|---|---|
| CONTRADICTION | 10 | 0.70-0.85 / contradicted | >=0.80 / user_explicit | neg_key == pos_key (SAME) | D+ = +1, D- = -1 | false | LogicalContradiction | true | G1-G6, G8a, S1, G10 |
| SUPERSESSION | 6 | < (D+ auth - 0.05) / contradicted | > (D- auth + 0.05) / user_explicit | any | D- polarity == D+ polarity | TRUE | None | false | G1-G6, G7 |
| AUTHORITY-POLARITY | 6 | 0.70-0.80 / contradicted | 0.80-0.90 / user_explicit or corrected | neg_key DIFFERENT from pos_key (or None) | any | false | AuthorityPolarity | true | G1-G6, G8b, G10 |
| ADVERSARIAL-AUTHORITY | 6 | 0.90-1.00 / user_explicit | 0.75-0.85 / corrected | any | any | false | AuthorityPolarity | true | G1-G6, G8b, G10 |
| SAFE-ABSTENTION | 6 | 0.75-0.87 / contradicted | 0.75-0.90 / user_explicit | any | any | false | LogicalContradiction or AuthorityPolarity | true | G1-G6, G9, G10 |
| UNRESOLVED-CONFLICT | 6 | 0.75-0.85 / user_explicit | 0.80-1.00 / user_explicit | any | same (no polarity clash) | false | None | false | G1-G6, G8c, G10 |

Additional constraints (unchanged from v1.8, all enforced by Section 12 validator):

CONTRADICTION: D+ and D- share the same assertion_key; D+ polarity +1, D- polarity -1; D- auth 0.70-0.85.

SUPERSESSION: pos_auth >= neg_auth + 0.05. supersedes_neg = true. Same polarity on D+ and D-.

AUTHORITY-POLARITY: neg_key must be different from pos_key (or neg_key = None).

ADVERSARIAL-AUTHORITY: neg_auth > pos_auth. neg_prov = user_explicit.

SAFE-ABSTENTION: |pos_auth - neg_auth| <= 0.10. No ordering constraint between pos_auth and neg_auth.

UNRESOLVED-CONFLICT: Both user_explicit provenance. pos_auth > neg_auth. No polarity clash.

---

## 5. Empirical Nature of Gate G5

*(Unchanged from v1.8)*

Gate G5 is an empirical evaluation gate with NO feasibility pre-guarantee. Evaluated via 10,000 bootstrap resamples. Upper 95% bootstrap confidence bound must be strictly < 0.40.

Occupancy@5 = (total D- in top-5 slots) / (5 * N)

---

## 6. Acceptance Gates, Populations, and Stage Invariants

*(Unchanged from v1.8)*

GOVERNING CONSTRAINT: No gate may be removed, weakened, or reinterpreted after observing a failure.

| Gate | Metric | Numerator | Denominator | Threshold | CI Condition |
|---|---|---|---|---|---|
| G1 | Recall@1 | count(pos_rank_SCORED = 1) | Full corpus N | >= 0.40 | Lower 95% CI >= 0.40 |
| G2 | Recall@5 | count(pos_in_top5_SCORED) | Full corpus N | >= 0.67 | Lower 95% CI >= 0.67 |
| G3 | MRR | Sum 1/pos_rank | Full corpus N | >= 0.40 | Lower 95% CI >= 0.40 |
| G4 | Contradiction-at-1 | count(neg_rank_SCORED = 1) | Full corpus N | < 0.40 | UPPER 95% CI < 0.40 |
| G5 | Hard-Neg Occupancy@5 | Total D- in top-5 slots | 5 * N slots | < 0.40 | UPPER 95% CI < 0.40 |
| G6 | Mean Margin | Sum (S(D+) - S(D-)) | Full corpus N | > 0.00 | Lower 95% CI > 0.00 |
| G7 | Supersession Safety | Count superseded D- in post_filter/scored/conflict/chain/winner | SUPERSESSION (N >= 6) | = 0.00 | Exact (0 violations) |
| G8a | LogicalContradiction Rate | count(conflict_type = LogicalContradiction) | Eligible CONTRADICTION (S1) | >= 0.80 | Exact (>= 80%) |
| S1 | LC Eligibility Rate | D+/D- reach SCORED top-2 with gap < 0.02 | All CONTRADICTION (N >= 10) | >= 0.60 | Exact (>= 60%) |
| G8b | AuthorityPolarity Rate | count(conflict_type = AuthorityPolarity) | AUTH_POLARITY + ADVERSARIAL (N >= 12) | >= 0.80 | Exact (>= 80%) |
| G8c | Unresolved Non-Conflict | count(conflict_type = None) | UNRESOLVED-CONFLICT (N >= 6) | >= 0.80 | Exact (>= 80%) |
| G9 | Safe Abstention Rate | count(abstained = true) | SAFE-ABSTENTION (N >= 6) | >= 0.80 | Exact (>= 80%) |
| G10 | Pool Contamination | Count where D- absent from RAW_NN | All non-superseded scenarios | = 0 | Exact (0 errors) |

---

## 7. Predeclared 5-Way Outcome Categorization

*(Unchanged from v1.8)*

1. selected_d_plus: abstained = false AND winner_id = Some(pos_id)
2. selected_d_minus: abstained = false AND winner_id = Some(neg_id)
3. selected_other: abstained = false AND winner_id is neither D+ nor D-
4. correct_abstention: abstained = true AND scenario.expected_abstain = true
5. incorrect_abstention: abstained = true AND scenario.expected_abstain = false

---

## 8. Frozen Artifact Cryptographic Manifest

*(Unchanged from v1.8)*

Implementation freeze commit: bc6634047cb900f90fd9832a6cf83793e885d1dc

NOTE: retrieval.rs, engine.rs, and config.rs hashes below are the v1.8 frozen baseline. These files will differ after Section 14 remediation is implemented. New hashes will be recorded in Section 10.1 once the remediation commit is frozen.

| File | Size | SHA-256 |
|---|---|---|
| cmhm_core/src/retrieval.rs | 21,380 | 5243A41869776FE0E78AE458FA0796E01C69644BE787487E5FF3C5B07C5C0A81 |
| cmhm_core/src/engine.rs | 57,100 | F4BBF63D2A12D31DF7BE394F9EFD8B899E112B448972B64D9B5A2A3D0A89B066 |
| cmhm_core/src/config.rs | 7,215 | 63E2F0ABCDDCBCD04E2A9ABBA1CA60B6E2A49B6D3E70D476F88D3BE9F61E8CC0 |
| cmhm_mcp/src/lib.rs | 1,451 | F5E09E80B4756F309EF8FB7467058D1C09E7B1FEAD51DC29A40F6F00C65E6845 |
| cmhm_mcp/src/tools.rs | 58,624 | 806BE9F9FC611C32E65AA9AC6B4BA6DA6939B2BB94F29721A2D80DC41124706E |
| cmhm_core/tests/phase3_synthetic_eval_tests.rs | 18,811 | 5D0C483C9258BAD34DC62364A16C8833BD798AA2C025563A1069F43850416AC0 |
| scripts/audit_corpus_overlap.ps1 | 12,533 | E3C8BE2FF91E5083864769F1189070846BD5808B7204161C3956A630CB7CCB59 |
| scripts/verify_phase3_approval.ps1 | 19,385 | 519EA3AC53FE3E73768B950352AEDCF0437D3044C219E45EC45958BE2EEA94D2 |

---

## 9. Multi-Stage Lifecycle and The One-Run Rule

*(Core lifecycle unchanged from v1.8; Harness Configuration Mandate is new in v1.9)*

Stage 0 (Pre-Flight): Clean working tree + synthetic self-tests. Failures do not consume an attempt.

Stage 1 (Overlap Audit): scripts/audit_corpus_overlap.ps1 against prohibited corpora. Any violation rejects corpus and terminates the attempt before Stage 2.

Stage 1.5 (Conformance Validation): phase3_conformance_check Rust test (Section 12). Validates every scenario against the Section 4.4 class-spec matrix. Any violation fails CLOSED before Stage 2. The attempt is NOT consumed by a Stage 1.5 failure.

Stage 2 (Single Benchmark Execution): scripts/verify_phase3_approval.ps1. PERMANENTLY CONSUMES the single approval attempt. No retries permitted.

Stage 3 (Gate Verification): Verifier evaluates all gates; exits 0 only on unanimous passage.

HARNESS CONFIGURATION MANDATE (v1.9): The benchmark harness must run with Config::default() — no field overrides. This ensures the harness tests the production configuration. The detection contract in Section 14 is expressed as changes to Config::default() values, not as harness overrides. The only permissible Config field overrides are data_dir and projections_dir (tempdir paths required for test isolation). Any other Config override invalidates the run. This rule applies to attempt 3 and all future attempts.

---

## 10. Scaffold Freeze Identity and Role B Ruling

### 10.1 Scaffold Freeze Identity

| Artifact | Value |
|---|---|
| v1.6 scaffold freeze commit | 984bef7525454c601f6fcda94a32359fba27577d |
| v1.6 SHA-256 | 22D9DD22D2CACF0EA444730BA196297ED2C895008973A4C895641F66097DA187 |
| v1.8 scaffold freeze commit | a80457dd99bf1ae36511270937c41b99606787e7 |
| v1.8 SHA-256 | E7267F1569A7D56E7A84A6B60FA60BE013B45D0A5CDE5DDA2057167C80B0B2A7 |
| v1.8 size | 42,783 bytes |
| v1.9 remediation commit | to be set after Section 14 engine changes |
| v1.9 scaffold freeze commit | to be set after SCENARIOS cleared for corpus v3 |

### 10.2 Role B Ruling

*(Unchanged from v1.8)*

The independent reviewer is disqualified from Role B due to source-level audit exposure to phase3_held_out_bench.rs queries. Charan (the user) is confirmed as Role B for all corpus authoring.

---

## 11. Attempt 1 Record (Permanent and Immutable)

Nothing in this section may be altered in any subsequent protocol revision.

### 11.1 Attempt Facts

| Field | Value |
|---|---|
| Attempt number | 1 |
| Corpus commit | e989a913f56f483a63486e7d795318ace8b77e3a |
| Corpus size | 40 scenarios (orig IDs 1-37, 57, 62, 67) |
| Overlap audit | PASS — Max Jaccard 0.0556 < 0.30; zero verbatim violations |
| Gate results | G1 PASS G2 PASS G3 PASS G4 PASS G5 PASS G6 PASS G7 PASS S1 FAIL G8a PASS G8b FAIL G8c PASS G9 FAIL G10 PASS |
| Outcome decomposition | selected_d_plus=35, selected_d_minus=5, correct_abstention=0, incorrect_abstention=0 |

### 11.2 Ruling: INVALID ON ITS FACE

Not rejected on the merits. The corpus violated Section 4.4 before Stage 2 executed.

| Class | Field | Required | Actual |
|---|---|---|---|
| Contradiction | D- authority | 0.70-0.85 | 0.38-0.55 |
| AuthorityPolarity | D- provenance | contradicted | user_explicit |
| AuthorityPolarity | D- key rule | different or absent from D+ | same key as D+ |
| SafeAbstention | D- provenance | contradicted | user_explicit |

Engine verdict: neither approved nor disapproved.

### 11.3 Root Cause

Corpus deviations originated in Role B plaintext authoring without a validator. Closed by Section 12.

### 11.4 Role A Post-Run Breach

Role A read corpus Rust file post-run and cited authority value ranges in diagnostic output, then issued a proposed corpus fix. Permanently recorded. Tuning guidance retracted unconditionally.

### 11.5 Section 4.4 Change Log: v1.6 to v1.7/v1.8

*(Full change log unchanged — see v1.8 Section 11.5 for complete text.)*

Seven fixed values widened to ranges (Category A). One substantive provenance correction: AdversarialAuthority D- prov contradicted -> user_explicit (Category B). Full rationale in v1.8.

---

## 12. Pre-Run Conformance Validator Specification

*(Unchanged from v1.8)*

### 12.1 Purpose

Validates every scenario against Section 4.4 before Stage 2 executes.

### 12.2 Implementation

  cargo test --package cmhm_mcp --test phase3_approval_bench phase3_conformance_check -- --nocapture

### 12.3 Per-Scenario Validation Rules

*(Full rules unchanged from v1.8 — see v1.8 Section 12.3 for complete text.)*

### 12.4 Global Corpus Validation Rules

1. SCENARIOS.len() >= 40
2. All id values unique
3. Per-class minimums: Contradiction >= 10; all others >= 6
4. No domain string appears > 6 times
5. All query strings unique (case-insensitive)
6. Validator self-integrity: total checked == SCENARIOS.len()

### 12.5 Validator Output Format

On success:  CONFORMANCE_CHECK|PASS|N=40|classes=Contradiction:N,...|domains_ok=true
On failure:  CONFORMANCE_CHECK|FAIL|id=N|class=X|field=Y|value=V|expected=E

---

## 13. Corpus v2 Requirements (Historical)

Corpus v2 requirements were specified in v1.8 Section 13 and satisfied by corpus v2 (commit 11c0a07). Corpus v2 is consumed. Corpus v3 requirements are in Section 16. This section is retained for historical reference only.

---

## 14. Conflict-Detection and Abstention Contract (v1.9 -- NEW)

This section specifies the production contract for conflict detection and abstention, replacing the score-gap proximity mechanism in frozen retrieval.rs. Grounded in LFA Section 12 and the persisted-metadata learn path.

### 14.1 Why Score-Gap Proximity Fails for hash-128d

The frozen retrieval.rs conflict gate (line 291) fires only when the top-2 composite scores are within conflict_detection_threshold of each other. This assumes conflicting memories produce near-indistinguishable embeddings -- that contradiction is a proximity phenomenon in embedding space.

FNV-1a hash embeddings (hash-128d) have no semantic geometry. The embedding of a text is a deterministic function of its byte sequence, not its meaning. Semantically opposite text produces embeddings as far apart as unrelated text. Conflict is therefore not detectable from score proximity under hash-128d; a threshold that catches contradictory pairs would need to exceed the typical benign top-2 gap (~0.40), which is equivalent to firing on all pairs indiscriminately.

This is an embedding-theoretic argument. Attempt-2 gap distributions corroborate it (Contradiction gaps 0.495-0.756, threshold 0.02) but do not constitute the justification. The argument stands independently of any corpus.

### 14.2 Per-Signal Detection Contract

All detection inputs are persisted node metadata written by the production learn path via authenticate_and_clamp and birth_node. They are not labels supplied by the harness at query time. This was independently verified by provenance trace (see Section 15.5).

| Signal | Runtime inputs (persisted metadata only) | Outcome |
|---|---|---|
| LogicalContradiction | Top-2 share identical non-empty assertion_key AND pa * pb < 0 (opposing polarity) | conflict_detected = true, conflict_type = LogicalContradiction |
| ProvenancePair | One of top-2 carries prov in HIGH_TRUST {"corrected","user_explicit"} AND the other carries prov in LOW_TRUST {"contradicted"} | conflict_detected = true, conflict_type = AuthorityPolarity |
| AdversarialPair (pending Section 14.4 ruling) | Both top-2 carry HIGH_TRUST prov AND neg_auth > pos_auth (adversarial authority inversion) | conflict_detected = true, conflict_type = AuthorityPolarity |
| TrustCompromised (abstention sub-condition) | ProvenancePair fires AND |auth_rank1 - auth_rank2| < delta_authority (0.15) | abstained = true (when abstain_on_conflict = true) |
| score_gap_proximity | -- | REMOVED from conflict path; see Section 14.3 |

Signal interaction notes:

- TrustCompromised is a sub-condition of ProvenancePair, not independent. ProvenancePair fires for both AuthorityPolarity and SafeAbstention class scenarios (both have contradicted D- vs. user_explicit/corrected D+). The authority-margin check determines whether to additionally abstain.
- AuthorityPolarity scenarios: auth gap ~0.39-0.47 > delta_authority (0.15). ProvenancePair fires -> conflict_type = AuthorityPolarity. TrustCompromised does NOT fire -> no abstention. G8b: satisfied.
- SafeAbstention scenarios: auth gap <= 0.10 (S4.4 constraint) < delta_authority (0.15). ProvenancePair fires -> conflict_type = AuthorityPolarity. TrustCompromised fires -> abstained = true. G9: satisfied.
- UnresolvedConflict: both user_explicit; no HIGH/LOW pairing. ProvenancePair does not fire. conflict_type = None. G8c: satisfied (already passing in attempt 2, unchanged).
- AdversarialAuthority: see Section 14.4.

### 14.3 Disposition of conflict_detection_threshold

Removed from conflict classification logic. Config::default() retains the field for API compatibility at value 0.0 (no-op sentinel; gap >= 0.0 always). Field documented as deprecated-in-place. The outer `if gap < conflict_detection_threshold` gate in retrieval.rs Step 2d is eliminated entirely.

### 14.4 G8b and AdversarialAuthority -- Open Question

G8b numerator = count(conflict_type = AuthorityPolarity) over AUTH_POLARITY + ADVERSARIAL (N >= 12).

AdversarialAuthority scenarios: neg_prov = user_explicit (not in LOW_TRUST). ProvenancePair does not fire. Under the base Section 14.2 contract, conflict_type = None for AA scenarios. G8b = 6/12 = 0.50, below >= 0.80 threshold. G8b fails.

REVIEWER RULING REQUIRED. Two options:

Option A (Role A recommendation): Add AdversarialPair signal. Detect: both top-2 carry HIGH_TRUST prov AND neg_auth > pos_auth (the adversarial condition already expressed in persisted metadata). Set conflict_type = AuthorityPolarity. G8b: 12/12 AP+AA. This keeps detection in metadata (provenance + authority ordering), consistent with LFA Section 12 trust-signal framework. No score-proximity component.

Option B: Separate G8b population, add new gate G8d for AA. Material change to Section 6 gate table -- requires full rationale. Not recommended: adds complexity and a new gate rather than capturing the existing adversarial concept cleanly.

Pending reviewer ruling; implementation of Section 14.7 waits on this decision.

### 14.5 Pre-Declared Authority-Margin Threshold

delta_authority = 0.15 (pre-declared; not adjustable after attempt 3 is observed)

Rationale (independent of corpus values):

Authority is a calibrated trust signal in [0, 1]. A node with contradicted provenance is explicitly tagged as having been challenged. If its authority is within delta_authority of the top-ranked node, the system cannot assert that the winner's trust meaningfully outweighs the challenger's.

The value 0.15 corresponds to one trust band in a practical 5-band model (bands at 0.0-0.2, 0.2-0.4, 0.4-0.6, 0.6-0.8, 0.8-1.0). At gap < 0.15, winner and challenger straddle at most one band boundary; at gap >= 0.15, the winner has crossed into a meaningfully higher trust band.

Relationship to S4.4: SafeAbstention requires |pos_auth - neg_auth| <= 0.10. delta_authority = 0.15 is strictly larger, so every conformant SafeAbstention scenario (gap <= 0.10) is detected. The corpus constraint and detector threshold are independently derived. Their relationship (corpus subset of detector range) is the correct direction: the corpus is designed to be detected, not the detector designed to match the corpus.

### 14.6 Engine Default Changes (Production)

Required changes to Config::default() in cmhm_core/src/config.rs:

1. abstain_on_conflict: false -> true

   Rationale: LFA S12 -- "abstain if confidence is below calibrated threshold or trusted-cell disagreement exceeds D_max." An engine that detects conflict but does not abstain presents a contested answer as settled, violating the LFA abstention contract. This must be the production default. The benchmark harness runs Config::default() unchanged (Section 9 mandate); this change affects production and benchmark simultaneously. No harness override.

2. conflict_detection_threshold: 0.02 -> 0.0

   Documents that the score-gap gate is intentionally inert. Value 0.0 makes explicit that the gap check never fires (gap >= 0.0 always). Rationale: Section 14.1.

### 14.7 Implementation Scope

Changes required in cmhm_core/src/retrieval.rs (Step 2d):

1. Remove the outer `if gap < conflict_detection_threshold { ... }` gate entirely.
2. Retain the LogicalContradiction check (assertion_key match + polarity opposition) as a standalone signal -- remove only its gap-gate enclosure.
3. Retain the SupersessionConflict check -- remove only its gap-gate enclosure.
4. Retain and refactor the provenance-pair check (HIGH_TRUST / LOW_TRUST) as the ProvenancePair signal, firing regardless of gap.
5. Add TrustCompromised sub-condition: within ProvenancePair, if |auth_a - auth_b| < 0.15, set should_abstain = true. conflict_type remains AuthorityPolarity in all ProvenancePair cases.
6. Pending Section 14.4 ruling: if Option A approved, add AdversarialPair variant (HIGH_TRUST vs. HIGH_TRUST with neg_auth > pos_auth -> conflict_type = AuthorityPolarity, no abstention trigger).

Changes required in cmhm_core/src/config.rs:

1. default_abstain_on_conflict() -> true
2. default_conflict_detection_threshold() -> 0.0 with deprecation comment

---

## 15. Attempt 2 Record (Permanent and Immutable)

Nothing in this section may be altered in any subsequent protocol revision.

### 15.1 Attempt Facts

| Field | Value |
|---|---|
| Attempt number | 2 |
| Corpus commit | 11c0a07f0ebaabe46cd065671c637fde8cd78dc2 |
| Corpus SHA-256 | 20B3EFD1034783E37F4AEB60B0E18E5F92039D6D8B5731A8ED9CF0C29DD3FE58 |
| Corpus size | 43 scenarios (IDs 201-243), 73,402 bytes |
| Stage 0 audit | PASS -- Max Jaccard 0.2727 < 0.30; attempt-1 exact matches 0; zero violations |
| Audit log commit | 551121ee58382f927c9e713f5372d653f02257a4 (phase3_overlap_audit_v2b.log) |
| Stage 1.5 conformance | PASS -- CONFORMANCE_COUNTS|N=43|Contradiction:13,Supersession:6,AuthorityPolarity:6,AdversarialAuthority:6,SafeAbstention:6,UnresolvedConflict:6 |
| Stage 2 verdict | REJECTED |
| Benchmark log commit | 551121ee58382f927c9e713f5372d653f02257a4 (phase3_attempt2_benchmark.log) |

### 15.2 Gate Results

| Gate | Result | Value | CI Lo | CI Hi | Note |
|---|---|---|---|---|---|
| G1 | PASS | 0.8372 | 0.7209 | 0.9302 | 36/43 |
| G2 | PASS | 1.0000 | 1.0000 | 1.0000 | 43/43 |
| G3 | PASS | 0.9186 | 0.8605 | 0.9651 | mrr=0.9186 |
| G4 | PASS | 0.1628 | 0.0698 | 0.2791 | 7/43 |
| G5 | PASS | 0.0558 | 0.0279 | 0.0884 | 12/215 |
| G6 | PASS | 0.4387 | 0.3226 | 0.5590 | mean_margin=0.4387 |
| G7 | PASS | 0.0000 | 0.0000 | 0.0000 | 0 violations / 6 supersession |
| S1 | FAIL | 0.0000 | 0.0000 | 0.0000 | 0/13 |
| G8a | PASS | 0.0000 | 0.0000 | 0.0000 | 0/0 eligible |
| G8b | FAIL | 0.0000 | 0.0000 | 0.0000 | 0/12 |
| G8c | PASS | 1.0000 | 1.0000 | 1.0000 | 6/6 |
| G9 | FAIL | 0.0000 | 0.0000 | 0.0000 | 0/6 |
| G10 | PASS | 0.0000 | 0.0000 | 0.0000 | 0 violations / 37 non-supersession |

Gates passed: 10/13. Gates failed: S1, G8b, G9.

### 15.3 Ruling: REJECTED ON THE MERITS

The corpus was conformant (Stage 1.5 PASS) and fresh (Stage 0 PASS). The benchmark ran cleanly (G2 = 43/43). Rejection is a genuine engine capability finding, not a corpus defect. Gates failed for a single root cause: conflict-detection and abstention subsystems never fired. abstained = false and conflict = None on all 43 scenarios. No gate is relaxed or reinterpreted. The attempt is permanently consumed.

### 15.4 Root Cause

Primary: conflict_detection_threshold = 0.02 never fires under hash-128d. Attempt-2 Contradiction gaps: 0.495-0.756. No scenario approached threshold.

Secondary: abstain_on_conflict = false would have suppressed abstention even if conflict had fired.

The conflict-detection code (retrieval.rs:286-367) is implemented and wired. This is a calibration and architectural mismatch, not a missing implementation.

Evidence discipline: gap-distribution values are diagnostic corroboration from a consumed corpus. They confirm the embedding-theoretic argument; they do not constitute its basis.

### 15.5 Provenance Trace Verification

The four metadata fields used by the conflict classifier (assertion_key, polarity, provenance, superseded_by) are runtime-derived from the persisted node store -- written by the production learn path (authenticate_and_clamp -> birth_node -> node_store) at learn time and read back at query time. ScenarioClass appears only in post-retrieval gate partitioning and never crosses the retrieval boundary. Verified by Role A provenance trace and confirmed by Reviewer independent inspection of the frozen scaffold. Benchmark contract: SOUND.

### 15.6 Corpus v2 Diagnostic Status

Corpus v2 (commit 11c0a07) is permanently consumed. It may be used as diagnostic evidence only. It may never serve as an approval corpus, be partially reused in corpus v3, or be used to tune any gate threshold or detection parameter.

---

## 16. Attempt 3 Authorization (v1.9 -- NEW)

### 16.1 Prerequisites (all must be complete before attempt 3 begins)

1. Reviewer approval of v1.9 -- including ruling on Section 14.4 (G8b / AdversarialAuthority).
2. Engine remediation committed -- changes per Section 14.7 (retrieval.rs + config.rs). New SHA-256 hashes of modified files recorded in Section 10.1.
3. Synthetic pre-flight re-run -- phase3_synthetic_eval_tests.rs must pass 10/10 after Section 14 changes. Any regression blocks attempt 3.
4. Scaffold cleared for v3 -- SCENARIOS slice cleared to placeholder; bench file frozen at new commit; identity (commit hash + SHA-256 + byte size) recorded in Section 10.1.
5. Overlap auditor extended (Source 4) -- audit_corpus_overlap.ps1 extended with 43 hardcoded attempt-2 query strings (from commit 11c0a07), exact mechanism as Source 3. Self-test extended with at least one Source-4 live-fire positive. Commit identity recorded here after extension.
6. Corpus v3 authored by Role B -- per Section 4.4, Section 16.2, and Section 13.2 cheat-sheet. Zero reuse from corpora v1 or v2.
7. Role B explicit authorization -- sign-off that corpus v3 is ready for transcription.

### 16.2 Corpus v3 Requirements

- Satisfies Sections 4.1, 4.2, 4.3, and 4.4
- Zero query-text reuse from attempt-1 corpus (enforced by auditor Source 3)
- Zero query-text reuse from attempt-2 corpus (enforced by auditor Source 4)
- Authored by Role B with no reference to attempt-1 or attempt-2 gate diagnostics, score values, gap distributions, or authority-margin values from either consumed corpus
- Authored using only Section 4.4 matrix and Section 13.2 cheat-sheet as spec references
- Fresh means fresh: not minor paraphrases of v1 or v2 queries. Skeleton self-scan conducted by Role A at transcription stage per established procedure.

### 16.3 Attempt 3 Sequence

1. Prerequisites 1-5 complete
2. Role B authors corpus v3 -> submits plaintext
3. Skeleton self-scan (Role A) -> flagged queries returned to Role B for replacement
4. Role A transcribes -> corpus commit
5. Stage 0 (overlap audit, all four sources) -> PASS required
6. Stage 1 (synthetic pre-flight) -> PASS required
7. Stage 1.5 (conformance check) -> PASS required
8. Stage 2 (benchmark, single run) -> executed by Role B on Role A go-ahead; PERMANENTLY CONSUMES attempt
9. Stage 3 (gate verification) -> reviewer evaluates all gates

---

## 15.7 Change Log: v1.8 to v1.9

### What changed

| Item | Change | Section |
|---|---|---|
| Resolution Table | Entries 16-18 added | S3 |
| Prohibited Corpora | Items 3-4 added (consumed corpora v1 and v2) | S4.3 |
| Harness Configuration Mandate | Config::default() override prohibition stated | S9 |
| Scaffold freeze table | v1.8 identity recorded; v1.9 placeholders added | S10.1 |
| Manifest NOTE | Pre-discloses hashes will change post-remediation | S8 |
| Conflict-Detection Contract | Per-signal metadata contract; score-gap removed; delta_authority = 0.15 pre-declared | S14 (NEW) |
| Config defaults | abstain_on_conflict -> true; conflict_detection_threshold -> 0.0 | S14.6 |
| Attempt 2 Record | Permanent: facts, gate results, REJECTED ruling, root cause, provenance trace | S15 (NEW) |
| Attempt 3 Authorization | Prerequisites, corpus v3 requirements, sequence | S16 (NEW) |
| Corpus v2 section | Marked historical; live requirements moved to S16 | S13 |

### What did NOT change

- Section 6: all 13 gates unchanged in threshold, numerator, denominator, and CI condition
- Section 4.4 matrix: unchanged
- Section 12 validator: unchanged
- Section 11 (Attempt 1 record): unchanged and immutable
- Section 2 (Role assignments): unchanged
- Section 9 one-run rule: unchanged (mandate is additive)
- Section 5 empirical gate: unchanged

---

*End of Phase 3 Approval Protocol v1.9 (DRAFT)*
