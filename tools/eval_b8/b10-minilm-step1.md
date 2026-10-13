# Step 1 design note: MiniLM recall leg (M-series continuation, B10)

**Status:** owner-approved start (2026-10-14, "go do whatever, step by step").
One step at a time; each gate stops the track until reviewed.
**Context:** Session L bench (hash) on current master missed all three recall
gates (Recall@1 0.505 vs ≥0.70; hard 0.000 vs ≥0.50; pool 0.560 vs ≥0.85)
while latency gates passed with 55–185x headroom. Failure mode isolated to
the hash embedder (WITHOUT-dense equally dead; B9 §2 showed surfacing is
fine). MiniLM/L1 record in task.md: hard seed 0.0667, hard pool 0.60.
**In scope for Step 1:** build + bench only. No serving default change, no
live-data touch, no reranker, no other engine edits.

## Hypothesis

H1: the MiniLM (onnx) path, unchanged, reproduces its recorded recall
premium over hash on the L1 corpus on current master.

H0: the record does not reproduce → the embedder path itself is suspect;
stop the track and investigate the path, not the engine.

## Pre-registered gates (set before any run)

Reproduce gate (from task.md record, achieved numbers, not aspirations):
- para-hard pool recall ≥ 0.55 (recorded 0.6000; allow ~1-pair window).
- para-hard seed P@1: at least the recorded count of hard rank-1 hits
  (2/30 = 0.0667); fewer = regression, stop.
Improvement gate (the point of the step):
- WITH-fused overall seed P@1 (200 queries) strictly above today's hash
  0.505 by more than noise (7 pairs → ≥ 108/200 = 0.540). If lex BM25
  carries as expected, expect ≈0.71 — but the gate is only "> hash by ≥
  8-ish pairs", stated as ≥0.540.

Invariants (must hold or stop):
- Hash bench reruns bit-identical from this tree's state (separate build,
  hash feature — re-ran as control).
- No code changes in cmhm_core/cmhm_mcp at all in Step 1. Bench builds only;
  known env: GNU override is directory-local; use
  `cargo +stable-x86_64-pc-windows-msvc --target x86_64-pc-windows-msvc
  --features onnx` per ops_bench_minilm_v2 header. Network needed once for
  the 14MB model download if not cached.

## Method

1. Control: re-run `ops_bench_v2` (hash, release) — expect bit-identical to
  the 2026-10-14 numbers recorded in results.md (provenance note: seed/pool
  deterministic; chain entropy-seeded — exclude chain from bit-identical
  claims).
2. Bench: `ops_bench_minilm_v2` (MSVC + onnx, release). Report the same
  table shape (seed P@1 / pool / chain / MRR × 4 groups, WITH-dense and
  WITH-fused).
3. Gate check against §2, report table + verdict, STOP. Step 2 (B8 re-run
  on MiniLM serving) and Step 3 (ranking) are separate gates and not yet
  authorised.

## Known coupling to flag now (not blocking Step 1)

Switching the *serving* embedder changes every future node's model_id;
existing hash-128d nodes are never compared against MiniLM queries (N3 by
design — silent zero-similarity is prevented by explicit model-id matching).
So a serving switch implies live-node invisibility until re-embed. That is a
Step-2 decision, not Step 1.
