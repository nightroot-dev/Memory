# Phase 3 Approval Protocol v1.9

**Version:** 1.9
**Supersedes:** v1.8 (A9E8D8F17795D9457B3660881E715BBF77FEE869E34A0A4CCC935B196BDC70AA)
**Status:** DRAFT -- PENDING REVIEWER APPROVAL before governing

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

1. cmhm_core/tests/dprime_bench.rs -- 15-query rejection set
2. cmhm_core/tests/phase3_held_out_bench.rs -- 32-query compromised corpus
3. Attempt-1 corpus (commit e989a91) -- consumed; zero query reuse (enforced by auditor Source 3)
4. Attempt-2 corpus (commit 11c0a07) -- consumed diagnostic corpus; zero query reuse (enforced by auditor Source 4)

### 4.4 Complete Scenario-Class Matrix

*(Unchanged from v1.8 -- this section may not be altered in response to gate results)*

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

GOVERNING CONSTRAINT: No gate may be removed, weakened, or reinterpreted after observing a failure.

Note: S1 metric is amended in this version to remove the score-gap clause. Disclosure, owner sign-off, and reviewer confirmation are recorded in Section 14.8.

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
| S1 | LC Eligibility Rate | Both D+ and D- in SCORED top-2 | All CONTRADICTION (N >= 10) | >= 0.60 | Exact (>= 60%) |
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

config.rs is listed as NEW v1.9: it was omitted from the v1.8 manifest table. The hash above is the state at commit 551121ee. It will change after Section 14.6 defaults are applied; the post-remediation hash will be recorded in Section 10.1.

Post-freeze harness changes (outside manifest; do not affect the implementation under test):

| File | Change | Commit |
|---|---|---|
| cmhm_mcp/tests/phase3_approval_bench.rs | Scaffold authored; corpus v2 transcribed; g5_slots defect fixed | 984bef7, 1b07c83 |
| scripts/audit_corpus_overlap.ps1 | PS empty-List unrolling bug fixed; pos_text/neg_text field aliases added; Source 3 (43 attempt-1 queries) + live-fire self-tests added | e989a91, 57e31a0 |
| scripts/verify_phase3_approval.ps1 | Stage 1.5 conformance gate added | a80457d |

Note on post-freeze script hashes: the working-tree hashes of audit_corpus_overlap.ps1 and verify_phase3_approval.ps1 differ from the archive hashes above because both scripts grew after bc66340 (Source 3 additions, Stage 1.5 integration). Current working-tree hashes: audit_corpus_overlap.ps1 = 20,197 bytes / 805C425C71ACBF18154813178B6D836AD3982CD41FBC20EF1F8C3F6CCC1A3F59; verify_phase3_approval.ps1 = 20,647 bytes / 702E1D8DD6943B497E0184705857B8A7DCAD9744E925B1D14EABE2F6A11CC8D2. These will be updated again when Source 4 is added (Section 16.1 prerequisite 5).

---

## 9. Multi-Stage Lifecycle and The One-Run Rule

*(Core lifecycle unchanged from v1.8; Harness Configuration Mandate is new in v1.9)*

Stage 0 (Pre-Flight): Clean working tree + synthetic self-tests. Failures do not consume an attempt.

Stage 1 (Overlap Audit): scripts/audit_corpus_overlap.ps1 against prohibited corpora. Any violation rejects corpus and terminates the attempt before Stage 2.

Stage 1.5 (Conformance Validation): phase3_conformance_check Rust test (Section 12). Validates every scenario against the Section 4.4 class-spec matrix. Any violation fails CLOSED before Stage 2. The attempt is NOT consumed by a Stage 1.5 failure.

Stage 2 (Single Benchmark Execution): scripts/verify_phase3_approval.ps1. PERMANENTLY CONSUMES the single approval attempt. No retries permitted.

Stage 3 (Gate Verification): Verifier evaluates all gates; exits 0 only on unanimous passage.

