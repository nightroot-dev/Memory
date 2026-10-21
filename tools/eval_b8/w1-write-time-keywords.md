## W1 RUN — PART 1 verdict (2026-10-21, single run per leg, flagged)

Run: `cargo +stable-x86_64-pc-windows-msvc test -p cmhm_core --target
x86_64-pc-windows-msvc --features onnx --test ops_bench_w1_kw_v1 -- --ignored
--nocapture` (fixture commit 633e9cd; raw output tools/eval_w1/w1_run1.txt).
Both legs in one instrument, 40 s. No deviations; one cosmetic unused-var
warning in the harness (noted, not fixed-mid-run).

### W1b (blind control) — CLEAN, byte-level identical to the MiniLM record

| group | fused seedP@1 | poolRec | MRR |
|---|---|---|---|
| lexical | 1.0000 (100/100 guard ✓) | 1.0000 | 1.0000 |
| para-easy | 0.2069 (6/29) | 0.6207 | 0.2667 |
| para-medium | 0.1463 (6/41) | 0.6829 | 0.2553 |
| para-hard | 0.0667 (2/30) | 0.6000 (18/30) | 0.1689 |

Aggregates: pool 164/200, R@1 114/200 — equals the B10 Step-1 record
exactly. Attribution hard: 12 lost_candidate / 16 lost_rank — matches the
record's "hard: 12 candidate / 16 rank of 28". Store-side keywords that add
no new vocabulary change NOTHING — the gate math is genuinely about the
bridge, not about adding text.

Gates W1b: A ✓ B ✗(2<15) C ✗(164<170) D ✗(114<140) E ✓(164>=164) → FAIL.

### W1a (transcript) — PAIR RESTORATION 100%, top-1 CONVERSION 0

| group | fused seedP@1 | poolRec | MRR |
|---|---|---|---|
| lexical | 1.0000 (100/100 guard ✓) | 1.0000 | 1.0000 |
| para-easy | 0.2069 (6/29 — unchanged) | **1.0000** (0.6207 record) | 0.2900 |
| para-medium | 0.1463 (6/41 — unchanged) | **1.0000** (0.6829) | 0.2741 |
| para-hard | 0.0667 (2/30 — unchanged) | **1.0000** (30/30!) | 0.1909 |

Aggregates: pool **200/200** (record 164), R@1 **114/200 — unchanged**.
Attribution after the bridge: lost_candidate = **0 in every paraphrase
group**; para-hard misses are now 28/28 lost_rank (were 12 cand + 16 rank).

Gates W1a: A ✓ C ✓(200>=170) E ✓(200>=164) — but B ✗(2<15) and D ✗(114<140).
**W1a FAILS the standing gates.** Pre-registered channel check honored: the
gain arrived exactly through the candidate channel as predicted, but the
rank leg converted none of it — reported as the finding, not a headline.

### Findings (measured, in scope of this instrument only)

1. **The candidate-generation wall is REMOVABLE at write time, with zero
   engine changes**: caller-phrasing keywords in the learn text bring every
   corpus item within reach of the fused path (pool 164→200/200; hard pool
   18→30/30).
2. **The remaining wall is the final ranker**, and it is strong: an exact
   BM25-grade keyword match inside the indexed text loses the top-1 race to
   the dense/RRF signal in every one of the 84 paraphrase misses. Same wall
   where B13's rerankers (net 0/1 vs +10 bar) failed; now measured against a
   FULLY populated pool — the pair (W1 bridge + reranker) is now the only
   in-class combination whose pieces exist, and half of it is proven.
3. Store-side-only keywords (W1b) contribute nothing — the fix's value is
   conversational vocabulary, not added text.
4. Upper-bound framing per pre-registration: W1a keywords match query
   tokens by construction; it measures the CEILING of the save-time
   transcript discipline on this corpus, not a production guarantee.

Standing verdict unchanged; lever 1 = FAIL under the standing gates, with
the pool-restoration result recorded as a real finding. Handing back:
lever 2 (D2 query expansion, pre-registered, gate hard >= 21/30) fires
next only on the owner's go. B12-waiver note to be registered THEN.

---

# W1 pre-registration — write-time keyword enrichment (owner-approved lever 1/3)

Owner directive 2026-10-21: give each recall lever a chance, one by one,
order (1) write-time keywords, (2) query expansion (D2), (3) bigger
embedder/LLM rerank. After each lever: verdict back to owner BEFORE the next
fires. This file registers lever 1 BEFORE any measurement. No run may start
before both legs' fixtures + harness are committed (pre-registration rule).

## W1 hypothesis (two legs, one instrument)

The paraphrase failure is a TEXT BRIDGE problem, not necessarily a model
problem: the store has the technical words, the query has the user's
everyday words, and BM25 (which is 100% on lexical overlap) cannot cross.
W1 tests what happens when the memory TEXT carries bridge keywords at learn
time. Zero engine changes: keywords are appended INSIDE the learn text the
caller supplies (raw situation text); retrieval stays untouched and still
runs on the ORIGINAL query only.

