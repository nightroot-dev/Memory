# L3b REPLICATION PRE-REGISTRATION — same-family variance sample (2026-10-25)

Status: registered BEFORE any run, per owner request 2026-10-25
("test again, you are the client model on me"). Variance replication of
**L3b only** (the hard-slice pick over the shipped MiniLM record pools).

## Purpose and scope

- Second, independent sample of the L3b mechanism: an uninvolved picker
  choosing, per hard item, the candidate that best explains the symptom, from
  the SHIPPED pools (30 hard items; recorded L3b = 14/30 on 2026-10-21).
- Does **NOT** replace, revise, or re-target L3b or its published number
  (14/30 = 46.7%, Clopper-Pearson 95% CI [28.3%, 65.6%]). Both samples are
  reported side by side; no number in README/records changes because of this
  run.
- No new gates. No pass/fail target. Measures run-to-run spread of the same
  mechanism in the same model family.
- NOT cross-model generality (same family as L3); NOT a recall-metric change;
  no engine changes.

## Fixtures (committed, unchanged)

- Prompts: `tools/eval_l3/batches/l3b/b01.txt`, `b02.txt`, `b03.txt`
  (committed at `eae433d`; gold-stripped) — reproduced verbatim, in that
  order, one batch per message.
- Scoring pools: `tools/eval_l3/pools_record_hard.json` (30 hard items, each
  with `gold_node_id` and `presented[]` ranks).
- Pick mapping (identical to the L3 scorer): for item `i`, pick `N` counts
  correct iff `presented[N-1].node_id == gold_node_id`.
- Original comparator artifacts: `rerank_record_hard.raw` / `.json` (L3b).

## Picker

- Model: `a stand-in client LLM (name withheld)` (same model id as L3; verified live in
  the catalog on 2026-10-25, status active, before the run).
- One fresh uninvolved session (no repo access, no golds); three messages,
  one per batch. Session id recorded in PART 2 after the run.
- Owner asked for "you as the client model": the assistant itself is
  contaminated (has read the records and corpus), so a fresh session of the
  same model family is the honest analogue. This substitution is part of the
  registration.

## Rules

- Single run. No re-rolls, no prompt edits, no retry-on-bad-luck.
- Malformed, missing, or duplicate picks for an item count as misses; no
  manual repair, no re-ask.
- Outputs saved verbatim (`tools/eval_l3/rerank_record_hard_repl.raw`, same
  `b01.txt: {json}` format) + parsed json; score computed mechanically by the
  committed script `tools/eval_l3/score_repl.py`; CP 95% CI reported.
- If the model is unavailable or refuses, the run is ABORTED and recorded as
  an abort (B12 lesson) — not re-tried on another model without a new
  registration.

## Reporting (PART 2, appended after the run)

- Picks per batch (expected 10/10/10), correct/30, CP 95% CI, side-by-side
  with the recorded 14/30, deviations (none expected), and the honest scope
  labels (hard slice only; single run; same family; not a replacement).

---

# PART 2 — RESULTS (2026-10-25)

## Run facts

- Picker: `a stand-in client LLM (name withheld)` (same model id as L3b), one fresh
  uninvolved session `ses_e8d333d36ffeofVpIxdd1gLZfG`.
- Fixtures reproduced verbatim, one batch per message, in order
  `b01.txt` → `b02.txt` → `b03.txt`; no prompt edits, no re-rolls, no
  retries, no second session.
- Answers: 30/30 items, 10 per batch; 0 missing, 0 duplicate, 0
  out-of-range picks.
- Raw outputs: `tools/eval_l3/rerank_record_hard_repl.raw`; parsed:
  `tools/eval_l3/rerank_record_hard_repl.json`.
- Scorer: `tools/eval_l3/score_repl.py` (pick rule unchanged:
  `presented[N-1].node_id == gold_node_id` for pick `N`).

## Headline (hard slice only, single run, same family)

| Sample | Correct | Rate | CP 95% CI |
|---|---|---|---|
| L3b recorded 2026-10-21 | **14/30** | 46.7% | [28.3%, 65.6%] |
| L3b replication 2026-10-25 | **13/30** | 43.3% | [25.5%, 62.6%] |

- The replication is a **second sample, not a replacement**: the published
  L3b number remains 14/30 [28.3%, 65.6%]. No README or recorded number
  changes because of this run.
- No gate, no pass/fail target, no re-target: this run measures same-family
  run-to-run spread only.

## On-presented context (descriptive, secondary)

Gold was present in the pool for 18 of the 30 hard items (the other 12 are
the measured cap-20 visibility loss, unchanged by this run).

| Sample | Correct on presented | Rate | CP 95% CI |
|---|---|---|---|
| L3b recorded | 14/18 | 77.8% | [52.4%, 93.6%] |
| Replication | 13/18 | 72.2% | [46.5%, 90.3%] |

## Item-level agreement with the recorded sample

- Identical pick on **23/30** items.
- Seven disagreements (id, recorded pick, replication pick): `para-049`
  (1, 8), `para-057` (6, 9), `para-061` (3, 17), `para-081` (1, 19),
  `para-084` (3, 10), `para-085` (3, 19), `para-086` (18, 19).
- The replication's 13 correct items are a **strict subset** of the recorded
  14: it gained no item the first sample missed and lost only `para-057`
  (gold at presented rank 5; replication picked 9). Difference between the
  samples is therefore one pick on an item the first sample already had in
  view.

## Deviations

- **Run deviations: none.** Three batches verbatim, one session, single run,
  no re-rolls, no repairs.
- **Instrument deviation (disclosed):** the pre-registered scorer's
  Clopper-Pearson helper had both bisection comparisons inverted and first
  printed `[59.4%, 100.0%]` for 13/30 — an interval that excludes its own
  point estimate, which is how the defect was caught. The helper was
  corrected post-verdict (record adjacent to this file; commit touching
  `score_repl.py`). The pick rule, the picks, and the count (13/30) were
  never affected. Validation of the corrected helper: (a) matches
  `scipy.stats.beta` exact intervals (scipy 1.18.0), and (b) reproduces the
  published L3b interval `[28.3%, 65.6%]` from 14/30.

## Honest reading

- Two same-family samples now bracket the blind-hard pick rate in the
  **mid-40s** (14/30, 13/30; overlapping CIs). This is consistency evidence
  for the one family measured — it is **not** cross-model generality.
- Both samples sit far above engine-alone (2/30) and below the 70% claim
  bar. Nothing here changes the published claim or the README labels.
- The save-discipline ceiling result (L3d, 30/30 with the bridge + pick
  stack under held save discipline) is untouched by this run; the blind
  shipped-pool sample remains the honest product number.

Labels for any citation of this run: **hard slice only; single run; same
model family (`a stand-in client LLM (name withheld)`); replication sample — not a
replacement for the recorded 14/30; no engine change; no gate applied.**
