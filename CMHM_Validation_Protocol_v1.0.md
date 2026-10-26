# CMHM Validation Protocol

## Version 1.0 — Formal Standalone Document

> **Status**: Specification 3 of 3. Completes the Three-Spec Framework defined in
> `CMHM_Context_OS_Spec_v1.0.md` Part 15 (Memory Mathematics ✅ frozen · Context OS ✅ v1.0 · **Validation Protocol ✅ v1.0 — this document**).
> **Companion documents**: CMHM Memory Mathematics (`MATHCONTEXT.MD`) · CMHM Context OS Spec (`CMHM_Context_OS_Spec_v1.0.md`) · build-status map (`CMHM_Context_OS_Spec_v1.0_BUILD_STATUS.md`)
> **Freeze rule**: this protocol is fixed from v1.0 onward so improvements are measured consistently. Future amendments happen as **versioned protocol notes** (`PROTOCOL_v1.1_*.md` style) plus a version bump — never as edits to a concluded record.

---

## Preamble

The protocol answers one question, the same way, every time:

> **Does the engine help an agent re-find the right experience at decision time — measured mechanically, without any LLM in the loop, on a corpus that is checkable before the run starts?**

It encodes the discipline this repository already practices (pre-registered fixtures, single mechanical runs, immutable published records) and fixes it so future changes cannot quietly redefine "better".

**Claim scope (non-negotiable).** The system claim is *agent-assisted recall*: the runtime routes and retrieves; the calling agent reads, selects, and decides. The phrase "engine semantic recall" is never claimed anywhere. The bench harness itself never calls any LLM.

---

# Part 1 — Principles