- **W1a (transcript leg, the fix candidate)**: at save time the agent has
  just heard the user's phrasing, so the product-real keyword source is the
  conversation itself. Bench-model: keywords are derived from the query
  tokens of the corpus item. This is NOT a blind test — it measures the
  CEILING of the "agent transcribes the user's words into the memory"
  product discipline. If W1a PASSES the standing gates, the product lesson
  is a save-time rule ("memories must carry the caller's phrasing"), not an
  engine change.
- **W1b (blind control leg, mechanism attribution)**: keywords derived from
  the STORE side only (situation tokens), zero query visibility. BM25
  already indexes those tokens, so W1b is expected ~flat on para-hard. If
  W1b passes the hard gate, the bridge story is WRONG and the finding
  changes — log it, do not bury it.

## Authorship rules (registered before authorship)

- The ONLY fixture source is the committed `tools/corpus_l1.json` (n=200,
  100 lexical / 29 para-easy / 41 para-medium / 30 para-hard).
- Generation is fully mechanical by the committed script
  `tools/eval_w1/make_keywords.py` run once; its output
  `tools/eval_w1/corpus_l1_kw.json` is committed byte-for-byte. No manual
  keyword edits. The builder does not touch the generated file afterward.
- Format appended at learn time, situation text only (single slot so the
  effect is attributable): `{situation} keywords: {keywords}`.
- Action/outcome learn text stays byte-identical to the MiniLM record
  harness (including the `{idx}` suffix that separates duplicates).
- Retrieval still uses the original `query` string only. Keywords are
  indexed, never queried.
- No LLM produces keywords (mechanical derivation; no model choice to
  document). Stopword list is fixed inside the script (function words +
  question words; numerals are kept — they are discriminative).
- W1a derivation: distinct content tokens of `query` minus the stopword
  list, order preserved, truncated at 10.
- W1b derivation: first 4 distinct content tokens of `situation` minus the
  stopword list.
- Script self-checks (print + abort-on-fail):
  1. n=200; every item's both keyword strings non-empty.
  2. On para-hard: kw_blind tokens have ZERO overlap with query content
     tokens (corpus-guaranteed by construction; if not, generation aborts).
  3. On para-hard: kw_transcript tokens have ZERO overlap with base text
     tokens (they genuinely add new vocabulary; if not, generation aborts).
  4. Counts of items where kw_transcript shares ≥1 query token with the
     query and where kw_blind shares ≥1 token with the query, per slice —
     printed in the pre-run block into the results entry (the honest
     disclosure of what the bridge does).

## Instrument

`cmhm_core/tests/ops_bench_w1_kw_v1.rs` — clone of `ops_bench_minilm_v2.rs`
(same WITHOUT brute-force legs, same WITH (engine) legs, same aggregation,
same stage attribution, same print shape), two `#[ignore]` tests (one per
leg), corpus loader `tools/eval_w1/corpus_l1_kw.json` vs `../tools/...`.
Harness invariants asserted before learning: kw fields non-empty for the
leg in use; kw_blind tokens ⊆ situation tokens.

In-leg learn call = the MiniLM record learn call verbatim except the
situation text carries the appended keywords. Provenance ref: hard
MiniLM-record numbers are hard seedP@1 2/30, hard pool 18/30, overall R@1
114/200, overall pool 164/200, lexical seedP@1 1.00, hash re-run R@1
101/200 identical shape (results.md "Session L recall + scale bench" +
"B10 Step 1" + post-B11 re-run sections).

## Gates (standing shipping gates — NOT re-targeted; per leg)

- A regression guard: lexical WITH-fused seedP@1 == 1.00 (100/100).
- B primary: hard WITH-fused seedP@1 ≥ 15/30 (0.50).
- C pool: overall WITH-fused poolRec ≥ 170/200 (0.85).
- D overall: overall WITH-fused R@1 ≥ 140/200 (0.70).
- E not-a-swap-worse: overall WITH-fused pool ≥ 164/200 (MiniLM record,
  pre-B11-verified twice) for BOTH legs — a keyword leg that improves hard
  while collapsing overall pool FAILS.
- Interpretation: PASS = A∧B∧C∧D∧E for the leg. Partial deltas are recorded
  regardless; hard-slice noise tolerance ±1 pair per 30 (±3.3 pts) applies
  for single-run cells (every cell here is a single run; flagged).
- Channel check (from the attribution table): an honest W1a pass is
  expected to reduce lost_candidate (bridge restores BM25 hits). A pass
  delivered by lost_rank or lost_safety would be a different mechanism and
  is explicitly reported as such, not as gate headline.

## Verdict plan

One run each leg, both after this pre-registration and the compile-green
instrument are committed. Verdict lines verbatim in results.md; appended
under a "W1" heading with the run command, fixture commit, and the
pre-run disclosure block. The lever is then handed back to the owner
before D2 (lever 2) fires.
