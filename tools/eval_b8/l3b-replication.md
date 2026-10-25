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
