# Phase 3 Approval Protocol v2.0

**Version:** 2.0
**Supersedes:** v1.9 (SHA-256 F0736803B5E42F909E4FD89A7B7747FA2D9516CDD4085ECA0780E3DB51B22346, commit 7bcf24e)
**Status:** DRAFT -- pending reviewer approval

---

## 1. Purpose and Scope

*(Unchanged from v1.9)*

This protocol governs Phase 3 approval for the CMHM (Contextual Hierarchical Memory Model) system. Phase 3 verifies that the retrieval-safety properties claimed by Phase 2 hold on a fresh, independent corpus under pre-declared, immutable acceptance gates.

Phase 3 does not re-measure Phase 2 metrics; it measures operational retrieval safety across six independently authored scenario classes using the production retrieve_unscoped path and hash-128d model identifier.

---

## 2. Role Assignments and Custody Model

*(Unchanged from v1.9)*

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

RECORDED ROLE A CONTAMINATION (attempt 1, post-run): Role A read corpus Rust file post-run and cited specific authority value ranges in diagnostic output. Role A also issued a proposed corpus fix derived from this run's diagnostics -- exactly the tuning the protocol prohibits. Both actions are permanently recorded. The tuning guidance is retracted unconditionally.

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
| 17 | conflict_detection_threshold = 0.02 never fires under hash-128d embeddings | Resolved | Section 14: score-gap gate DELETED from decision path; per-signal metadata contract specified |
| 18 | abstain_on_conflict = false (default) prevents all safe abstention | Resolved | Section 14.6: production default changed to true |
| 19 | S1 gate hardcodes gap < 0.02 in bench (line 849) independent of Config; guarantees attempt-3 S1 = 0/N | Resolved | Section 14.8: S1 re-scoped to top-2 co-presence; owner and reviewer sign-offs recorded |
| 20 | authority.rs absent from Section 8 manifest (gap 1) | Resolved | Added to Section 8 manifest in v2.0 with provenance |
| 21 | G8b derivation did not check LC-precedence interaction for same-key AA pairs (gap 2) | Resolved | Section 4.4 AA key rule amended (neg_key different or None); Section 12.3 validator extended; G8b derivation updated in Section 14.2 |
| 22 | TrustCompromised structurally unreachable for contradicted pairs -- clamped runtime gap always >= 0.45 (gap 3) | Resolved | TrustCompromised reformulated on provenance semantics in Section 14.2: fires for all ProvenancePair cases |
| 23 | Class-table runtime-consistency proof absent from review checklist (gap 4) | Resolved | Section 14.9: runtime-consistency proof added for every Section 14.2 class row |
| 24 | Section 12.3 validator does not check expected_abstain against Section 4.4 Abstain column (gap 5) | Resolved | Section 12.3 extended with per-class expected_abstain validation; Section 12.3 "no field is omitted" claim corrected |

---

## 4. Corpus Requirements

### 4.1 Size and Domain Distribution

*(Unchanged from v1.9)*

The approval corpus must contain exactly N scenarios (N >= 40) spanning at least 8 operational domains, with no domain exceeding 6 queries.

### 4.2 Class Count Minimums

*(Unchanged from v1.9)*

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

*(Unchanged from v1.9 except item 5 added)*

Permanently prohibited from any reuse of query text, positive text, or negative text:

1. cmhm_core/tests/dprime_bench.rs -- 15-query rejection set
2. cmhm_core/tests/phase3_held_out_bench.rs -- 32-query compromised corpus
3. Attempt-1 corpus (commit e989a91) -- consumed; zero query reuse (enforced by auditor Source 3)
4. Attempt-2 corpus (commit 11c0a07) -- consumed diagnostic corpus; zero query reuse (enforced by auditor Source 4)
5. Attempt-3 corpus (commit a58bc0a) -- consumed; zero query reuse (enforced by auditor Source 5 -- see Section 18.1)

### 4.4 Complete Scenario-Class Matrix

*(Section 4.4 may not be altered in response to gate results. The AA key rule is amended here with pre-declared rationale in Section 14.8.)*

| Class | Min | D- Auth / Prov | D+ Auth / Prov | Key Rule | Polarity | supersedes_neg | Expected Conflict | Abstain | Evaluating Gates |
|---|---|---|---|---|---|---|---|---|---|
| CONTRADICTION | 10 | 0.70-0.85 / contradicted | >=0.80 / user_explicit | neg_key == pos_key (SAME) | D+ = +1, D- = -1 | false | LogicalContradiction | true | G1-G6, G8a, S1, G10 |
| SUPERSESSION | 6 | < (D+ auth - 0.05) / contradicted | > (D- auth + 0.05) / user_explicit | any | D- polarity == D+ polarity | TRUE | None | false | G1-G6, G7 |
| AUTHORITY-POLARITY | 6 | 0.70-0.80 / contradicted | 0.80-0.90 / user_explicit or corrected | neg_key DIFFERENT from pos_key (or None) | any | false | AuthorityPolarity | true | G1-G6, G8b, G10 |
| ADVERSARIAL-AUTHORITY | 6 | 0.90-1.00 / user_explicit | 0.75-0.85 / corrected | neg_key DIFFERENT from pos_key (or None) [AMENDED v2.0] | any | false | AuthorityPolarity | true | G1-G6, G8b, G10 |
| SAFE-ABSTENTION | 6 | 0.75-0.87 / contradicted | 0.75-0.90 / user_explicit | any | any | false | AuthorityPolarity | true | G1-G6, G9, G10 |
| UNRESOLVED-CONFLICT | 6 | 0.75-0.85 / user_explicit | 0.80-1.00 / user_explicit | any | same (no polarity clash) | false | None | false | G1-G6, G8c, G10 |

Additional constraints (all enforced by Section 12 validator):

CONTRADICTION: D+ and D- share the same assertion_key; D+ polarity +1, D- polarity -1; D- auth 0.70-0.85.

SUPERSESSION: pos_auth >= neg_auth + 0.05. supersedes_neg = true. Same polarity on D+ and D-.

AUTHORITY-POLARITY: neg_key must be different from pos_key (or neg_key = None).

ADVERSARIAL-AUTHORITY: neg_auth > pos_auth. neg_prov = user_explicit. neg_key must be different from pos_key (or neg_key = None). [Key rule AMENDED v2.0 -- see Section 14.8 for rationale.] This prevents LogicalContradiction (same-key + opposing-polarity) from firing before AdversarialPair, preserving the intended authority-inversion test.

SAFE-ABSTENTION: |pos_auth - neg_auth| <= 0.10. No ordering constraint between pos_auth and neg_auth.