HARNESS CONFIGURATION MANDATE (v1.9): The benchmark harness must run with Config::default() -- no field overrides. The detection contract in Section 14 is expressed as changes to Config::default() values, not as harness overrides. The only permissible Config field overrides are data_dir and projections_dir (tempdir paths required for test isolation). Any other Config override invalidates the run. This rule applies to attempt 3 and all future attempts.

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
| v1.9 remediation commit | to be set after Section 14 engine changes (retrieval.rs + config.rs) |
| v1.9 scaffold freeze commit | to be set after S1 bench re-scope + SCENARIOS cleared for corpus v3 |

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

*(Unchanged from v1.8)*

### 12.1 Purpose

Validates every scenario against Section 4.4 before Stage 2 executes.

### 12.2 Implementation

  cargo test --package cmhm_mcp --test phase3_approval_bench phase3_conformance_check -- --nocapture

### 12.3 Per-Scenario Validation Rules

*(Full rules unchanged from v1.8 -- see v1.8 Section 12.3, permanently retained in repo at docs/phase3_approval_protocol_v1.8.md, for complete text.)*

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

This section specifies the production contract for conflict detection and abstention, replacing the score-gap proximity mechanism in frozen retrieval.rs.

### 14.1 Why Score-Gap Proximity Fails for hash-128d

The frozen retrieval.rs conflict gate (line 291) fires only when the top-2 composite scores are within conflict_detection_threshold of each other. This assumes conflicting memories produce near-indistinguishable embeddings -- that contradiction is a proximity phenomenon in embedding space.

FNV-1a hash embeddings (hash-128d) have no semantic geometry. The embedding of a text is a deterministic function of its byte sequence, not its meaning. Semantically opposite text produces embeddings as far apart as unrelated text. Conflict is therefore not detectable from score proximity under hash-128d; a workable threshold would need to exceed the typical benign top-2 gap (~0.40), which is structurally equivalent to firing on all pairs indiscriminately.

This is an embedding-theoretic argument. Attempt-2 gap distributions corroborate it but do not constitute the justification. The argument stands independently of any corpus.

### 14.2 Per-Signal Detection Contract

Detector input: post-Step-2a SCORED top-2 candidates only. Superseded nodes are filtered by Step 2a before scoring. Detection therefore cannot fire on a Supersession D- that was eliminated upstream, preventing spurious conflict signals on Supersession class scenarios.

Signal precedence (applied in order; first match governs conflict_type):
1. LogicalContradiction
2. ProvenancePair (covers AuthorityPolarity and SafeAbstention)
3. AdversarialPair (covers AdversarialAuthority)

All detection inputs are persisted node metadata written by the production learn path (authenticate_and_clamp -> birth_node -> node_store). They are not labels supplied by the harness at query time. Verified by Role A provenance trace and reviewer inspection (Section 15.5).

Trust-tier mapping:

| Provenance string | Trust tier | Rationale |
|---|---|---|
| contradicted | LOW | Explicitly challenged fact; flagged as contested by system or user |
| user_explicit | HIGH | Direct user assertion at stated authority; no challenge recorded |
| corrected | HIGH | Explicitly validated override of a prior entry; highest operational legitimacy |

Note on corrected in HIGH_TRUST: corrected provenance is assigned when a user or verified process explicitly overrides a prior entry. It represents the system's strongest trust signal and sits above user_explicit in operational legitimacy.

Signal definitions:

| Signal | Runtime conditions (persisted metadata, post-filter SCORED top-2) | Outcome |
|---|---|---|
| LogicalContradiction | top-2 share identical non-empty assertion_key AND pa * pb < 0 | conflict_detected=true, conflict_type=LogicalContradiction, abstained=true |
| ProvenancePair | One of top-2 is HIGH_TRUST prov AND the other is LOW_TRUST prov (any ordering) | conflict_detected=true, conflict_type=AuthorityPolarity; abstained=true iff TrustCompromised holds |
| TrustCompromised (sub-condition of ProvenancePair) | ProvenancePair fires AND abs(auth_rank1 - auth_rank2) <= delta_authority (0.10) | abstained=true (requires abstain_on_conflict=true in Config) |
| AdversarialPair | BOTH top-2 are HIGH_TRUST AND pos_prov == "corrected" AND neg_auth > pos_auth | conflict_detected=true, conflict_type=AuthorityPolarity, abstained=true |
| score_gap_proximity | -- | DELETED from decision path; see Section 14.3 |