| # | Principle | Enforcement |
|---|---|---|
| **P1** | **Pre-registration.** The fixture, its generator script, and every pass criterion are committed **before** the run. An engine-bits commit may follow (it must declare "all pre-run"). | Git history order is the proof; a run without a prior pre-registration commit does not count. |
| **P2** | **Single run.** One mechanical run per protocol instance. Reruns are new protocol instances with their own pre-registration. No cherry-picking across partial outputs. | Signed by the raw-output commit. |
| **P3** | **Immutable records.** Published numbers (README tables, verdict texts) are never edited after the fact. A changed claim requires a **new pre-registered instance** and appends to, or supersedes-with-note, the old row. | Strict review of every diff touching a published row. |
| **P4** | **No LLM in measurement.** Scoring is mechanical (rank arithmetic over known gold labels). Agent behavior (the tier-3 picker) is characterized, not scored, by the engine bench. | The run harness contains no model calls — discoverable from the committed script set. |
| **P5** | **Hygiene.** No vendor/model names in pushed artifacts (strict-pattern scan, zero tolerance). Personal runtime data (owner's own store, `backups/`, eval run scratch, `__pycache__`) never leaves the machine and is never committed. | Scan gate on every push. |
| **P6** | **Authorization.** Pushes are owner-GO-gated and each is flagged in the session report. | Session report. |
| **P7** | **Regression honesty.** A verdict that regresses any published record must state the delta explicitly in the verdict. Records are never silently dropped. | Verdict template (Part 7). |

---

# Part 2 — The Five Standing Experiment Families

Every bench instance is tagged to exactly one family. The families and their canonical, already-recorded forms:

| Family | Question | Canonical instance(s) |
|---|---|---|
| **E1 — Exact lookup** | Do stored node texts return rank-1 when the query is lexical? | Tier-1 corpus (`tools/gen_corpus_l1.py`, `tools/check_corpus_l1.py`): **100/100** on every recorded run |
| **E2 — Engine-alone hard retrieval** | Does the engine alone rank the recalled triple first under vague queries, given the save rule? | Tier-2w family: held-out benches (`cmhm_core/tests/phase3_held_out_bench.rs`, `honest_*.rs`) and the B13 ship-default arm (`tools/eval_b13`): **29/30** hard retrieval@1, CP95 [82.78%, 99.92%], pool 200/200, R@1 198/200 |
| **E3 — Agent-assisted published record** | What does the delivered stack achieve end-to-end, including the client-agent picker? | Tier-3 record on the published corpus: **14/30** (unchanged; idempotent across engine upgrades) |
| **E4 — Durability & restart** | Does a crash, restart, migration, backup, or shred preserve correctness? | `cmhm_core/tests/s05_restart.rs`, `backup_drill.rs`, `crypto_shred.rs`, WAL torn-tail suite; eval note `tools/eval_b8/r1-restart-idempotency.md` |
| **E5 — Safety channels** | Do abstention, conflict, authority, and save-discipline channels hold while ranking improves? | `cmhm_core/tests/retrieval_contract.rs` (8 gates incl. contradicted-node ineligibility, irrelevant-trusted displacement bound); authority weight sweep record (G1 42/43, MRR 0.988, on the phase-3 approval corpus, 43 scenarios × 60 distractors) |

A new bench that fits none of the five families requires a protocol amendment note first.

---

# Part 3 — Metrics

1. **R@1 (hard)** — fraction of queries whose gold node ranks strictly 1st under the pre-registered rank ceiling (default `retrieval_max_depth = 3`). The tier-2w headline metric.
2. **R@k** — gold node within top-k of the fused shortlist handed to the caller.
3. **MRR** — mean reciprocal rank on the labeled pool (recorded sweeps use MRR).
4. **Pool construction** — every R@1/R@k row states its pool: scenarios + distractors (e.g. "43 scenarios × 60 distractors", "pool 200/200 = 200 queries, 200 subscriptions").
5. **CP95 — Wilson score interval** (binomial proportion, two-sided, z = 1.959964; implemented in `tools/eval_b13/cp_29_30.py`):
   center = (p̂ + z²/2n)/(1 + z²/n); half-width = z/(1 + z²/n)·√(p̂(1−p̂)/n + z²/4n²)
   Every small-n headline carries its interval (e.g. 29/30 → [82.78%, 99.92%]).
6. **Channel checks** (E5, mandatory on rank-affecting changes):
   `lost_candidate` (gold left the shortlist), `lost_rank` (gold ranked worse), `lost_safety` (any E5 gate flipped).
7. **Gate shadow** — `would_index`/novelty classification recorded for every learn, so insertion behavior changes are visible without affecting the ledger.
8. **Regime labels** — every published number carries its regime: corpus id, embedder (hash-128d / MiniLM / BGE), fusion cap, save-rule on/off, ceiling depth, temperature-free.

---

# Part 4 — Ground Truth Generation

1. **Corpora are mechanical.** Written by committed generator scripts (`tools/gen_corpus_l1.py`, `tools/gen_pairs.py`, B-series `make_*_texts.py`). Corpus sources: repository source lines and synthetic/generic sentence templates only. **Personal memory data is prohibited as corpus.**
2. **Save-rule fixtures honor the caller-words rule.** Gold `state` text quotes the problem statement's own symptom phrasing (the discipline the engine itself teaches agents via the learn-path nudge).
3. **Paraphrase classes are pre-validated.** Precision-checked pair tables (`tools/validate_paraphrases.py` → `tools/validated_pairs.json`); paraphrase-hard buckets carry a **zero-overlap assertion** (no token overlap leak) verified before the run.
4. **Contamination gate.** The fixture's self-checks (overlap scan, label coverage, pool feasibility) must all print green in the pre-reg commit; a red self-check aborts before any engine run.

---

# Part 5 — Baselines and Arms

- **Embedder baselines:** hash-128d (dependency-free control), MiniLM (shipped default), BGE (secondary). A change is "better" only against the *shipped-default* baseline unless the pre-registration defines otherwise.
- **Arms:** rank-affecting changes run as paired arms (e.g. B13: {control-discipline, save-rule-discipline} × {fusion cap 20, 40}), each answering one pre-registered conditional (e.g. gate F: *cap-20 control must reproduce the standing MiniLM record exactly*).
- **Default flips** (engine-facing defaults, e.g. `retrieval_fusion_cap` 20 → 40) require: a pre-registered conditional flip clause **and** a post-flip green suite before publishing (core lib + retrieval contract + MCP tool tests).
- A baseline tie is a verdict ("cap 20 = MiniLM record exactly"), not a rounding detail.

---

# Part 6 — Run Anatomy (canonical)

1. **Pre-registration commit** — fixture generator, fixed corpus, pass criteria, conditional-flip clause, metric set, arms. (B13 anchor: run pre-reg `53bdaa3`.)
2. **Engine-bits commit** — the change under test, labeled *all pre-run*, after the fixture self-checks are green. (B13: `7713015`.)
3. **Single mechanical run** — no LLM calls; output captured verbatim. (B13: `tools/eval_b13/b13_run1.txt`.)
4. **Raw-output commit** — the run artifact as produced, unedited.
5. **CP artifact** — Wilson interval computed by the committed script, committed as a side artifact.
6. **Verdict commit** — the conditionals table (each pre-registered check → observed → PASS/FAIL), headline numbers with regime labels and intervals, explicit regression deltas. (B13: `c6196fe`.)
7. **Record pass** — only if a published README row changes. (B13: `af46816`; tier-3 14/30 left untouched.)
8. **Pushes** — owner-GO-gated, flagged one by one.

---

# Part 7 — Verdict Rules

- **PASS** requires *every* pre-registered conditional green — headline numbers do not rescue a red gate.
- **FAIL** is a verdict: it is committed with the same rigor as a pass, and the failed conditional names the next investigation.
- **Regression ledger**: every verdict lists published records it touches and their deltas; zero-delta is stated explicitly (B13 verification table did this).
- **Tier-3 published records** change only by appending a new pre-registered E3 instance (reviewer sign-off noted when required); they are never edited in place.

---

# Part 8 — What This Protocol Does *Not* Validate

- Part-11 scale targets of the Context OS spec (million-context fleets) — design targets, not measurements.
- Multi-agent concurrency behavior (covered by unit tests only; no published bench).
- Latency/throughput SLAs; onnx first-use model-download behavior on clean machines (that is a **distribution smoke gate** for release assets, not a bench).
- LLM-dependent memory quality — out of scope by P4; the tier-3 picker regime is characterized but not LLM-scored.
- **In-the-wild field validation**: published corpora are synthetic or curated; the tier-3 save-rule has no in-the-wild record yet. Field dogfooding is encouraged per the contributor docs and is categorized as *experience*, not as a bench number, until it gets its own pre-registered instance.

---

# Part 9 — Instance Registry

Authoritative notes live in `tools/eval_*`; this table anchors the framework.

| Instance | Family | On record |
|---|---|---|
| Corpus L1 | E1 | Tier-1 100/100 lexical, every recorded run |
| Held-out phase-3 / honest benches | E2 | Approval-corpus sweep record (G1 42/43, MRR 0.988, G4 0/43, α ≤ 0.5 property) |
| B13 ship-default | E2 (+E5 channels) | Pre-reg `53bdaa3` → bits `7713015` (all pre-run) → run `b13_run1.txt` → verdict `c6196fe` (29/30, CP95 [82.78%, 99.92%], pool 200/200, R@1 198/200, gate F tie, lost_candidate 12→0, lost_rank 16→1, zero lost_safety, cap flip 20→40) → README pass `af46816` (tier-2w row added; tier-3 14/30 unchanged) |
| Tier-3 published record | E3 | 14/30 with client-agent-as-reranker regime; idempotent across engine upgrades |
| B12 / B12-R | E3 prep | Caller-words save rule delivery; agent-text honoring verified |
| Restart idempotency (B8-R1) | E4 | r1-restart-idempotency note + suite |
| Retrieval contract | E5 | 8 standing gates, green on every engine-suite run |
| eval_d2 / eval_l3 / eval_w1 | E2 prep | Expansion / rerank / keyword-weight explorations (exploratory levels; any promotion to published requires this protocol's anatomy) |

---

# Part 10 — Amendment Procedure

1. Open an issue naming the gap (a metric, a family boundary, a corpus rule).
2. Write `PROTOCOL_v1.1_<topic>.md` with the delta and its motivating run.
3. The frozen text of `CMHM_Validation_Protocol_v1.0.md` never changes; broken cross-references are fixed inside the amendment note.
4. Instances already concluded keep their original protocol version forever.

---

*CMHM Validation Protocol v1.0 — Specification 3 of 3 complete.*
*Frozen: the method is now part of the product.*