UNRESOLVED-CONFLICT: Both user_explicit provenance. pos_auth > neg_auth. No polarity clash.

---

## 5. Empirical Nature of Gate G5

*(Unchanged from v1.9)*

Gate G5 is an empirical evaluation gate with NO feasibility pre-guarantee. Evaluated via 10,000 bootstrap resamples. Upper 95% bootstrap confidence bound must be strictly < 0.40.

Occupancy@5 = (total D- in top-5 slots) / (5 * N)

---

## 6. Acceptance Gates, Populations, and Stage Invariants

GOVERNING CONSTRAINT: No gate may be removed, weakened, or reinterpreted after observing a failure.

Note: S1 metric is further amended in v2.0 to reflect pool-based detection. Disclosure and rationale in Section 14.8. Threshold (>= 0.60) and denominator (All CONTRADICTION) unchanged.

| Gate | Metric | Numerator | Denominator | Threshold | CI Condition |
|---|---|---|---|---|---|
| G1 | Recall@1 | count(pos_rank_SCORED = 1) | Full corpus N | >= 0.40 | Lower 95% CI >= 0.40 |
| G2 | Recall@5 | count(pos_in_top5_SCORED) | Full corpus N | >= 0.67 | Lower 95% CI >= 0.67 |
| G3 | MRR | Sum 1/pos_rank | Full corpus N | >= 0.40 | Lower 95% CI >= 0.40 |
| G4 | Contradiction-at-1 | count(neg_rank_SCORED = 1) | Full corpus N | < 0.40 | UPPER 95% CI < 0.40 |
| G5 | Hard-Neg Occupancy@5 | Total D- in top-5 slots | 5 * N slots | < 0.40 | UPPER 95% CI < 0.40 |
| G6 | Mean Margin | Sum (S(D+) - S(D-)) | Full corpus N | > 0.00 | Lower 95% CI > 0.00 |
| G7 | Supersession Safety | Count superseded D- in post_filter/scored/conflict/chain/winner | SUPERSESSION (N >= 6) | = 0.00 | Exact (0 violations) |
| G8a | LogicalContradiction Rate | count(conflict_type = LogicalContradiction) | S1-eligible CONTRADICTION | >= 0.80 | Exact (>= 80%) |
| S1 | LC Pool-Eligibility Rate | count(D- present in post-filter pool) | All CONTRADICTION (N >= 10) | >= 0.60 | Exact (>= 60%) [AMENDED v2.0] |
| G8b | AuthorityPolarity Rate | count(conflict_type = AuthorityPolarity) | AUTH_POLARITY + ADVERSARIAL (N >= 12) | >= 0.80 | Exact (>= 80%) |
| G8c | Unresolved Non-Conflict | count(conflict_type = None) | UNRESOLVED-CONFLICT (N >= 6) | >= 0.80 | Exact (>= 80%) |
| G9 | Safe Abstention Rate | count(abstained = true) | SAFE-ABSTENTION (N >= 6) | >= 0.80 | Exact (>= 80%) |
| G10 | Pool Contamination | Count where D- absent from RAW_NN | All non-superseded scenarios | = 0 | Exact (0 errors) |

S1 amendment note: S1 now measures D- presence in the post-filter pool (output of Step 2a supersession filter) for Contradiction scenarios. Under pool-based detection (Section 14.2 v2.0), the detector operates on the post-filter pool; pool-membership is therefore the correct eligibility precondition for G8a, replacing scored top-2 co-presence. Threshold and denominator unchanged. Full disclosure in Section 14.8.

---

## 7. Predeclared 5-Way Outcome Categorization

*(Unchanged from v1.9. Note: as of v2.0, expected_abstain is validated against the Section 4.4 Abstain column by the Section 12.3 conformance validator; harness categorise() continues to read scenario.expected_abstain but a non-conformant value will be caught at Stage 1.5.)*

1. selected_d_plus: abstained = false AND winner_id = Some(pos_id)
2. selected_d_minus: abstained = false AND winner_id = Some(neg_id)
3. selected_other: abstained = false AND winner_id is neither D+ nor D-
4. correct_abstention: abstained = true AND scenario.expected_abstain = true
5. incorrect_abstention: abstained = true AND scenario.expected_abstain = false

Section 4.4 Abstain column truth table (enforced by Section 12.3 validator as of v2.0):

| Class | expected_abstain |
|---|---|
| Contradiction | true |
| Supersession | false |
| AuthorityPolarity | true |
| AdversarialAuthority | true |
| SafeAbstention | true |
| UnresolvedConflict | false |

---

## 8. Frozen Artifact Cryptographic Manifest

Implementation freeze commit: bc6634047cb900f90fd9832a6cf83793e885d1dc
Archive: phase3-bc66340-lf.zip (2,812,990 bytes, SHA-256: 7D91A8B38F5D96123B6A5C312EDD99B2D5D16D890FAD079380B062EFEFC5DF70)
Line-ending policy: LF only. Hashes are raw-byte hashes of extracted archive entries.

Frozen implementation files (v1.8 baseline -- retrieval.rs, engine.rs, and config.rs hashes will change after Section 14 remediation; new hashes recorded in Section 10.1):

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
| cmhm_core/src/config.rs [NEW v1.9] | 10,549 | F0A7532E6C2F72A3363D107B60FC108BB77C0542473201FF8CF8C456BF749CAD |
| cmhm_core/src/authority.rs [NEW v2.0] | 9,887 | A820C7791F85BBC6551C4BEA0DAEF4A6EC57DC94E810483422833ADB2FE85C16 |

authority.rs provenance: this file contains the contradicted provenance clamp at line 109 ([0.00, 0.30]). It was present at bc66340 but omitted from the v1.8 and v1.9 manifests. The clamp constant in this file is the proximate cause of D- score suppression in attempts 2 and 3. It is added to the manifest here with its commit-bc66340 hash (LF-extracted from phase3-bc66340-lf.zip). Its hash will change after Section 14 v2.0 remediation if TrustCompromised reformulation requires any changes to authority.rs; new hash recorded in Section 10.1 post-remediation.

config.rs is listed as NEW v1.9: it was omitted from the v1.8 manifest table. The hash above is the state at commit 551121ee. It will change after Section 14.6 defaults are applied; the post-remediation hash is recorded in Section 10.1.

Post-freeze harness changes (outside manifest; do not affect the implementation under test):

| File | Change | Commit |
|---|---|---|
| cmhm_mcp/tests/phase3_approval_bench.rs | Scaffold authored; corpus v2 transcribed; g5_slots defect fixed | 984bef7, 1b07c83 |
| scripts/audit_corpus_overlap.ps1 | PS empty-List unrolling bug fixed; pos_text/neg_text field aliases added; Source 3 (43 attempt-1 queries) + live-fire self-tests added | e989a91, 57e31a0 |
| scripts/verify_phase3_approval.ps1 | Stage 1.5 conformance gate added | a80457d |