Class-by-class behavior under contract:

| Class | Signal fires | conflict_type | abstained |
|---|---|---|---|
| Contradiction | LogicalContradiction (same key, opposing polarity) | LogicalContradiction | true |
| AuthorityPolarity | ProvenancePair (contradicted D- vs HIGH_TRUST D+); TrustCompromised fires iff abs(auth gap) <= 0.10 | AuthorityPolarity | false (typical; see note) |
| AdversarialAuthority | AdversarialPair (both HIGH_TRUST, pos_prov=corrected, neg_auth > pos_auth) | AuthorityPolarity | true |
| SafeAbstention | ProvenancePair (contradicted D- vs user_explicit D+); abs(auth gap) <= 0.10; TrustCompromised fires | AuthorityPolarity | true |
| UnresolvedConflict | No signal (both user_explicit; no LOW_TRUST; AdversarialPair: pos_auth > neg_auth fails condition) | None | false |
| Supersession | No signal (D- filtered Step 2a; never reaches detector) | None | false |

Note on AuthorityPolarity abstention: Section 4.4 permits |pos_auth - neg_auth| in the range 0.00-0.20 for AP scenarios (D+ 0.80-0.90, D- 0.70-0.80). Conformant AP scenarios with |auth gap| <= 0.10 satisfy TrustCompromised and will abstain. This produces correct_abstention for those scenarios (AP expected_abstain=true per §4.4), not incorrect_abstention -- the class-level Abstain column in §4.4 is true, so the engine abstaining is the expected behavior. The class-table "false (typical)" refers to the sub-population with auth gap > 0.10; the abstaining sub-population is equally accepted behavior. No gate penalizes either outcome.

G8b derivation (verified against Section 6 gate definition):
G8b gate: count(conflict_type = AuthorityPolarity) / (AUTH_POLARITY + ADVERSARIAL) >= 0.80 exact.
Denominator = 6 AP + 6 AA = 12. Numerator:
- AuthorityPolarity (6/6): ProvenancePair fires -> conflict_type = AuthorityPolarity.
- AdversarialAuthority (6/6): AdversarialPair fires -> conflict_type = AuthorityPolarity.
G8b = 12/12 = 1.00 >= 0.80. Gate passes.

Misfire verification:
- Contradiction: LogicalContradiction fires first (precedence 1); ProvenancePair not reached.
- Supersession: D- filtered Step 2a; not in SCORED top-2; no signal.
- UnresolvedConflict: both HIGH_TRUST, pos_auth > neg_auth -> AdversarialPair (neg_auth > pos_auth) fails; no LOW_TRUST -> ProvenancePair fails; no key clash -> LogicalContradiction fails. conflict_type = None. G8c satisfied.
- SafeAbstention abstention does not affect G8b; G8b denominator is AP+AA only.

G8a as consistency check: Under the metadata-only detection contract, LogicalContradiction fires deterministically whenever a Contradiction pair is jointly in SCORED top-2 (same assertion_key + opposing polarity is guaranteed by §4.4 for all Contradiction scenarios). Therefore G8a = 1.0 whenever S1's numerator > 0 -- G8a is no longer an independent capability measure. Its role under the new contract is to verify the implementation correctly fires LC given co-presence; the effective contradiction-safety gate is S1 alone (which permits up to 40% of Contradiction scenarios to miss top-2 entirely and return an un-flagged answer -- exposure that existed in v1.8 and is unchanged). Future readers must not credit G8a with independent evidentiary weight beyond implementation correctness.

### 14.3 Disposition of conflict_detection_threshold

The score-gap gate (`if gap < conflict_detection_threshold { ... }`) is DELETED from the conflict decision path in retrieval.rs -- this is a code deletion, not a re-configuration. The gap condition no longer participates in any conflict classification branch.

Config::conflict_detection_threshold is retained for schema compatibility. Its value in Config::default() is set to 0.0 as a no-op sentinel. The field is documented in config.rs as deprecated-in-place.

