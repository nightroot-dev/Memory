# D2 pre-registration addendum — protocol mechanics (lever 2/3, owner GO 2026-10-21)

The registered D2 protocol (results.md reviewer-deltas section, commit
b3151db) is honored verbatim. This addendum only fixes the mechanics the
registration left open, BEFORE any run (pre-registration rule).

## Registered terms (verbatim from results.md, unchanged)

- "An uninvolved LLM proposes 3 likely causes per hard query (agent-side
  symptom→cause rewriting); each expansion retrieves separately; pools are
  merged for pool metrics and the expanded-query set runs the SAME fused
  path (BM25 regains lexical anchor on expanded queries)."
- Gates: "hard WITH-fused seedP@1 >= 21/30 (70%, direct owner bar) = the
  ONLY pass; anything below is a failure of D2, not a reason to relax the
  target. If hard poolRec with merged pools < 21/30, expansion fails
  without further rank work."
- Scope: bench-only, no engine build. Hard queries only (30 items).

## Condition change (documented, not a deviation)

The registration says D2 "runs ONLY after B12 and only if the recall track
is still alive". B12 is recorded NO-GO (parity-guard fired, model
unavailable). The owner's 2026-10-21 lever directive ("one by one: 1) write
keywords, 2) query expansion, 3) bigger embedder/LLM rerank") supersedes
the sequencing condition; B12 remains NO-GO.

## Mechanics registered here (before the run)

- **Expansion source (uninvolved LLM, documented)**: one fresh subagent
  session (no repo access; instructed to work only from the query strings
  and general IT knowledge). Model: `a stand-in client LLM (name withheld)`
  (provider catalog 2026-10-21). Raw output committed verbatim as
  `tools/eval_d2/subagent_output.json`; an in-run generation attempt by the
  builder-as-LLM was REJECTED (builder has corpus visibility from W1 work
  — the uninvolved-LLM ring-fence requires a session without it).
- **Rule honored**: EXACTLY 3 causes per hard query; each ≤ 12 words;
  technical SRE/wiki style.
- **Merged metrics semantics (registered)**: for each hard item the fused
  path runs 4 retrievals — the original query text plus the 3 cause
  texts (each the SAME `retrieve()` fused call as the record). Each run
  is SELF-CONTAINED: its dense embedding and its BM25 text are the same
  string (original run pairs the original query's embedding with the
  original query text; each cause run embeds its own cause text for both
  the dense and the fused-BM25 part — an agent's rewrite is a complete
  new query, not a re-skinned original). Post-run merge per item:
  - merged poolRec = gold state node present in ANY run's
    `post_filter_candidates` (union of slots);
  - merged seedP@1 (the gate) = gold hit as SEED in ANY of the 4 fused
    runs (best-of-runs; agent-realistic expand-and-combine);
  - merged MRR = best 1/(rank+1) across runs;
  - merged stage attribution: candidate loss if gold ∉ union of
    `raw_nn_candidates` across runs; safety loss if in raw-union but ∉
    post-filter-union; rank loss otherwise (never a seed).
- **Control leg registered**: the SAME instrument also runs the plain
  record baseline (no expansion, all groups fused as the record) — a
  drift control for the harness clone; expected to match the MiniLM
  record numbers bit-stable (as W1b did).
- **Corpus**: original `tools/corpus_l1.json` (NO keyword fields — query
  side only). Fixture `tools/eval_d2/corpus_l1_d2.json` couples each hard
  item with its 3 causes; generated mechanically by
  `tools/eval_d2/make_expansions.py` from the two source files; committed
  BEFORE the run; no manual edits afterward.
- **Instrument**: `cmhm_core/tests/ops_bench_d2_expansion.rs` (clone of
  the W1/record harness body; learn calls byte-identical to the record's
  learn calls — no keyword insertion). Two `#[ignore]` tests (control,
  expanded). Retrieval path untouched; no engine changes.
- **Disclosure printed pre-verdict**: per hard item, cause-text token
  overlap with (a) the item's store text and (b) the query — counts only,
  no gate attached.

## W1 relationship (informational, no gate change)

W1 (lever 1) measured that a restored pool does not convert under the
final ranker (hard seed unchanged at 2/30 with pool 30/30). D2's merged
seed metric measures best-of-runs: a cause statement that lands the gold
item at seed#1 in its own fused run counts as a hit. This is the one new
conversion channel D2 adds on top of pooling (seed re-decision, not
just pool growth).

## Verdict rule (from registration)

- PASS iff merged hard WITH-fused seedP@1 >= 21/30.
- If merged hard poolRec < 21/30 → expansion fails, no rank work may follow.
- Anything else recorded as measured deltas; hand-back to owner BEFORE
  lever 3.