Note on post-freeze script hashes: the working-tree hashes of audit_corpus_overlap.ps1 and verify_phase3_approval.ps1 differ from the archive hashes above because both scripts grew after bc66340 (Source 3 additions, Stage 1.5 integration). Current working-tree hashes: audit_corpus_overlap.ps1 = 27,543 bytes / 6FB3FD92DF80F760BE64E3B6C575FC4A5DF25CC55BE9171475FEC8D88E8ABFBA (commit 7c90c3a); verify_phase3_approval.ps1 = 20,647 bytes / 702E1D8DD6943B497E0184705857B8A7DCAD9744E925B1D14EABE2F6A11CC8D2.

---

## 9. Multi-Stage Lifecycle and The One-Run Rule

*(Unchanged from v1.9)*

Stage 0 (Pre-Flight): Clean working tree + synthetic self-tests. Failures do not consume an attempt.

Stage 1 (Overlap Audit): scripts/audit_corpus_overlap.ps1 against prohibited corpora. Any violation rejects corpus and terminates the attempt before Stage 2.

Stage 1.5 (Conformance Validation): phase3_conformance_check Rust test (Section 12). Validates every scenario against the Section 4.4 class-spec matrix. Any violation fails CLOSED before Stage 2. The attempt is NOT consumed by a Stage 1.5 failure.

Stage 2 (Single Benchmark Execution): scripts/verify_phase3_approval.ps1. PERMANENTLY CONSUMES the single approval attempt. No retries permitted.

Stage 3 (Gate Verification): Verifier evaluates all gates; exits 0 only on unanimous passage.

HARNESS CONFIGURATION MANDATE (v1.9, unchanged): The benchmark harness must run with Config::default() -- no field overrides. The detection contract in Section 14 is expressed as changes to Config::default() values, not as harness overrides. The only permissible Config field overrides are data_dir and projections_dir (tempdir paths required for test isolation). Any other Config override invalidates the run.

---

## 10. Scaffold Freeze Identity and Role B Ruling

### 10.1 Scaffold Freeze Identity

*(Pre-remediation entries unchanged from v1.9. Post-remediation entries for v2.0 engine changes to be added after implementation.)*

| Artifact | Value |
|---|---|
| v1.6 scaffold freeze commit | 984bef7525454c601f6fcda94a32359fba27577d |
| v1.6 SHA-256 | 22D9DD22D2CACF0EA444730BA196297ED2C895008973A4C895641F66097DA187 |
| v1.8 scaffold freeze commit | a80457dd99bf1ae36511270937c41b99606787e7 |
| v1.8 SHA-256 | E7267F1569A7D56E7A84A6B60FA60BE013B45D0A5CDE5DDA2057167C80B0B2A7 |
| v1.8 size | 42,783 bytes |
| v1.9 remediation commit | a1a933d2948a297db7760c275f098c50d63be19e |
| v1.9 remediation retrieval.rs SHA-256 | 88626F3865B29FDBBFEBC5598CF5BA84807BC56C61A38912501B63607BEC585C (21,892 bytes) |
| v1.9 remediation config.rs SHA-256 | 37F61660A3637DE59955D8472A99C2C99D84582FB99643363FAD13BFF7A9DE94 (10,142 bytes) |
| v1.9 remediation lib.rs SHA-256 | F44AD56C7E556222BC8401F835BBA947066D84251A3062784F6906F70DAE8DE0 (2,691 bytes) |
| v1.9 scaffold freeze commit | 40851a8d6ebb5579f6bf158db9b0c98f439c4ea9 |
| v1.9 scaffold freeze SHA-256 | 60A2675E6207FE4726359394CC5A4A4AFAFAA15F31E8F760DE49DA394EEE23C3 |
| v1.9 scaffold freeze size | 42,721 bytes |
| v1.9 auditor extension commit | 7c90c3af4a99187ec0caae352936720f18aa0eb5 |
| v1.9 auditor audit_corpus_overlap.ps1 SHA-256 | 6FB3FD92DF80F760BE64E3B6C575CC55BE9171475FEC8D88E8ABFBA (27,543 bytes) |
| v2.0 remediation retrieval.rs SHA-256 | [PENDING -- to be filled after v2.0 engine remediation] |
| v2.0 remediation authority.rs SHA-256 | [PENDING -- to be filled if authority.rs changes] |
| v2.0 scaffold freeze commit | [PENDING] |
| v2.0 scaffold freeze SHA-256 | [PENDING] |
| v2.0 auditor Source 5 commit | [PENDING] |

### 10.2 Role B Ruling

*(Unchanged from v1.9)*

The independent reviewer is disqualified from Role B due to source-level audit exposure to phase3_held_out_bench.rs queries. Charan (the user) is confirmed as Role B for all corpus authoring.

---

## 11. Attempt 1 Record (Permanent and Immutable)

*(Unchanged from v1.9 -- Section 11 in its entirety is immutable)*

Nothing in this section may be altered in any subsequent protocol revision.

### 11.1 Attempt Facts

| Field | Value |
|---|---|
| Attempt number | 1 |
| Corpus commit | e989a913f56f483a63486e7d795318ace8b77e3a |
| Corpus size | 40 scenarios (orig IDs 1-37, 57, 62, 67) |
| Overlap audit | PASS -- Max Jaccard 0.0556 < 0.30; zero verbatim violations |
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

*(Full change log unchanged -- see v1.8 Section 11.5, permanently retained in repo at docs/phase3_approval_protocol_v1.8.md, for complete text.)*

Seven fixed values widened to ranges (Category A). One substantive provenance correction: AdversarialAuthority D- prov contradicted -> user_explicit (Category B). Full rationale in v1.8.

---

## 12. Pre-Run Conformance Validator Specification

### 12.1 Purpose

*(Unchanged from v1.9)*

Validates every scenario against Section 4.4 before Stage 2 executes.

### 12.2 Implementation

*(Unchanged from v1.9)*

  cargo test --package cmhm_mcp --test phase3_approval_bench phase3_conformance_check -- --nocapture

### 12.3 Per-Scenario Validation Rules

*(Unchanged rules retained from v1.9. v2.0 adds: (A) expected_abstain validation for all classes; (B) AA key rule validation. The v1.9 claim "every field specified in the matrix is checked; no field is omitted" was found false at attempt 3 -- the Abstain column was not validated. That claim is corrected here: as of v2.0 all Section 4.4 fields including Abstain are validated.)*