A unit test must be committed before attempt 3, asserting gap-independence: two nodes satisfying a signal condition (e.g. same assertion_key, opposing polarity) must produce the same conflict_type regardless of score gap (test with gap=0.001 and gap=1.000). This is the enforcement guarantee that the gap gate cannot silently re-enter the decision path.

### 14.4 AdversarialPair Amendments (from initial draft)

Amendment 1 -- Abstention required: Section 4.4 Abstain column for ADVERSARIAL-AUTHORITY is true. Section 11.5 (v1.8): "the engine must abstain rather than blindly trust the high-authority D-." The AdversarialPair signal therefore includes abstained=true (via abstain_on_conflict production default).

Amendment 2 -- Signal tightened: "both HIGH_TRUST + neg_auth > pos_auth" fires on ordinary recency (newer user_explicit outranking older user_explicit). Amended to require pos_prov == "corrected", uniquely identifying the corrected-overridden-by-user_explicit adversarial pattern without misfiring on same-trust recency.

### 14.5 Pre-Declared Authority-Margin Threshold

delta_authority = 0.10 (pre-declared; not adjustable after attempt 3 is observed)

Derivation from scoring mathematics:

The composite scoring formula (retrieval.rs:252-254, production defaults verified against frozen archive at bc66340):

  score = 0.7 * cos(query, node) + 0.3 * cos(dense_ctx, node) + 0.8 * node.authority

Production defaults confirmed: query_weight=0.7, context_weight=0.3, authority_weight=0.8, conflict_detection_threshold=0.02, abstain_on_conflict=false (config.rs:172-179).

The authority contribution to composite score differential is 0.8 * |auth_a - auth_b|.

Under hash-128d (FNV-1a), embeddings have no semantic geometry -- cosine similarity of a semantically distinct node with a query is noise uncorrelated with meaning. For a D-dimensional random unit vector, the standard deviation of the cosine with another random unit vector is sigma = 1/sqrt(D). For D=128: sigma_cosine = 1/sqrt(128) ~= 0.0884 per component term.

With two cosine terms, the combined score noise standard deviation (assuming query and context embeddings are independent noise) is:

  sigma_combined = sqrt(0.7^2 + 0.3^2) / sqrt(128) ~= sqrt(0.58) / 11.314 ~= 0.0672

The crossover: below what authority margin can a combined cosine-noise realization (1 sigma) reverse the authority-based ordering?

  0.8 * |auth_a - auth_b| < sigma_combined
  |auth_a - auth_b| < 0.0672 / 0.8 ~= 0.084

Below this margin, the authority differential contributes less to composite score than a one-sigma combined cosine noise unit -- the winner cannot be confidently preferred on authority grounds alone within the typical noise band. Rounding to one decimal place conservatively: delta_authority = 0.10 (at gap=0.10, authority contributes 0.080, comparable to the 0.067 combined noise unit (≈ 1.19σ)).

Relationship to Section 4.4 SafeAbstention constraint: Section 4.4 specifies |pos_auth - neg_auth| <= 0.10 for SafeAbstention. delta_authority = 0.10 coincides with this bound. The two are independently derived: Section 4.4's bound was set to express "near-equal authority" from a corpus-design perspective; delta_authority was derived from the scoring mathematics (at what gap does authority fail to exceed one-sigma combined cosine noise?). They converge at 0.10 because they model the same physical boundary from different angles. This coincidence is stated here and must not be cited as a tuning justification in any future protocol version.

Implementation note: 0.10 must be implemented as a named constant (e.g., TRUST_COMPROMISE_DELTA) in retrieval.rs with a doc comment citing Section 14.5. It must not appear as an inline literal to prevent silent drift in future edits.

### 14.6 Engine Default Changes (Production)

Required changes to Config::default() in cmhm_core/src/config.rs:

1. abstain_on_conflict: false -> true

   Rationale: LFA Section 12 -- abstain when trusted-cell disagreement exceeds D_max. An engine that detects conflict but does not abstain presents a contested answer as settled, violating the LFA abstention contract. Must be production default. Benchmark harness runs Config::default() unchanged; change affects production and benchmark simultaneously. No harness override.

