# B8 results (written by the runner after arm executions â€” not hand-edited)

Win condition: CMHM arm uses fewer tool calls AND less wall time than BOTH
baselines on a majority of the 7 tasks. Ties favor the baselines.

| task | none: calls | none: time | none: pass | notes: calls | notes: time | notes: pass | cmhm: calls | cmhm: time | cmhm: pass | winner |
|---|---|---|---|---|---|---|---|---|---|---|
| t4-hunt-wal-replay (pilot) | 23 | 222.9s | PASS | 15 | 99.0s | PASS | 21 (1 retr) | 170.3s | PASS | notes |
| t1-mods-tab-missing | 16 | 52.1s | PASS | 12 | 33.2s | PASS | 19 (2 retr) | 158.5s | PASS | notes |
| t2-ethernet-500mbps | 5 | 70.0s | FAILÂ¹ | 5 | 112.4s | PASS | 11 (2 retr) | 152.3s | FAILÂ¹ | notes |
| t3-openbot-shortcut | 16 | 34.2s | FAILÂ² | 15 | 49.7s | PASS | 21 (1 retr) | 65.3s | PASS | notes |
| t5-hunt-bm25-scoring | 23 | 284.1s | PASS | 18 | 129.1s | PASS | 29 (1 retr) | 202.4s | PASS | notes |
| t6-hunt-trust-boundary | 31 | 156.2s | PASS | 30 | 219.3s | PASS | 32 (1 retr) | 99.6s | PASS | noneÂ³ |
| t7-hunt-api-surface | 39 | 264.2s | PASS | 43 | 174.8s | PASS | 34 (1 retr) | 108.6s | PASS | cmhm |

Â¹ T2 none/cmhm missed only rubric group 4 (the minor iperf-path pitfall);
both gave strong main diagnoses. Rubric stands as pre-registered.
Â² T3 none's shortcut target has a path typo (`tasks/3-` vs `tasks/t3-`) and
does not resolve; report itself was strong.
Â³ T6: no outright winner â€” notes fewest calls (30 vs 31/32), cmhm fastest
wall (99.6s vs 156/219s). Neither satisfies the AND condition.

Pilot notes (2026-10-13, all arms bench-agent-m xhigh, model parity holds):
- All three arms produced correct, verifier-passing reports. The harness
  discriminates on efficiency, not capability â€” as designed.
- notes beats cmhm beats none on both metrics. Per the kill criterion this
  task is a loss for CMHM (0/1): memory helped vs nothing (23â†’21 calls,
  223â†’170s) but lost to the ideal-facts file.
- Compliance verified from opencode.db: none/notes used zero memory tools;
  cmhm used exactly 1 retrieve (nested in execute) + 0 learns, as instructed.
- Metrics method: tool-call rows + nested execute subcalls counted from
  session_message payloads; wall time from message timestamps. Method pinned
  here so the full run uses the identical procedure.
- Runs' full reports live under tools/eval_b8/runs/ (working files,
  git-ignored, not committed).

Verdict: CMHM 1/7 â€” LOSES per the kill criterion (majority required).
Notes wins 5/7 outright; T6 has no winner. Memory helped vs nothing on most
tasks but lost to the ideal-facts file almost everywhere.

Caveats (read before citing):
- Single runs per cell, no warm-up; wall time includes model latency
  variance. Call counts are the stabler metric.
- Version skew UNDERESTIMATES cmhm: arms ran against the live server
  (pre-B1 stub retrieval â€” agents quote "no actionable text" and work
  around it), not repo code. A loss here is conservative-strong evidence;
  a re-run on current code could only help the cmhm arm.
- Compliance verified from opencode.db for every arm: none/notes zero
  memory calls; cmhm arms 1â€“2 retrieves + 0 learns as instructed.
- All arms bench-agent-m xhigh (model parity). Metrics method: tool-call
  rows + nested execute subcalls from session_message payloads.
- Fixture worktrees verified clean after all hunt runs.