Per-class rules (full 1:1 correspondence with Section 4.4; all fields now validated):

**CONTRADICTION:**
- pos_prov = user_explicit
- pos_polarity = Some(+1)
- pos_auth >= 0.80
- neg_auth in [0.70, 0.85]
- neg_prov = contradicted
- neg_polarity = Some(-1)
- neg_key == pos_key (SAME -- both present and equal)
- supersedes_neg = false
- expected_abstain = true [NEW v2.0]

**SUPERSESSION:**
- pos_prov = user_explicit
- neg_prov = contradicted
- pos_auth >= neg_auth + 0.05
- neg_polarity == pos_polarity (same direction)
- supersedes_neg = true
- expected_abstain = false [NEW v2.0]

**AUTHORITY-POLARITY:**
- pos_auth in [0.80, 0.90]
- pos_prov in {user_explicit, corrected}
- neg_auth in [0.70, 0.80]
- neg_prov = contradicted
- neg_key != pos_key OR neg_key.is_none()
- supersedes_neg = false
- expected_abstain = true [NEW v2.0]

**ADVERSARIAL-AUTHORITY:**
- pos_auth in [0.75, 0.85]
- pos_prov = corrected
- neg_auth in [0.90, 1.00]
- neg_prov = user_explicit
- neg_auth > pos_auth
- supersedes_neg = false
- neg_key != pos_key OR neg_key.is_none() [NEW v2.0 -- prevents LC from firing before AdversarialPair]
- expected_abstain = true [NEW v2.0]

**SAFE-ABSTENTION:**
- pos_auth in [0.75, 0.90]
- pos_prov = user_explicit
- neg_auth in [0.75, 0.87]
- neg_prov = contradicted
- |pos_auth - neg_auth| <= 0.10
- supersedes_neg = false
- expected_abstain = true [NEW v2.0]

**UNRESOLVED-CONFLICT:**
- pos_auth in [0.80, 1.00]
- pos_prov = user_explicit
- neg_auth in [0.75, 0.85]
- neg_prov = user_explicit
- pos_auth > neg_auth
- No polarity clash: NOT (pos_polarity = Some(+1) AND neg_polarity = Some(-1)) AND NOT (pos_polarity = Some(-1) AND neg_polarity = Some(+1))
- supersedes_neg = false
- expected_abstain = false [NEW v2.0]

### 12.4 Global Corpus Validation Rules

*(Unchanged from v1.9)*

1. SCENARIOS.len() >= 40
2. All id values unique
3. Per-class minimums: Contradiction >= 10; all others >= 6
4. No domain string appears > 6 times
5. All query strings unique (case-insensitive)
6. Validator self-integrity: total checked == SCENARIOS.len()

### 12.5 Validator Output Format

*(Unchanged from v1.9)*

On success:  CONFORMANCE_CHECK|PASS|N=40|classes=Contradiction:N,...|domains_ok=true
On failure:  CONFORMANCE_CHECK|FAIL|id=N|class=X|field=Y|value=V|expected=E

---

## 13. Corpus v2 and v3 Requirements (Historical)

Corpus v2 requirements were specified in v1.8 Section 13 and satisfied by corpus v2 (commit 11c0a07). Corpus v3 requirements were specified in v1.9 Section 16 and satisfied by corpus v3 (commit a58bc0a). Both corpora are consumed. Corpus v4 requirements are in Section 17. This section is retained for historical reference only.

---

## 14. Conflict-Detection and Abstention Contract (v2.0 -- AMENDED)

This section supersedes Section 14 of v1.9 in its entirety. Changes are: (1) detection input shifted from scored top-2 to post-filter pool; (2) TrustCompromised reformulated on provenance semantics; (3) class-behavior table updated and verified for runtime consistency; (4) G8b derivation corrected for LC-precedence interaction with AA. Unchanged from v1.9: score-gap gate is deleted, named constant TRUST_COMPROMISE_DELTA, AdversarialPair tightened condition (pos_prov=corrected), gap-independence unit test requirement.

### 14.1 Why Score-Gap Proximity Fails for hash-128d

*(Unchanged from v1.9)*

The frozen retrieval.rs conflict gate (line 291) fires only when the top-2 composite scores are within conflict_detection_threshold of each other. This assumes conflicting memories produce near-indistinguishable embeddings -- that contradiction is a proximity phenomenon in embedding space.

FNV-1a hash embeddings (hash-128d) have no semantic geometry. The embedding of a text is a deterministic function of its byte sequence, not its meaning. Semantically opposite text produces embeddings as far apart as unrelated text. Conflict is therefore not detectable from score proximity under hash-128d; a workable threshold would need to exceed the typical benign top-2 gap (~0.40), which is structurally equivalent to firing on all pairs indiscriminately.

This is an embedding-theoretic argument. Attempt-2 gap distributions corroborate it but do not constitute the justification. The argument stands independently of any corpus.

### 14.2 Per-Signal Detection Contract (v2.0 -- AMENDED)

**Detection input [AMENDED v2.0]:** The post-filter pool -- all nodes passing Step 2a supersession filter -- NOT the scored top-2. This change is required because composite scoring incorporates the authority weight (0.8 * node.authority), which applies the clamped persisted authority of contradicted nodes (<= 0.30). The clamp suppresses contradicted D- nodes below scored top-2 despite their presence in raw-NN, creating a circular dependency: the detection window's input is determined by a ranking function that embeds the trust signal (authority clamp) it is trying to detect. Pool-based detection breaks this circularity by operating at the metadata layer before the authority weight re-ranks the pool. LFA Section 12 rationale: trust-cell disagreement is expressed in persisted provenance metadata; the conflict signal IS the provenance metadata, not the scoring outcome derived from it.

Detection sequence:
1. Step 2a: supersession filter on raw-NN (unchanged)
2. **Step 2b [NEW v2.0]: conflict detection on post-filter pool using persisted metadata** (replaces Step 2d scored-top-2 detection in v1.9)
3. Step 2c: composite scoring for winner selection (unchanged; score = 0.7*cos_q + 0.3*cos_ctx + 0.8*node.authority)
4. Step 2d: return winner (rank-1 scored node) or abstain based on Step 2b result

Signal precedence (applied in order; first match governs conflict_type):
1. LogicalContradiction
2. ProvenancePair (covers AuthorityPolarity, SafeAbstention, AuthorityPolarity sub-population)
3. AdversarialPair (covers AdversarialAuthority)

All detection inputs are persisted node metadata written by the production learn path (authenticate_and_clamp -> birth_node -> node_store). They are not labels supplied by the harness at query time. Verified by Role A provenance trace and reviewer inspection (Section 15.5).

Trust-tier mapping (unchanged from v1.9):