2. conflict_detection_threshold: 0.02 -> 0.0

   No-op sentinel; gap gate is deleted from code. Retained for schema compatibility only; documented as deprecated-in-place. See Section 14.3.

### 14.7 Implementation Scope

Changes in cmhm_core/src/retrieval.rs (Step 2d):

1. Delete the outer `if gap < conflict_detection_threshold { ... }` gate and its closing brace.
2. Retain LogicalContradiction check unchanged -- remove only gap-gate enclosure.
3. Retain SupersessionConflict check unchanged -- remove only gap-gate enclosure. (Defensive; D- filtered Step 2a, but retained against future Step 2a changes.)
4. Replace existing provenance-pair check with ProvenancePair signal (Section 14.2): HIGH_TRUST vs LOW_TRUST, regardless of gap. Add TrustCompromised sub-condition: if abs(auth_rank1 - auth_rank2) <= 0.10 (using named constant TRUST_COMPROMISE_DELTA), set should_abstain = true.
5. Add AdversarialPair signal (Section 14.2): both HIGH_TRUST, pos_prov == "corrected", neg_auth > pos_auth -> conflict_type = AuthorityPolarity, abstained = true.
6. Delete ScoreAmbiguity branch (fallthrough of old gap-gated provenance check; not a signal in new contract; no gate in Section 6).

Changes in cmhm_core/src/config.rs:

1. abstain_on_conflict default: false -> true
2. conflict_detection_threshold default: 0.02 -> 0.0; doc comment updated to "deprecated-in-place; no effect after v1.9 remediation"

Changes in cmhm_mcp/tests/phase3_approval_bench.rs (S1 re-scope):

1. Line 849: replace `pr <= 2 && nr <= 2 && (ps - ns).abs() < 0.02` with `pr <= 2 && nr <= 2`
2. Comment at line 844: update to reflect S1 v1.9 metric (top-2 co-presence, gap clause removed)
3. Re-freeze scaffold after this change; new commit identity recorded in Section 10.1

New unit test (cmhm_core/tests/ or cmhm_mcp/tests/):

Gap-independence assertion: same-key opposing-polarity pair must produce conflict_type = LogicalContradiction at gap=0.001 and at gap=1.000. Required before attempt 3.

### 14.8 S1 Scope Amendment -- Disclosure and Owner Sign-Off

What changed: S1 metric in Section 6 amended from "D+ and D- reach SCORED top-2 with gap < 0.02" to "Both D+ and D- in SCORED top-2". Gap < 0.02 clause removed. Threshold (>= 0.60) and denominator (All CONTRADICTION) unchanged.

This is a mechanism-decoupling, not a pass-adjustment, for four independent reasons:

(i) v1.8 Section 4.4 states the gap clause's role: "D- auth 0.70-0.85 ensures near-equal composite score with D+ for S1 eligibility (gap < 0.02 in SCORED top-2)." The clause was eligibility bookkeeping for the score-proximity detector. Removing the detector removes the clause's referent; retaining it would gate G8a's denominator on a measurement with no remaining meaning.

(ii) The protected property S1 was always tracking is embedder-agnostic: the conflicting pair must be jointly present in SCORED top-2 so any detector has both candidates available. The gap sub-clause was a proxy under semantic embedders; it is not a proxy under hash-128d (attempt-2 showed G2=43/43 -- both candidates retrieved -- while S1=0/13 -- gap too large). "Both D+ and D- in SCORED top-2" directly measures the property the gate protects.

(iii) This change is made before attempt 3 exists. It is not made after observing failure under the new detection contract.

(iv) This change does not relax the threshold (>= 0.60 unchanged) or denominator (All CONTRADICTION unchanged).

Role A owner sign-off: I assert that the S1 amendment is mechanism-decoupling. The gap < 0.02 clause is removed because its referent (score-proximity detection) is removed on embedding-theoretic grounds. The remaining metric measures the co-presence precondition that G8a requires and that any top-2-based detector depends on.