# B8 re-run on current build (cmhm arms only, 2026-10-14 â€” not hand-edited)
#
# Context: live server upgraded to current master (post-B1 â€¦ S-25, v6
# snapshots); re-run seeds byte-identical (pinned cmhm-seeds.md) in fresh
# b8r-* contexts. none/notes cells are the ORIGINAL numbers (no server
# contact, still valid). Kill criterion unchanged: fewer calls AND less
# wall than BOTH baselines, majority of 7, ties favor baselines.
#
# | task | none: calls/time/pass | notes: calls/time/pass | cmhm re-run: calls/time/pass | winner |
# |---|---|---|---|---|
# | t1-mods-tab-missing | 16 / 52.1s / PASS | 12 / 33.2s / PASS | 14 (1 retr) / 37.7s / PASS | notes |
# | t2-ethernet-500mbps | 5 / 70.0s / FAILÂ¹ | 5 / 112.4s / PASS | 10 (1 retr) / 94.2s / PASS | notesÂ² |
# | t3-openbot-shortcut | 16 / 34.2s / FAIL | 15 / 49.7s / PASS | 24 (1 retr) / 73.0s / PASS | notes |
# | t4-hunt-wal-replay | 23 / 222.9s / PASS | 15 / 99.0s / PASS | 19 (1 retr) / 121.0s / PASS | notes |
# | t5-hunt-bm25-scoring | 23 / 284.1s / PASS | 18 / 129.1s / PASS | 24 (1 retr) / 154.1s / PASS | notes |
# | t6-hunt-trust-boundary | 31 / 156.2s / PASS | 30 / 219.3s / PASS | 37 (1 retr) / 176.2s / PASS | noneÂ³ |
# | t7-hunt-api-surface | 39 / 264.2s / PASS | 43 / 174.8s / PASS | 33 (1 retr) / 157.4s / PASS | cmhm |
#
# Â¹ Original rubric stands as pre-registered (see full-run table).
# Â² T2: cmhm correctness fixed (FAILâ†’PASS â€” the iperf pitfall is recalled
#   now) but trails notes 10 vs 5 calls; beats notes on wall only. Winner
#   stays notes.
# Â³ T6: same split as the original run â€” notes fewest calls (30), none
#   fastest wall (156s), cmhm neither. No outright winner; not-cmhm.
#
# Re-run verdict: CMHM 1/7 â€” LOSES per the kill criterion, same as the
# original run. The version-skew caveat is now retired: current code returns
# full stored text (verified: b8r-t1 retrieve renders the whole triple).
# Effects of the upgrade, measured: T2 FAILâ†’PASS (the iperf pitfall is now
# recalled), T1 19â†’14 calls / 159sâ†’38s, T4/T5 faster than their original
# cmhm cells (170sâ†’121s, 202sâ†’154s); T6/T7 slower on wall than their
# original cells (100sâ†’176s, 109sâ†’157s â€” run variance) with T7 still an
# outright win. Notes still wins 5/7 outright and the tally doesn't move.
# Compliance re-verified from opencode.db: all 7 re-run arms 1 retrieve +
# 0 learns; model parity held (bench-agent-m xhigh); worktrees clean.