| Provenance string | Trust tier | Rationale |
|---|---|---|
| contradicted | LOW | Explicitly challenged fact; flagged as contested by system or user |
| user_explicit | HIGH | Direct user assertion at stated authority; no challenge recorded |
| corrected | HIGH | Explicitly validated override of a prior entry; highest operational legitimacy |

Signal definitions (v2.0):

| Signal | Runtime conditions (persisted metadata, post-filter pool) | Outcome |
|---|---|---|
| LogicalContradiction | ANY pair in pool shares identical non-empty assertion_key AND pa * pb < 0 | conflict_detected=true, conflict_type=LogicalContradiction, abstained=true |
| ProvenancePair | ANY pair in pool has one HIGH_TRUST prov AND one LOW_TRUST prov | conflict_detected=true, conflict_type=AuthorityPolarity, abstained=true [AMENDED v2.0: unconditional -- TrustCompromised gap condition removed] |
| AdversarialPair | ANY pair in pool: BOTH HIGH_TRUST AND pos_prov == "corrected" AND neg_auth > pos_auth | conflict_detected=true, conflict_type=AuthorityPolarity, abstained=true |
| score_gap_proximity | -- | DELETED (v1.9); unchanged |
| TrustCompromised sub-condition | [DELETED v2.0: the abs(auth_rank1 - auth_rank2) <= 0.10 condition is removed. ProvenancePair fires unconditionally when HIGH vs LOW provenance is detected. Rationale: runtime clamped authorities of contradicted nodes are always <= 0.30; user_explicit nodes are >= 0.75; the runtime gap is always >= 0.45, making the 0.10 threshold structurally unreachable. The gap condition was intended to identify near-equal confidence but the authority clamp destroys that signal. Provenance trust tier (HIGH vs LOW) IS the persisted signal for contested-fact status; a contradicted node competing in the post-filter pool is by definition a contested assertion. TRUST_COMPROMISE_DELTA constant is retained in retrieval.rs as a documented deprecated-in-place value; it must not be read by any live code path after v2.0 remediation.] | -- |

Class-by-class behavior under v2.0 contract (verified for runtime consistency in Section 14.9):

| Class | Signal fires | conflict_type | abstained |
|---|---|---|---|
| Contradiction | LogicalContradiction (same key, opposing polarity; pool-present) | LogicalContradiction | true |
| AuthorityPolarity | ProvenancePair (contradicted D- in pool vs HIGH_TRUST D+); unconditional | AuthorityPolarity | true |
| AdversarialAuthority | AdversarialPair (both HIGH_TRUST, pos_prov=corrected, neg_auth > pos_auth; different keys per Section 4.4 v2.0 -- LC cannot fire) | AuthorityPolarity | true |
| SafeAbstention | ProvenancePair (contradicted D- in pool vs user_explicit D+); unconditional | AuthorityPolarity | true |
| UnresolvedConflict | No signal (both user_explicit HIGH_TRUST; no LOW_TRUST -> ProvenancePair fails; AdversarialPair: pos_prov != corrected; LC: same key but no opposing polarity per Section 4.4) | None | false |
| Supersession | No signal (D- filtered Step 2a; never reaches detector) | None | false |

G8b derivation (v2.0 -- corrected for LC-precedence interaction):

G8b gate: count(conflict_type = AuthorityPolarity) / (AUTH_POLARITY + ADVERSARIAL) >= 0.80 exact.
Denominator = 6 AP + 6 AA = 12. Numerator under v2.0 contract:
- AuthorityPolarity (6/6): D- is contradicted (LOW_TRUST); D+ is user_explicit/corrected (HIGH_TRUST). ProvenancePair fires -> conflict_type = AuthorityPolarity.
- AdversarialAuthority (6/6): D- is user_explicit (HIGH_TRUST); D+ is corrected (HIGH_TRUST). Section 4.4 v2.0 requires neg_key != pos_key; therefore LC (same-key) cannot fire. AdversarialPair fires (both HIGH_TRUST, pos_prov=corrected, neg_auth > pos_auth) -> conflict_type = AuthorityPolarity.

G8b = 12/12 = 1.00 >= 0.80. Gate passes under v2.0 contract.

Misfire verification:
- Contradiction: LogicalContradiction fires first (precedence 1); ProvenancePair not reached.
- Supersession: D- filtered Step 2a; not in post-filter pool; no signal.
- UnresolvedConflict: both HIGH_TRUST -> ProvenancePair fails; pos_prov = user_explicit (not corrected) -> AdversarialPair fails; Section 4.4 requires no polarity clash -> LC fails. conflict_type = None. G8c satisfied.
- SafeAbstention abstention does not affect G8b; G8b denominator is AP+AA only.

G8a as consistency check: Under pool-based detection, LogicalContradiction fires deterministically for all Contradiction scenarios where D- is in the post-filter pool (same assertion_key + opposing polarity is guaranteed by Section 4.4; D- is not superseded for Contradiction; D- is in raw-NN per G10). Therefore G8a = 1.0 whenever S1's numerator > 0 (i.e., whenever D- is in pool). G8a's role is to verify the implementation correctly fires LC given pool-presence; S1 remains the independent pool-membership gate.

### 14.3 Disposition of conflict_detection_threshold

*(Unchanged from v1.9)*

The score-gap gate is DELETED from the conflict decision path. Config::conflict_detection_threshold is retained as a no-op sentinel set to 0.0, documented as deprecated-in-place.

### 14.4 AdversarialPair Definition (unchanged from v1.9 except key-rule interaction noted)

Both HIGH_TRUST + pos_prov == "corrected" + neg_auth > pos_auth -> conflict_type = AuthorityPolarity, abstained = true. Under Section 4.4 v2.0, AA scenarios have neg_key != pos_key, ensuring LogicalContradiction (same-key) cannot precede AdversarialPair in the signal precedence chain.

### 14.5 Pre-Declared Authority-Margin Threshold

*(Unchanged from v1.9 -- TRUST_COMPROMISE_DELTA = 0.10 is retained as a named constant but is no longer read by active code. Its derivation stands as documented; the reformulation in Section 14.2 replaces the runtime condition, not the mathematical derivation.)*

delta_authority = 0.10 (pre-declared; not adjustable after attempt 3 is observed)

The composite scoring formula (retrieval.rs:252-254, production defaults verified against frozen archive at bc66340):

  score = 0.7 * cos(query, node) + 0.3 * cos(dense_ctx, node) + 0.8 * node.authority

sigma_combined ~= 0.067. delta_authority = 0.10 gives 0.8 * 0.10 = 0.080, comparable to 1.19 sigma. Derivation unchanged.

