# B13 — ship-default save discipline + configurable fusion cap (engine-default experiment)

**Status: PRE-REGISTERED BEFORE ANY ENGINE EDIT AND BEFORE ANY RUN
(2026-10-25).** Owner GO given 2026-10-25 for the engine-fix
experiment with save-first sequencing. No discipline fixture, no
instrument, and no engine change exists at the time of this commit.

## 0. Question

The delivered stack stands at hard-tier retrieval **14/30** (46.7%, CP
95% [28.3%, 65.6%]), with engine-alone rank-1 at 2/30 on the same slice.
The committed evidence separates two things this registration takes on:

- **Save side.** B12-R (`tools/eval_b8/b12r-save-discipline.md`) showed
  that when the memory text quotes the user's own symptom words, the
  gold memory lands at **pool rank 0 in 30/30** hard items (forced save,
  store of 30; the pick stage added nothing that matters). But W1a
  (`tools/eval_b8/w1-write-time-keywords.md`) — the keyword-SUFFIX form
  of the same save-time idea — restored the pool and converted **ZERO**
  top-1 (hard seedP@1 stayed 2/30). Whether the discipline's text FORM
  converts at the RANK stage at 200-node scale, or whether the W1a
  failure is the true shape, is an open measured question.
- **Engine side.** The fused shortlist is truncated at a hardcoded
  fusion cap of 20 inside `retrieval::retrieve`
  (`rrf_fuse(..., 20)`). L3 measured gold present-but-invisible beyond
  that cap in 8 hard items (`lost_rerank_visibility = 8`). The value is
  not configurable, so the leak cannot be measured as shipped.

**Question:** does the save-side discipline, as a SHIPPED DEFAULT
(mechanical text rule + learn-path nudge), clear the standing 21/30
hard bar with the fusion cap knob in place — and does the cap restore
beyond-cap visibility without regressing overall pool or R@1?

## 1. Relationship to standing gates and policy

- The standing owner bar is retained **unretargeted**: hard WITH-fused
  retrieval@1 ≥ **21/30** (70%).
- Gates A/C/D/E are copied **verbatim** from the W1 standing
  pre-registration (`tools/eval_b8/w1-write-time-keywords.md`, "Gates"
  section). C and D thresholds are absolute.
- Per-lever policy: verdict to the owner BEFORE any further lever; no
  push and no release/README wording without the owner's ask (pushes in
  the publish arc are authorized but each is flagged); recorded numbers
  change only with the reviewer's sign-off.
- This is a NEW registration — not an edit to W1, B12-R, or recorded
  results; its verdict is appended to `tools/eval_b8/results.md` under
  a `## B13` heading.

## 2. What this measures — and what it does not

**Measures (engine-only; NO LLM anywhere in the instrument):**

- The effect on the shipped fused retrieval path of the mechanical
  save-discipline text rule, leg-controlled against the record.
- The new fusion-cap knob at 20 vs 40, on both text conditions.
- Standing gates A–E, a record-fidelity control gate (F), and the
  stage-attribution channel check.

**Does NOT measure:**

- Spontaneous agent compliance (the save is a mechanical template; B8's
  zero-learns finding stands). The production-side evidence that a real
  agent follows the rule is B12-R PART 2 (forced save, query-token
  recall 0.9889, 30/30 pool rank 0).
- Pick-stage behavior: carried by committed B12-R data (D-arm pick
  30/30); no pick session is opened.
- The shipped exe's hashed-embedding path: this instrument runs the
  MiniLM int8 + committed-projection lineage, the same lineage as the
  standing W1/L3/B10 records.
- Cross-model generality, easy/medium product claims, notes-file
  comparisons, or a 95% lower-bound claim. n=30 supports the
  point-estimate claim only; CP CI reported.

## 3. The text rule (the shipped default being tested)

One mechanical template applied to the state learn text, applied
verbatim:

    For '<query>', what was learned: <situation>

- `'<query>'` is the incident's verbatim user-symptom line and
  `<situation>` is the verbatim learned text — straight ASCII
  apostrophes, no escaping, no trailing punctuation added.
- Generator: `tools/eval_b13/make_discipline_texts.py`, committed
  BEFORE the fixture it produces so provenance is checkable. Its output
  `tools/eval_b13/corpus_b13_discipline.json` is committed
  byte-for-byte; no manual edits after generation.