# Session L recall + scale bench (current master, 2026-10-14 â€” not hand-edited)
#
# Commands (repo root C:\Games\Projects\Memory, release):
#   cargo +stable-x86_64-pc-windows-gnu test --release -p cmhm_core --test ops_bench_v2 -- --ignored --nocapture
#   CMHM_LOAD_N=10000 [same] --test loadbench -- --ignored --nocapture
# Corpus: tools/corpus_l1.json, n=200 (100 lexical-easy, 29 para-easy,
# 41 para-medium, 30 para-hard). Model: hash-128d (Phase 1 path).
#
# Recall (WITH-fused = shipping path; WITHOUT = flat baselines):
# | group | WITHOUT-dense seedP@1/poolRec | WITHOUT-bm25 | WITH-dense | WITH-fused seedP@1/poolRec/chainAcc/MRR |
# | lexical (100) | 0.17 / 0.71 | 1.00 / 1.00 | 0.18 / 0.97 | 1.00 / 1.00 / 0.14 / 1.00 |
# | para-easy (29) | 0.03 / 0.07 | 0.00 / 0.00 | 0.03 / 0.17 | 0.03 / 0.17 / 0.03 / 0.05 |
# | para-medium (41) | 0.00 / 0.02 | 0.00 / 0.00 | 0.00 / 0.15 | 0.00 / 0.10 / 0.00 / 0.01 |
# | para-hard (30) | 0.00 / 0.03 | 0.00 / 0.00 | 0.00 / 0.10 | 0.00 / 0.10 / 0.00 / 0.01 |
# Aggregates WITH-fused: Recall@1 = 101/200 = 0.505 (gate â‰¥0.70 MISS);
# hard Recall@1 = 0/30 = 0.000 (gate â‰¥0.50 MISS); pool recall = 112/200 =
# 0.560 (R@20 slot â‰¥0.85 MISS â€” harness poolRec is post-filter depth, not
# exactly @20, but misses under any reading); chainAcc 0.075, MRR 0.511.
# Failure mode: lexical perfect via BM25 fusion; paraphrase seed P@1 = 1 hit
# in 100 queries. WITHOUT-dense equally dead â†’ embedder bottleneck, not
# engine/ranking/surfacing. Evidence-backed lever (only): MiniLM/BGE path.
#
# Scale (10k learns, hash-128d): 190 learns/s; 7941 nodes (0.8/learn);
# retrieve mean 1.84ms / p50 1.80ms (gate â‰¤100ms PASS, 55x headroom) /
# p99 2.70ms (gate â‰¤500ms PASS, 185x headroom), k=200, pool non-empty
# 200/200; disk 5138 B/node; hash embed 3.33M texts/s.
# Net: latency gates PASS big; all three recall gates MISS. CMHM is fast
# enough by 2 orders of magnitude and not knowledgeable enough by ~20-50
# points â€” the binding constraint is the hash embedder on paraphrase.
#
# B10 Step 1 (2026-10-14, gates pre-registered in b10-minilm-step1.md):
# Control rerun bit-identical to the table above (chain variance excluded,
# expected). MiniLM leg (MSVC+onnx, ops_bench_minilm_v2) WITH-fused:
#   lex 1.0000/1.0000, easy 0.2069/0.6207, med 0.1463/0.6829, hard 0.0667/0.6000
# (seedP@1/poolRec) â€” all four cells bit-identical to the task.md record.
# Step-1 gates: reproduce PASS, improve PASS (overall seedP@1 114/200 =
# 0.570 vs hash 0.505, gate â‰¥0.540). Shipping gates on MiniLM: R@1 0.570
# MISS (â‰¥0.70), hard R@1 0.067 MISS (â‰¥0.50), pool 0.820 MISS (â‰¥0.85, 6
# pairs short). Attribution unchanged (hard: 12 candidate / 16 rank of 28).
# Track halted at the gate per design note â€” Step 2 (B8 re-run on MiniLM
# serving; live-node model_id coupling) and Step 3 (ranking) not authorised
# yet.
# ---
# B10 (MiniLM serving) full re-run, 2026-10-14. Code: 55f107f (last code
# commit touching cmhm_core/cmhm_mcp); serving binary built from that tree
# with --features onnx (MSVC), SHA256 8a5ea1288a13fe82b455c8c79ba2041ee88
# e1f26f3952248cb75ef8016ccdb4f — verified byte-identical to the MSVC onnx
# build dir and confirmed running (process path + file size 10,080,768 vs
# hash backup 7,572,480).
#
# Win rule (pre-registered, b10-minilm-step2.md): a task counts as a cmhm
# win only if cmhm is PASS AND has fewer calls AND less wall time than BOTH
# baselines; ties favor the baselines. Clear win for the step = cmhm wins
# >=4/7 tasks.
#
# | task | none: calls/wall/pass | notes: calls/wall/pass | cmhm (MiniLM serving) calls/wall/pass | winner |
# |---|---|---|---|---|
# | t1-mods-tab-missing | 16 / 52.1s / PASS | 12 / 33.2s / PASS | 14 / 37.7s / PASS | notes |
# | t2-ethernet-500mbps | 5 / 70.0s / FAIL | 5 / 112.4s / PASS | 7 / 732.2s / FAIL (missed iperf pitfall group) | notes |
# | t3-openbot-shortcut | 16 / 34.2s / FAIL | 15 / 49.7s / PASS | 7 / 1041.0s / PASS | notes (tie-break: calls 7<15 but wall 1041>50 |
# -> not outright; rule says ties/none-outright favor baselines) |
# | t4-hunt-wal-replay | 23 / 222.9s / PASS | 15 / 99.0s / PASS | 21 / 2642.9s / PASS | notes |
# | t5-hunt-bm25-scoring | 23 / 284.1s / PASS | 18 / 129.1s / PASS | 17 / 2219.8s / PASS | notes (17 vs 18 calls is a tie by rule; wall much worse) |
# | t6-hunt-trust-boundary | 31 / 156.2s / PASS | 30 / 219.3s / PASS | 27 / 3368.7s / PASS | notes |
# | t7-hunt-api-surface | 39 / 264.2s / PASS | 43 / 174.8s / PASS | 12 / 1748.7s / PASS | cmhm (calls 12 < 39 and < 43) but wall 1749 > both |
#
# Verdict per the pre-registered rule: cmhm wins 0/7 outright (no task
# satisfies calls AND wall vs both baselines). Clear-win bar >=4/7: FAIL.
# Correctness: 6/7 PASS; T2 regressed vs the hash re-run (FAIL, only missed
# the minor iperf-pitfall rubric group — same group the hash run caught).
#
# Latency on the serve path, measured after the run on the idle live server
# (2026-10-14): context_create 29ms; learn (3 embeds) 420ms; retrieve (1
# embed) 12ms mean; 3 concurrent retrieves 30ms total (~10ms each); 2x3
# concurrent 58ms (~10ms each). Embedder init is once per process
# (OnceLock get_or_init in cmhm_mcp/src/tools.rs) — NOT per call. The
# 10-20x wall inflation vs hash runs is therefore NOT server-side embedding
# cost (total embed time per arm is <1s). Best-supported explanation:
# agent-side LLM latency variance under 7 concurrent arms (same model
# concurrency pattern existed in prior runs; today's provider latency was
# simply higher). Not proven to exclusion; flagged as measurement caveat.
#
# Repro note: hash re-run (B8r) rows for the same tasks: T1 14/37.7s/P,
# T2 10/94.2s/P, T3 24/73.0s/P, T4 19/121.0s/P, T5 24/154.1s/P,
# T6 32/176.2s/P, T7 33/157.4s/P, for comparison.
# Compliance: all 7 B10 arms used exactly 1-2 retrieves + 0 learns
# (verified from opencode.db); hunt worktrees verified clean; live b10-*
# contexts deleted 2026-10-14 — NOT at run close as first recorded (that
# claim was wrong): the client's catalog refresh killed the serve process
# without clean shutdown, the registry lost their entries (context_list
# showed only 3 stale July contexts; context_delete by UUID returned
# E_NOT_FOUND), leaving 7 orphaned context folders under data/contexts.
# Removed from disk directly; verified 0 remain. Durability observation
# for the reviewer: context data dirs can outlive their registry entries
# after an unclean server kill (registry flush lost), orphaning contexts.
#
# Net: MiniLM serving did not move the standing verdict. Facts file still
# wins the majority. MiniLM adds semantic recall (proven in Session L:
# hard pool 0.10 -> 0.60) but at the B8 task level the bottleneck is not
# semantic recall of seeded facts. No further builds authorised; Step 3
# (ranking) decision is the owner's, informed by B9/B10 evidence.