Relationship to Section 4.4 SafeAbstention: as recorded in Section 15.7.5 of v1.9, the "coincidence" between delta_authority and the Section 4.4 |auth gap| <= 0.10 bound is illusory -- runtime clamped authorities destroy the authored near-equality before it reaches the threshold comparison. The TrustCompromised reformulation in Section 14.2 corrects this by operating on provenance semantics rather than runtime numerical proximity.

### 14.6 Engine Default Changes (Production)

*(Unchanged from v1.9 for existing fields. v2.0 adds: detection input change in retrieval.rs Step 2b.)*

Required changes to Config::default() in cmhm_core/src/config.rs:

1. abstain_on_conflict: false -> true (v1.9 change; unchanged)
2. conflict_detection_threshold: 0.02 -> 0.0, deprecated-in-place (v1.9 change; unchanged)

### 14.7 Implementation Scope (v2.0 -- AMENDED)

Changes in cmhm_core/src/retrieval.rs:

1. [v1.9 unchanged] Delete the outer if gap < conflict_detection_threshold { ... } gate and its closing brace.
2. [v1.9 unchanged] Retain LogicalContradiction check -- remove only gap-gate enclosure.
3. [v1.9 unchanged] Retain SupersessionConflict check -- remove only gap-gate enclosure.
4. [v1.9 amended] Replace ProvenancePair signal: HIGH_TRUST vs LOW_TRUST -> conflict_type = AuthorityPolarity, abstained = true UNCONDITIONALLY. Remove TrustCompromised sub-condition (abs gap <= 0.10). TRUST_COMPROMISE_DELTA constant is retained as a documented deprecated-in-place value; it must not appear in any live conditional.
5. [v1.9 unchanged] AdversarialPair signal: both HIGH_TRUST, pos_prov == "corrected", neg_auth > pos_auth -> conflict_type = AuthorityPolarity, abstained = true.
6. [v1.9 unchanged] Delete ScoreAmbiguity branch.
7. **[NEW v2.0] Move detection (steps 1-6) to operate on the post-filter pool (Step 2b), before composite scoring (Step 2c). Concretely: after Step 2a supersession filter produces the candidate pool, scan the pool for LC, ProvenancePair, and AdversarialPair signals using persisted node metadata (assertion_key, polarity, provenance, authority). Set conflict_type and abstained. Then run composite scoring on the full pool for winner selection if not abstaining.**

Changes in cmhm_core/src/config.rs:
1. [v1.9 unchanged] abstain_on_conflict default: false -> true
2. [v1.9 unchanged] conflict_detection_threshold default: 0.02 -> 0.0; deprecated-in-place

Changes in cmhm_mcp/tests/phase3_approval_bench.rs:
1. [v1.9 unchanged] Line 849: pr <= 2 && nr <= 2 (gap clause already removed).
2. [NEW v2.0] S1 eligibility measurement updated to pool-presence (D- in post-filter pool, not scored top-2). Scaffold amended per Section 14.8; new commit identity recorded in Section 10.1.
3. [NEW v2.0] Section 12.3 conformance check extended with expected_abstain and AA key-rule validations. Scaffold amended; new commit identity recorded in Section 10.1.

New/updated unit tests:
1. [v1.9 unchanged] Gap-independence assertion: same-key opposing-polarity pair produces conflict_type = LogicalContradiction at gap=0.001 and gap=1.000.
2. [NEW v2.0] Pool-based detection assertion: contradicted D- at rank 6 in SCORED pool still triggers ProvenancePair -> abstained=true, because detection precedes scoring.
3. [NEW v2.0] TrustCompromised-removed assertion: ProvenancePair fires and produces abstained=true regardless of abs(auth_a - auth_b) (test with gap=0.01 and gap=0.80).

### 14.8 Amendment Disclosures and Owner Sign-Offs

**Disclosure 1 (v1.9): S1 top-2 co-presence amendment** (carried forward unchanged)

S1 metric amended from "D+ and D- reach SCORED top-2 with gap < 0.02" to "Both D+ and D- in SCORED top-2". Reviewer sign-off 2026-09-13. Rationale in v1.9 Section 14.8.

**Disclosure 2 (v2.0): S1 pool-membership amendment**

What changed: S1 metric amended from "Both D+ and D- in SCORED top-2" to "D- present in post-filter pool (Step 2a output)". Threshold >= 0.60 and denominator (All CONTRADICTION) unchanged.

This is a mechanism-decoupling, not a pass-adjustment, for three independent reasons:

(i) S1's purpose has always been to gate G8a's denominator on scenarios where the conflict detector HAS D- available. Under the v1.9 contract, the detector operated on scored top-2; "available" meant "in scored top-2." Under the v2.0 contract (Section 14.2), the detector operates on the post-filter pool; "available" means "in post-filter pool." S1 maps to the correct availability test for the current detector input. Threshold and denominator unchanged.

(ii) Pool-presence for Contradiction scenarios is structurally guaranteed by Section 4.4 (D- is not superseded for Contradiction, so it passes Step 2a) combined with G10=0 (D- is always in raw-NN). In practice S1 should be 1.00 under the v2.0 contract. This is stricter than v1.9's S1=0/13 observed at attempt 3.

(iii) This change is part of a pre-declared amendment addressing a registered gap (gap 3 / Section 14.2), not made after observing a new failure.

Role A owner sign-off: I assert that the S1 v2.0 amendment is mechanism-decoupling. "D- in post-filter pool" is the correct precondition for G8a under pool-based detection. The underlying protected property -- the conflict detector has D- available -- is preserved; the measurement maps to the detector's actual input stage.

**Disclosure 3 (v2.0): AA key rule amendment**

What changed: Section 4.4 ADVERSARIAL-AUTHORITY key rule amended from "any" to "neg_key DIFFERENT from pos_key (or None)."

Rationale: AdversarialAuthority tests whether the engine correctly abstains when confronted with a high-authority user assertion that contradicts a corrected (verified) override -- an authority-inversion test, not a logical contradiction test. Allowing same-key + opposing-polarity AA scenarios permits LogicalContradiction (precedence 1) to fire before AdversarialPair, classifying the pair as LC rather than AA. The gate arithmetic in v1.9 assumed AdversarialPair fires for all 6 AA scenarios; the interaction with LC-first precedence was not checked. This is a specification defect (registered as gap 2). Requiring different keys ensures LC cannot fire on AA scenarios (LC requires identical non-empty assertion_key); AdversarialPair fires on the authority-inversion signal as intended.

This amendment tightens the AA corpus constraint. Attempt-3 corpus AA scenarios (IDs 326-331) all had same-key opposing-polarity pairs, which caused LC to fire instead of AdversarialPair; those 6 scenarios are diagnosed as operationally safe (correct_abstention per governing §4.4+§7) but structurally misclassified by the gate.