- Script self-checks (print + abort-on-fail): n=200
  (100 lexical / 29 easy / 41 medium / 30 hard, exact counts); every
  item id unique; every field present; every template text non-empty;
  the fixture's `discipline_text` equals the template applied to the
  item's fields for ALL items; per-group count of items where query and
  situation share ≥1 content token (expected 0 for para-hard by corpus
  construction — abort if not; other groups: printed as disclosure, not
  gated).
- **Ceiling framing (carried from W1):** the template feeds the
  incident's symptom words to the save side BY CONSTRUCTION. It
  measures the CEILING of the discipline "quote the caller's own
  symptom phrasing in the memory text" on this corpus — not a
  production guarantee. The product-real version delivered with this
  registration is the learn-path nudge (§6b): the rule travels to the
  agent as guidance, and B12-R is the committed evidence that agents
  produce such text when instructed.

## 4. Legs and engine changes

Four legs, one instrument, each leg its own engine instance and
tempdir store:

| leg                   | state learn text        | state embedding        | fusion cap |
|-----------------------|-------------------------|------------------------|------------|
| `b13_control_cap20`   | situation, verbatim     | embed(situation)       | 20         |
| `b13_control_cap40`   | situation, verbatim     | embed(situation)       | 40         |
| `b13_discipline_cap20`| template text (§3)      | embed(template text)   | 20         |
| `b13_discipline_cap40`| template text (§3)      | embed(template text)   | 40         |

- Action/outcome learn text and embeddings are byte-identical to the
  MiniLM record harness, including the `{idx}` embed suffix
  (`emb.embed(&format!("{} {idx}", p.action))` and the same for
  outcome; raw text context unchanged).
- The WITHOUT brute-force context rows (native 384-d and projected
  128-d) are computed in every leg from the RAW situation/query
  embeddings, exactly as W1 computes them — record-fidelity context,
  not leg-conditioned.
- Retrieval per item: a dense-only retrieve (`""` query text) and a
  fused retrieve with the ORIGINAL query string only — keyword-like
  fixtures are indexed, never queried. W1 instrument call pattern
  verbatim.
- The state embedding for the discipline legs is the embedding of the
  full template text: this is what the real learn path would receive
  when the caller's text follows the discipline (B12-R learned the
  agent's full memory text as the State node; the discipline leg is the
  mechanical stand-in for that same shape).

**Engine changes delivered with this registration, committed BEFORE
any run (so the knob exists and no run-side live-editing is needed):**

(a) `Config::retrieval_fusion_cap` (default **20**) threaded to
    `retrieval::retrieve` as an explicit parameter, replacing the
    literal 20 in the fused-pool truncation. **No behavior change at
    the default.**
(b) `config.toml` documents the knob.
(c) The cmhm_mcp learn response gains the save-discipline nudge
    (§6b) and the server startup guidance gains the same one-liner —
    this is the shipped-default delivery surface in the product.

No retrieval arithmetic is touched besides the cap argument. The
cap-40 configuration is a measured value of the knob, not a hidden
behavior change.

## 5. Instrument

`cmhm_core/tests/ops_bench_b13_ship_default.rs` — clone of
`ops_bench_w1_kw_v1.rs` body (same aggregates, same WITH/WITHOUT rows,
same stage attribution `lost_candidate/lost_safety/lost_rank`, same
print shape); corpus loader reads
`tools/eval_b13/corpus_b13_discipline.json`; the `Pair` struct drops
the W1 keyword fields and gains the template text; per-leg invariants
asserted before learning: n=200, template assertion (§3) for all items,
leg naming restricted to the four registered legs.

Run command (repo root, standing family):

    cargo +stable-x86_64-pc-windows-msvc test -p cmhm_core --target
    x86_64-pc-windows-msvc --features onnx --test
    ops_bench_b13_ship_default -- --ignored --nocapture

Compile-green REQUIRED before any leg runs, including `cmhm_core` lib
tests and `cmhm_mcp` tool_tests (both must stay green with the engine
changes in the tree).

## 6. Gates (standing — NOT re-targeted)

### 6a. Instrument gates

- **A regression guard:** lexical WITH-fused seedP@1 == 1.00 (100/100)
  — required in BOTH discipline legs.
- **B primary (ship bar):** para-hard WITH-fused seedP@1 ≥ **21/30**
  in the discipline leg that ships. The cap-20 discipline leg carries
  the claim for the cap-20 default; the cap-40 discipline leg must
  ALSO satisfy A–E if the shipped cap becomes 40.
- **C pool:** overall WITH-fused poolRec ≥ **170/200** — discipline
  legs (standing W1 gate verbatim).