# Post-B11-fix benchmark re-run (current master, 2026-10-17 - not hand-edited)
#
# Context: B11 registry lost-update fixed (c48ee4a: exclusive sidecar lock +
# reload-merge save; acceptance repro green per b11-registry-durability.md,
# docs commit 7a491b1). Serving binary rebuilt MSVC+onnx and swapped onto the
# GNU path: sha256 acde87b2cf4ee4d160cd61a05c6aa650eb2bcfcf28d52582c46b3eec8b6ed118.
# Workspace suite --no-fail-fast (S-10 protocol): 55 binaries, 0 FAILED.
#
# Commands (repo root, release):
#   cargo +stable-x86_64-pc-windows-gnu test --release -p cmhm_core --test ops_bench_v2 -- --ignored --nocapture
#   CMHM_LOAD_N=10000 [same] --test loadbench -- --ignored --nocapture
#   cargo +stable-x86_64-pc-windows-msvc test -p cmhm_core --target x86_64-pc-windows-msvc --features onnx --test ops_bench_minilm_v2 -- --ignored --nocapture
# Corpus: tools/corpus_l1.json, n=200 (100 lexical, 29 para-easy, 41 para-medium, 30 para-hard).
#
# HASH leg (2 fresh runs): seedP@1 / poolRec / MRR bit-stable and equal to the
# Session L record. WITH-fused aggregates recompute to R@1 = 101/200 = 0.505,
# pool = 112/200 = 0.560, MRR = 0.511 (matches record exactly).
#
# CHAIN metric finding: chainAcc varies run-to-run on IDENTICAL code
# (lexical WITH-fused: record 0.14, this session 0.24 then 0.17, same build).
# Root cause by inspection (not fixed): retrieval.rs:886 Thompson-sampled
# Dijkstra - chain edge costs are random draws per retrieve, so the reported
# chain is a stochastic path. chainAcc is NOT a bit-reproducible metric;
# single-run chain numbers are draws, not delta-comparable values. Any
# future chain gate needs an n-run mean+spread or a determinism change in a
# locked file (not authorised here).
#
# MiniLM leg (1 run, fresh): WITH-fused == task.md Session L record
# bit-identical: easy 0.2069/0.6207, medium 0.1463/0.6829, hard
# 0.0667/0.6000 (seedP@1/poolRec); lexical 1.00/1.00. Aggregates: R@1 =
# 114/200 = 0.570, pool = 164/200 = 0.820. Stage attribution identical to
# the record: candidate 36 / rank 50 total (hard 12 candidate / 16 rank).
#
# Loadbench 10k (fixed build): learn throughput 349/s (record 190/s - wall
# variance across sessions, not code: nodes stored 7941 identical in both);
# retrieve mean 1.34ms / p50 1.27ms / p99 2.62ms at k=200 (record 1.84/1.80/2.70);
# disk 5138 B/node (record ~5.0KB). Gates: p50 <= 100ms PASS (~79x headroom),
# p99 <= 500ms PASS (~190x headroom).
#
# POST-FIX GATE STATUS: unchanged by the fix, as expected (registry
# durability has no retrieval path). R@1 0.505 hash / 0.570 MiniLM - MISS
# (gate 0.70); hard R@1 0.000 / 0.067 - MISS (gate 0.50); pool 0.560 / 0.820
# - MISS (gate 0.85); latency PASS big. R1 (release blocker) stands until
# the embedder/rank question is resolved (B12 design pending owner gate).