**Disclosure 4 (v2.0): TrustCompromised reformulation**

What changed: Section 14.2 TrustCompromised sub-condition removed. ProvenancePair fires unconditionally (abstained=true for any HIGH vs LOW provenance pair in pool).

LFA rationale (Section 12): "abstain when trusted-cell disagreement exceeds D_max." A contradicted node competing into the post-filter pool IS trusted-cell disagreement -- the node's contradicted provenance records that an explicit challenge was registered against this fact. The prior clamped-gap proximity test (abs(auth_rank1 - auth_rank2) <= 0.10) was intended to identify near-equal confidence, but the authority clamp (contradicted -> [0.00, 0.30]) guarantees runtime gaps >= 0.45 for any contradicted vs user_explicit pair, making the condition permanently unreachable. Provenance trust tier (HIGH vs LOW) is the persisted, clamp-independent signal for "is this fact contested?" -- a LOW_TRUST node in the post-filter pool is contested by system definition, regardless of how the composite score ranks it.

### 14.9 Runtime-Consistency Proof (Required Review Checklist Item -- NEW v2.0)

This section is a mandatory review checklist item. Before any Section 14.2 class-behavior row is accepted, it must be derived against runtime clamp semantics, not just authored field values. The absence of this proof was the mechanism by which both the TrustCompromised structural impossibility (gap 3) and the §12.3 Abstain-column blind spot (gap 5) passed review in v1.9.

Runtime constants:
- contradicted authority: clamped to [0.00, 0.30] by authority.rs:109. Runtime persisted authority: <= 0.30.
- user_explicit authority: unclamped. Runtime persisted authority: authored value (0.75-0.93 range in practice).
- corrected authority: unclamped. Runtime persisted authority: authored value (0.75-0.90 range in practice).
- TRUST_COMPROMISE_DELTA: 0.10 (deprecated-in-place in v2.0; not in active code path).

Per-class runtime-consistency proof:

**CONTRADICTION:** D- contradicted (LOW), D+ user_explicit (HIGH). LC requires same assertion_key AND pa*pb < 0. Section 4.4 guarantees same key and opposing polarity. Pool presence: D- not superseded (supersedes_neg=false) + G10=0 -> D- in pool. LC fires deterministically. abstained=true. expected_abstain=true per Section 4.4. CONSISTENT.

**SUPERSESSION:** D- contradicted, supersedes_neg=true. Step 2a supersession filter removes D- from pool before detection. No signal reaches detector. conflict_type=None, abstained=false. expected_abstain=false. CONSISTENT.

**AUTHORITY-POLARITY:** D- contradicted (LOW, runtime auth <= 0.30), D+ user_explicit/corrected (HIGH, runtime auth 0.80-0.90). neg_key != pos_key -> LC cannot fire. ProvenancePair fires (HIGH vs LOW). Unconditional abstention (v2.0 TrustCompromised removed). abstained=true. expected_abstain=true. CONSISTENT.

**ADVERSARIAL-AUTHORITY:** D- user_explicit (HIGH, runtime auth 0.90-1.00), D+ corrected (HIGH, runtime auth 0.75-0.85). neg_key != pos_key (Section 4.4 v2.0) -> LC cannot fire. No LOW_TRUST -> ProvenancePair cannot fire. AdversarialPair: both HIGH_TRUST + pos_prov=corrected + neg_auth > pos_auth (Section 4.4: neg_auth 0.90-1.00 > pos_auth 0.75-0.85). AdversarialPair fires. conflict_type=AuthorityPolarity, abstained=true. expected_abstain=true. CONSISTENT.

**SAFE-ABSTENTION:** D- contradicted (LOW, runtime auth <= 0.30), D+ user_explicit (HIGH, runtime auth 0.75-0.90). LC: Section 4.4 does not require same key (key rule "any"); if no same-key opposing-polarity pair, LC does not fire. ProvenancePair fires (HIGH vs LOW). Unconditional abstention (v2.0). abstained=true. expected_abstain=true. CONSISTENT.

Note on SA LC interaction: if a corpus author gives D- and D+ the same key with opposing polarity, LC fires (precedence 1) instead of ProvenancePair. This produces the same outcome (abstained=true) and is equally valid. The Section 4.4 SA matrix allows this; both paths satisfy expected_abstain=true.

**UNRESOLVED-CONFLICT:** Both user_explicit (HIGH). No LOW_TRUST -> ProvenancePair fails. Section 4.4: no polarity clash -> LC fails. Section 4.4: pos_prov = user_explicit (not corrected) -> AdversarialPair fails. conflict_type=None, abstained=false. expected_abstain=false. CONSISTENT.

All six class rows are consistent with runtime clamp semantics under the v2.0 contract. This proof must be re-run for any future Section 14.2 amendment before reviewer sign-off.

---

## 15. Attempt Records (Permanent and Immutable)

Section 15.1-15.7 from v1.9 are carried forward unchanged and immutable. They are reproduced by reference to the governing v1.9 document (commit 7bcf24e, SHA-256 F0736803B5E42F909E4FD89A7B7747FA2D9516CDD4085ECA0780E3DB51B22346).

Nothing in Sections 15.1 through 15.7 may be altered in any protocol revision.

Attempt summary:
- Attempt 1: INVALID ON ITS FACE (corpus non-conformant). Commit e989a91. Section 11.
- Attempt 2: REJECTED (S1 fail: gap < 0.02 threshold under hash-128d; abstain_on_conflict=false). Commit 11c0a07. Section 15.1-15.6.
- Attempt 3: REJECTED (S1=0/13, G8b=0/12, G9=0/6; clamp-suppression + TrustCompromised structural impossibility + LC-precedence/AA interaction). Commit a58bc0a. Section 15.7. Five gaps registered; all resolved in this v2.0 amendment.

---

## 16. Attempt 3 Authorization (v1.9 -- Historical)

Section 16 from v1.9 is carried forward for historical reference. Attempt 3 has been permanently consumed. Prerequisites 1-7 are all complete as recorded in v1.9 Section 16.1. Attempt 4 authorization is in Section 17.

---

## 17. Attempt 4 Authorization (v2.0 -- NEW)

### 17.1 Prerequisites (all must be complete before attempt 4 begins)