- **D overall:** overall WITH-fused R@1 ≥ **140/200** — discipline
  legs (standing W1 gate verbatim).
- **E not-a-swap-worse:** overall WITH-fused pool ≥ **164/200** — in
  EVERY leg (both control legs and both caps included). A leg that
  improves hard while collapsing overall pool FAILS.
- **F control fidelity:** `b13_control_cap20` must equal the MiniLM
  record exactly: hard seedP@1 **2/30**, overall pool **164/200**,
  overall R@1 **114/200**, lexical guard **1.00**. Any deviation makes
  the run VOID for verdict (instrument defect rule, not a number to
  report as a finding).
- **Channel check:** the attribution table must accompany every gate
  line. The pre-registered mechanism of an honest discipline pass is:
  lost_candidate collapses (the BM25 bridge restores candidates) AND
  lost_rank converts to seed hits (the D-arm shape). A pass whose
  delta is carried by lost_safety is labeled a different mechanism and
  does NOT count as the bridge/rank story. A discipline leg that fills
  pools without converting the top-1 (the W1a shape, hard seedP@1
  ≈ 2/30 with pool restored) fails B outright — reported as the
  finding, never as a headline.
- **Hard-slice single-run tolerance:** ±1 item per 30 applies to
  comparisons against prior single-run cells. Gate thresholds
  B/C/D/E are absolute minimums; the tolerance is a comparison aid,
  never a way to pass under the threshold.

### 6b. Shipped-default wording (delivered verbatim with the engine change)

Learn response nudge (appended to `memory_learn_text` success
responses) and the same sentence in the server guidance:

    Save discipline: include the user's own words for the symptom in
    the state text, alongside what was learned — paraphrased-only
    symptom wording often becomes unfindable by keyword+fusion
    retrieval; verbatim symptom phrasing is what future queries match.

### 6c. Cap-40 flip rule (pre-registered)

The shipped `retrieval_fusion_cap` default moves 20 → 40 ONLY IF
`b13_control_cap40` (same record-verbatim texts, variable isolated)
shows pool40 ≥ pool20, R@1 40 ≥ R@1 20 − 1, hard seed 40 ≥ hard seed
20 − 1, and lost_safety does not increase. Otherwise the shipped
default stays 20 and the finding is reported as a knob, not a default
change.

## 7. Interpretation / verdict rule

PASS = A ∧ B ∧ C ∧ D ∧ E on the shipped configuration, with F intact
and the channel check consistent. The supported claim is exactly:
"With the save-side discipline applied at learn time (mechanical
template rule, no LLM at retrieval or at the engine), hard WITH-fused
retrieval@1 reached ≥ 21/30 while overall pool and R@1 did not regress
below the standing gates (single run per leg, one embedder lineage,
hard slice of 30, n=200 store)."

FAIL on B with restored pools = the W1a-shape headline: the discipline
converts candidates at 200-node scale but not the top-1. That result
is reported verbatim and handed back to the owner — no gate
redefinition, no lever push without the owner's next GO.

Any partial result is handed to the owner BEFORE the next lever.

## 8. Abort / deviation rules

- A compile or fixture defect discovered before any leg runs → fix,
  disclose, re-commit; no data collected before the fix is used.
- F violation → run VOID for verdict; instrument diagnosis published
  before any re-run.
- Fixture mismatch (item order, count, missing fields, embedding
  dimension) → abort before any reported number.
- Single run per leg; no re-rolls; no mid-run engine edits; no
  threshold reinterpretation.
- No LLM session is opened; no MCP server, no contexts, no
  `registry.toml` interaction — benches build tempdir stores. B11
  registry risk does not apply.

## 9. Artifacts and reporting

Committed BEFORE the run: this doc, the generator script, the
generated fixture, the instrument, and the engine changes (cap knob +
nudge). Committed AFTER the run, BEFORE the verdict: raw leg outputs
in `tools/eval_b8/results.md` under `## B13 — ship-default save
discipline + fusion cap`, with the run command, fixture commit,
gate lines verbatim, attribution tables, the cap-40 conditional
outcome, and the labels below.

**Labels for any citation:** hard slice only; single run per leg;
MiniLM int8 + committed projection lineage; mechanical transcription
CEILING (template by construction, not agent behavior); store of 200;
no LLM in the loop; no notes-file comparison; no cross-model claim;
point estimate with the standing CP discipline; fusion cap measured at
20/40.