# OWNER DECISION (2026-10-17): R1 target re-defined - hard-only, zero word overlap
#
# Owner clarification: the recall bar is 70% Recall@1 on the HARD slice only,
# where every query word is new (no same words, no shared wording vs the
# stored text). The L1 corpus already enforces exactly that rule for all
# paraphrase pairs ("query and situation share ZERO content tokens",
# CORPUS_PROTOCOL.md #3; checker tools/check_corpus_l1.py re-run green
# 2026-10-17: [L1] PASS: corpus clean, hard=30).
#
# Hard-only scoreboard vs the 70% owner target (same bench runs, 2026-10-17):
#   hash-128d : R@1 0/30 = 0.0%
#   MiniLM    : R@1 2/30 = 6.7%   (seedP@1, WITH-fused serving path)
#   MiniLM hard pool recall: 18/30 = 60% (in pool, rank drops them)
#   miniLM hard miss attribution: 12 candidate-side / 16 rank-side
#   noise floor: 1 hard pair = 3.3 pts (n=30); overlap-free by construction
#
# Supersedes the earlier dual shipping gates (overall R@1 >= 0.70 / hard
# >= 0.50) as the deciding R1 criterion; those numbers stay in the tables
# above for history. The overall 0.70 / pool 0.85 els do not disappear as
# product targets but R1 gating now reads hard-only.

# CORRECTION (2026-10-17, reviewer seat): stage attribution != mechanism; ceiling math
#
# The "rank-side 16 vs candidate-side 12" hard split counts WHERE the pipeline
# loses a target relative to the fused top-1. It is NOT evidence that the
# alpha=0.25 authority weight buries paraphrase hits: in the Session L bench
# every node carries equal authority, so that weight cannot reorder them there,
# and the anchors own earlier run (plain unprojected MiniLM brute force)
# scored 1/30 on hard vs CMHM fused 2/30 - the usable ordering is essentially
# the embedder cosine itself. Until the rerank bench below measures it, the
# honest mechanism statement is: the information is largely absent from the
# geometry, with fusion performing near-limit on this corpus. retrieval.rs
# stays untouched pending that measurement (locked file; no evidence = no edit).
#
# Ceiling (measured pools, hard cap arguments):
#   hard-only R@1 cap  = hard pool recall  = 18/30      = 60.0%
#   med+hard  R@1 cap  = (28+18)/71        = 46/71      = 64.8%
#   overall   R@1 cap  = overall pool      = 164/200    = 82.0%
#   Current med+hard R@1 = (6+2)/71 = 8/71 = 11.3%.
# Every strict paraphrase reading of the 70% target therefore requires a
# candidate-side (embedder/candidate-generator) improvement FIRST; no ranker
# alone can exceed the caps above with the current embedder. Rank-only spend
# before that is capped at +16 hard pairs (worst case 2 -> 18 R@1).
#
# Bench-only rerank probe (pre-registered before any run, no engine change,
# no locked file): rerank the EXISTING top-20 pool of each paraphrase query
# with a small cross-encoder inside a test. PASS = rank-only gain >= +10 of
# the 30 hard pairs (suggested threshold from the reviewer seat, not derived
# from data; worst case cap +16). Purpose: quantify how much ranking alone
# recovers from the existing pool before any embedder spend.