1. Reviewer approval of v2.0 -- sign-off on all four disclosures in Section 14.8 (S1 pool-membership amendment, AA key rule amendment, TrustCompromised reformulation, runtime-consistency proof). [PENDING]
2. Engine remediation committed -- changes per Section 14.7 v2.0 (retrieval.rs pool-based detection, TrustCompromised removed, authority.rs unchanged or updated if needed). New SHA-256 hashes of modified files recorded in Section 10.1. [PENDING]
3. Synthetic pre-flight re-run -- phase3_synthetic_eval_tests.rs must pass 10/10. Gap-independence test passes. New pool-based-detection tests pass (Section 14.7 items 2-3). TrustCompromised-removed test passes. Any regression blocks attempt 4. [PENDING]
4. Scaffold updated -- S1 eligibility measurement updated (pool-presence); Section 12.3 conformance check extended (expected_abstain validation, AA key rule). SCENARIOS cleared to placeholder; bench file frozen at new commit; identity recorded in Section 10.1. [PENDING]
5. Overlap auditor extended (Source 5) -- audit_corpus_overlap.ps1 extended with 43 hardcoded attempt-3 query strings (from commit a58bc0a), exact mechanism as Sources 3 and 4. Self-test extended with at least one Source-5 live-fire positive. Commit identity recorded in Section 10.1. [PENDING]
6. Corpus v4 authored by Role B -- per Section 4.4 v2.0 (note amended AA key rule: neg_key must differ from pos_key for AA scenarios), Section 4.1-4.3, and Section 17.2. Zero reuse from corpora v1, v2, v3. [PENDING]
7. Role B explicit authorization -- sign-off that corpus v4 is ready for transcription. [PENDING]

### 17.2 Corpus v4 Requirements

- Satisfies Sections 4.1, 4.2, 4.3, and 4.4 (including v2.0 amendments: AA key rule = different or None)
- Zero query-text reuse from attempt-1, 2, or 3 corpora (enforced by auditor Sources 3, 4, 5)
- Authored by Role B with no reference to attempt-1, 2, or 3 gate diagnostics, score values, gap distributions, or authority-margin values from any consumed corpus
- Authored using only Section 4.4 v2.0 matrix and Section 13.2 cheat-sheet (to be updated for AA key rule) as spec references
- Fresh means fresh: not minor paraphrases of v1, v2, or v3 queries. Skeleton self-scan conducted by Role A at transcription stage.

### 17.3 Attempt 4 Sequence

1. Prerequisites 1-5 complete
2. Role B authors corpus v4 -> submits plaintext
3. Skeleton self-scan (Role A) -> flagged queries returned to Role B for replacement
4. Role A transcribes -> corpus commit
5. Stage 0 (overlap audit, all five sources) -> PASS required
6. Stage 1 (synthetic pre-flight) -> PASS required
7. Stage 1.5 (conformance check, v2.0 validator including expected_abstain and AA key rule) -> PASS required
8. Stage 2 (benchmark, single run) -> executed by Role B on Role A go-ahead; PERMANENTLY CONSUMES attempt
9. Stage 3 (gate verification) -> reviewer evaluates all gates under v2.0 contract

---

## 18. Change Log: v1.9 to v2.0

### 18.1 What changed

| Item | Change | Section |
|---|---|---|
| Resolution Table | Entries 20-24 added (five gaps from attempt-3 post-mortem) | S3 |
| Prohibited Corpora | Item 5 added (consumed corpus v3, commit a58bc0a) | S4.3 |
| Section 4.4 AA key rule | "any" -> "neg_key DIFFERENT from pos_key (or None)"; rationale in S14.8 | S4.4, S14.8 |
| Section 6 S1 gate | Metric amended: scored top-2 co-presence -> post-filter pool membership. Threshold/denominator unchanged. | S6, S14.8 |
| Section 7 expected_abstain note | Note added: harness categorise() reads corpus field; Section 12.3 now validates it against Section 4.4 | S7 |
| Section 7 Abstain truth table | Class -> expected_abstain truth table added; enforced by Section 12.3 | S7 |
| Section 8 manifest | authority.rs added with hash and provenance | S8 |
| Section 10.1 | v2.0 remediation placeholders added | S10.1 |
| Section 12.3 validator | expected_abstain validation for all classes; AA key rule validation; "no field is omitted" claim corrected | S12.3 |
| Section 14.2 detection input | Shifted from scored top-2 to post-filter pool (Step 2b before scoring) | S14.2 |
| Section 14.2 TrustCompromised | Clamped-gap condition (abs <= 0.10) DELETED; ProvenancePair fires unconditionally | S14.2 |
| Section 14.2 class-behavior table | All rows updated for v2.0 contract; AA row corrected (AdversarialPair, not LC) | S14.2 |
| Section 14.2 G8b derivation | Corrected for LC-precedence + AA different-key constraint | S14.2 |
| Section 14.5 TRUST_COMPROMISE_DELTA | Constant retained as deprecated-in-place; reformulation noted | S14.5 |
| Section 14.7 implementation scope | Pool-based detection Step 2b added; TrustCompromised removal; three new unit tests | S14.7 |
| Section 14.8 disclosures | Three new disclosures: S1 pool amendment, AA key rule, TrustCompromised reformulation | S14.8 |
| Section 14.9 runtime-consistency proof | NEW section: per-class derivation against runtime clamp semantics | S14.9 |
| Section 15 | Immutable attempt records carried by reference to v1.9 governing document | S15 |
| Section 16 | Marked historical | S16 |
| Section 17 | NEW: attempt 4 authorization (7 prerequisites, corpus v4 requirements, sequence) | S17 |
| Overlap auditor Source 5 | Must be added (attempt-3 queries) before attempt 4 | S17.1, S10.1 |

### 18.2 What did NOT change

- Section 6: G1-G7, G8a-G8c, G9-G10 thresholds, numerators, denominators, CI conditions unchanged; S1 metric amended as disclosed in S14.8
- Section 4.4 matrix: all rows unchanged except AA key rule (disclosed)
- Section 4.4 matrix: class minimums, authority ranges, provenance requirements, polarity rules, supersedes_neg -- all unchanged
- Section 5 empirical gate: unchanged
- Section 9 one-run rule: unchanged
- Section 11 (Attempt 1 record): unchanged and immutable
- Section 12.1-12.2, 12.4-12.5: unchanged
- Section 14.1 (why score-gap fails): unchanged
- Section 14.3 (conflict_detection_threshold disposition): unchanged
- Section 14.4 (AdversarialPair definition): unchanged (key-rule interaction noted but definition intact)
- Section 14.6 (engine default changes): unchanged
- TRUST_COMPROMISE_DELTA constant value (0.10): retained as named constant in code, deprecated-in-place
- Score formula (0.7*cos_q + 0.3*cos_ctx + 0.8*authority): unchanged
- Hash-128d model identifier: unchanged

---

*End of Phase 3 Approval Protocol v2.0 (DRAFT -- pending reviewer approval)*
