# Lever 3/3 pre-registration — bigger embedder / LLM rerank (owner GO 2026-10-21)

Registered BEFORE any run, any pool dump, or any rerank fixture. Nothing in
this file may be changed after the first commit; deviations must be
documented in the results record, never silently.

## Hand-back context (stamped)

- W1 (lever 1): FAIL on standing gates — pool lifted to 200/200, rank
  conversion 0 (`90b54e6`).
- D2 (lever 2): FAIL on its registered gate — hard merged seedP@1 15/30 <
  21/30; pool 26/30; 11 rank-stage misses remain (`42890e5`).
- Owner directive 2026-10-21: lever 3 = "bigger embedder / LLM rerank",
  verdict back to owner before any further work.

## Sections

1. [L3a — embedder swap (bge-base-en-v1.5)](#l3a)
2. [L3b — LLM rerank over record fused pools (hard-only diagnostic)](#l3b)
3. [L3c — LLM rerank over D2-expanded pools (gated)](#l3c)
4. [L3d — LLM rerank over W1a bridge pools (gated, the combination test)](#l3d)
5. [Registered gates](#gates)
6. [Rerank mechanism + ring-fence](#mechanism)
7. [Known risks / honesty notes](#risks)

<a name="l3a"></a>
## L3a — embedder swap: bge-base-en-v1.5

The "bigger embedder" half, benchable with ZERO engine changes: the engine
already ships `OnnxEmbedder::bge()` (model id `bge-base-en-v1.5`, 768-d) and
`data/projections/bge-base-en-v1.5.bin` exists on disk (393,732 B = 768×128
f32 PCA + header).

- Instrument: `ops_bench_l3_rerank` leg `l3a` — the record harness body
  (corpus_l1.json, byte-identical learn treatment incl. the "{idx}" suffix)
  with the embedder/model/projection swapped to bge.
- Judged on ITS OWN numbers against the standing shipping gates (see
  [Gates](#gates)). MiniLM record rows may appear as informational anchors
  only — B10's void rule respected: no cross-model number mixing into any
  gate.
- Documented limits (pre-registered): (a) no query prefix — the engine's
  embed path is prefix-free, so this measures the engine-as-shipped bge
  integration, not bge's researched ceiling; (b) the bge PCA is an untracked
  runtime artifact in a gitignored `/data/` tree — the same class as the
  MiniLM PCA every record run loads, but its TRAINING provenance is not in
  the repo (B10-era runtime step). Flagged as the first anomaly suspect if
  bge numbers look degenerate; (c) first call downloads the ~110 MB HF
  onnx model (single, then cached).

<a name="l3b"></a>
## L3b — LLM rerank, record fused pools, hard-only (DIAGNOSTIC — no gates)

Isolation leg: does an LLM-class rerank convert the record's own hard
rank-stage misses (16 of 28; gold in pool 18/30)? Ceiling 18/30. Gate C
fails by construction on the record pool (164 < 170) — so NO standing gates
are attached to this leg; it reports hard rerank accuracy / seed / the
attribution decomposition, to be read next to L3c/L3d.

<a name="l3c"></a>
## L3c — LLM rerank over D2-expanded pools (GATED)

- easy + medium: rerank over their RECORD fused pools (pools inherited
  unchanged: 18/29 pooled easy, 28/41 medium).
- hard: rerank over the D2 4-run MERGED pool (union of orig + 3 cause
  retrieves, first-seen order, ORIG-RUN rank order first), capped at 20
  (engine's own fuse cap; the union can exceed it — golds beyond the cap
  are `lost_rerank_visibility` and disclosed per item).
- Pool math (informational): C ceiling = 100 (lex) + 18 + 28 + 26 = 172;
  D ceiling = 172; B ceiling = 26/30. This is the only rerank leg whose
  pool math CAN clear C.

<a name="l3d"></a>
## L3d — LLM rerank over W1a bridge pools (GATED — the combination test)

- All 100 paraphrase items reranked over their W1a pools (pool 100/100 by
  W1a measurement); lexical group BYPASSED at 100/100 (exact-lookup
  preserved by design — no LLM in the exact-match path; production-sane).
- Candidate texts = the stored W1a situation strings INCLUDING the
  " keywords: ..." suffix (that is the stored text; the rerank sees exactly
  what production would store).
- W1a golds may sit deep (W1a hard MRR 0.1909): engine cap 20 applies;
  golds beyond cap = `lost_rerank_visibility`, disclosed per item.
- Pool math: C = 200, D ceiling = 200, B ceiling = 30/30.
- This leg answers the combination question the whole series has pointed
  at: (W1 write-time bridge + a working rank-side mechanism) — the only
  in-class pairing with one half proven before this lever.

<a name="gates"></a>
## Registered gates (standing; never re-target)

For the gated legs (L3a own-frame, L3c, L3d):

- **A** lexical seedP@1 == 1.00 (bypass preserves it by construction in
  L3c/L3d; L3a measures it in the bge frame).
- **B** hard WITH frame seedP@1 (final seed decision) >= 15/30.
- **C** pool >= 170/200.
- **D** overall R@1 (final seed decisions incl. bypassed lexical) >= 140/200.
- **E** not-a-swap-worse: pool >= 164/200.

Pass = all five. Anything below is a lever-3 failure, not a reason to
relax the target.

<a name="mechanism"></a>
## Rerank mechanism + ring-fence (registered)

- **Uninvolved LLM, documented**: `a stand-in client LLM (name withheld)` via fresh
  subagent sessions (one session per leg: L3b, L3c, L3d). No repo access;
  input = committed prompt files ONLY (copies placed in the approved temp
  area; byte-identical to the committed `tools/eval_l3/batches/<leg>/*.txt`).
- **Batching**: 10 items per prompt file; the subagent processes files in
  order and emits one STRICT JSON object per file mapping item-id ->
  chosen candidate number (1-based). Batch-of-10 is a bench-economy choice;
  any batch-attention degradation is part of the measured mechanism and is
  reported (pick-distribution sanity check printed).
- **Prompt content per item**: item id, USER SYMPTOM (the query text),
  numbered candidate situation texts (≤ 20, engine/union order), no gold
  labels, no action/outcome text, no other corpus fields.
- **Rules**: exactly one pick per item, from the presented numbers only;
  missing / out-of-range / invalid JSON answer = miss (`lost_rank_rerank`),
  never re-asked.
- **Fixture provenance**: dump JSONs (`tools/eval_l3/pools_*.json`) and
  rerank fixtures (`tools/eval_l3/rerank_*.json`) committed verbatim BEFORE
  the scored runs; make_batches.py is mechanical (dump minus gold fields ->
  prompt files), deterministic, committed.
- **Scoring (registered)**: seed = the LLM pick. seedP@1 = pick == gold.
  pool/poolRec inherited from the underlying pool leg (uncapped fused pool
  for gates C/E). Attribution buckets: `lost_candidate` (gold ∉ uncapped
  pool), `lost_rerank_visibility` (gold in pool, ∉ presented cap-20),
  `lost_rank_rerank` (presented, wrong pick), `lost_safety` (0 by
  construction — the pool already passed the engine's filters).
  chainAcc is DROPPED from rerank legs (seed-choice measurement; the
  underlying pools' chain metrics are unchanged and already recorded).

<a name="risks"></a>
## Known risks / honesty notes (pre-registered)

1. **Rerank is agent-side, query-time cost**: one LLM call per
   miss-prone retrieval (batched here for bench economy). The bench
   measures the recall mechanism only; shipping/cost decisions stay with
   the owner — same framing as D2's agent-side expansion.
2. **B13 relationship**: cheap cross-encoder rerank already failed (jina
   net 0/30, bge-CE net 1/30 vs +10 bar). This lever tests the LLM-class
   pick-over-full-texts rerank, a different mechanism class.
3. **Batch-attention risk** (batch=10): if picks collapse onto few
   candidate slots, accuracy-on-presented will show it; the pick
   distribution is printed. No re-rolls, no retries.
4. **L3a PCA provenance** (above) — first anomaly suspect for bge.
5. **Scope honesty**: L3b has no gates by construction (pool math); L3c is
   the only gated leg whose pool math can clear C; L3d has the maximal
   ceiling. A pass on L3d alone implies a two-lever stack (W1 + rerank),
   not a single-mechanism ship; the record will state which stack passed
   if any does.

## Run order (registered)

commit A (this file + instruments, compile-green) → L3a run → pool dumps
(L3b/c/d) → prompt files committed with dumps + rerank fixtures → scored
runs → verdict records → hand-back to owner (before anything else).