Reviewer sign-off (2026-09-13): The reviewer independently verified against governing v1.8 (§6 S1 row, §4.4 note) that the removed gap < 0.02 clause was eligibility bookkeeping for the score-proximity detector deleted by Section 14, that threshold and denominator are unchanged, and that the amendment widens G8a's denominator (strictening it). Confirmed: mechanism-decoupling, not gate weakening. The protected property — joint top-2 co-presence for adjudication — is embedder-agnostic and is what the amended metric measures.

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
| Stage 1.5 conformance | PASS -- CONFORMANCE_COUNTS\|N=43\|Contradiction:13,Supersession:6,AuthorityPolarity:6,AdversarialAuthority:6,SafeAbstention:6,UnresolvedConflict:6 |
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
| S1 (v1.8 metric) | FAIL | 0.0000 | 0.0000 | 0.0000 | 0/13 |
| G8a | PASS | 0.0000 | 0.0000 | 0.0000 | 0/0 eligible |
| G8b | FAIL | 0.0000 | 0.0000 | 0.0000 | 0/12 |
| G8c | PASS | 1.0000 | 1.0000 | 1.0000 | 6/6 |
| G9 | FAIL | 0.0000 | 0.0000 | 0.0000 | 0/6 |
| G10 | PASS | 0.0000 | 0.0000 | 0.0000 | 0 violations / 37 non-supersession |

Gates passed: 10/13. Gates failed: S1, G8b, G9. (S1 measured under v1.8 metric definition.)

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

1. Reviewer approval of v1.9 -- including sign-off on S1 amendment as mechanism-decoupling (Section 14.8).
2. Engine remediation committed -- changes per Section 14.7 (retrieval.rs + config.rs). New SHA-256 hashes of modified files recorded in Section 10.1.
3. Synthetic pre-flight re-run -- phase3_synthetic_eval_tests.rs must pass 10/10 after Section 14 changes. Gap-independence unit test passes. Any regression blocks attempt 3.
4. Scaffold updated -- bench line 849 updated (S1 re-scope); SCENARIOS cleared to placeholder; bench file frozen at new commit; identity (commit hash + SHA-256 + byte size) recorded in Section 10.1.
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
| Resolution Table | Entries 16-19 added | S3 |
| Prohibited Corpora | Items 3-4 added (consumed corpora v1 and v2) | S4.3 |
| Section 6 S1 gate | Metric amended: gap < 0.02 clause removed; top-2 co-presence only. Threshold/denominator unchanged. | S6, S14.8 |
| Harness Configuration Mandate | Config::default() override prohibition; data_dir and projections_dir only exceptions | S9 |
| Manifest table | Reproduced verbatim from v1.8; config.rs added as NEW v1.9 | S8 |
| Scaffold freeze table | v1.8 identity recorded; v1.9 placeholders added | S10.1 |
| Manifest NOTE | Pre-discloses retrieval.rs/config.rs hashes will change post-remediation | S8 |
| Conflict-Detection Contract | Per-signal metadata contract; gap gate DELETED (not re-configured); trust-tier table; signal precedence; post-filter input requirement; delta_authority=0.10 derived from scoring math; AdversarialPair with abstention and tightened condition; gap-independence unit test required | S14 (NEW) |
| Config defaults | abstain_on_conflict -> true; conflict_detection_threshold -> 0.0 deprecated-in-place | S14.6 |
| Attempt 2 Record | Permanent: facts, gate results (S1 under v1.8 metric noted), REJECTED ruling, root cause, provenance trace | S15 (NEW) |
| Attempt 3 Authorization | Prerequisites (incl. gap-independence test, Source 4 auditor extension), corpus v3 requirements, sequence | S16 (NEW) |
| Corpus v2 section | Marked historical | S13 |

### What did NOT change

- Section 6: G1-G7, G8a-G8c, G9-G10 unchanged (threshold, numerator, denominator, CI condition); S1 metric amended as disclosed in S14.8
- Section 4.4 matrix: unchanged
- Section 12 validator: unchanged
- Section 11 (Attempt 1 record): unchanged and immutable
- Section 2 (Role assignments): unchanged
- Section 9 one-run rule: unchanged (mandate is additive)
- Section 5 empirical gate: unchanged

---

*End of Phase 3 Approval Protocol v1.9 (DRAFT -- amended per reviewer ruling 2026-09-13)*
