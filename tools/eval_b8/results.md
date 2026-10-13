# B8 results (written by the runner after arm executions — not hand-edited)

Win condition: CMHM arm uses fewer tool calls AND less wall time than BOTH
baselines on a majority of the 7 tasks. Ties favor the baselines.

| task | none: calls | none: time | none: pass | notes: calls | notes: time | notes: pass | cmhm: calls | cmhm: time | cmhm: pass | winner |
|---|---|---|---|---|---|---|---|---|---|---|
| t4-hunt-wal-replay (pilot) | 23 | 222.9s | PASS | 15 | 99.0s | PASS | 21 (1 retr) | 170.3s | PASS | notes |
| t1-mods-tab-missing | 16 | 52.1s | PASS | 12 | 33.2s | PASS | 19 (2 retr) | 158.5s | PASS | notes |
| t2-ethernet-500mbps | 5 | 70.0s | FAIL¹ | 5 | 112.4s | PASS | 11 (2 retr) | 152.3s | FAIL¹ | notes |
| t3-openbot-shortcut | 16 | 34.2s | FAIL² | 15 | 49.7s | PASS | 21 (1 retr) | 65.3s | PASS | notes |
| t5-hunt-bm25-scoring | 23 | 284.1s | PASS | 18 | 129.1s | PASS | 29 (1 retr) | 202.4s | PASS | notes |
| t6-hunt-trust-boundary | 31 | 156.2s | PASS | 30 | 219.3s | PASS | 32 (1 retr) | 99.6s | PASS | none³ |
| t7-hunt-api-surface | 39 | 264.2s | PASS | 43 | 174.8s | PASS | 34 (1 retr) | 108.6s | PASS | cmhm |

¹ T2 none/cmhm missed only rubric group 4 (the minor iperf-path pitfall);
both gave strong main diagnoses. Rubric stands as pre-registered.
² T3 none's shortcut target has a path typo (`tasks/3-` vs `tasks/t3-`) and
does not resolve; report itself was strong.
³ T6: no outright winner — notes fewest calls (30 vs 31/32), cmhm fastest
wall (99.6s vs 156/219s). Neither satisfies the AND condition.

Pilot notes (2026-10-13, all arms bench-agent-m xhigh, model parity holds):
- All three arms produced correct, verifier-passing reports. The harness
  discriminates on efficiency, not capability — as designed.
- notes beats cmhm beats none on both metrics. Per the kill criterion this
  task is a loss for CMHM (0/1): memory helped vs nothing (23→21 calls,
  223→170s) but lost to the ideal-facts file.
- Compliance verified from opencode.db: none/notes used zero memory tools;
  cmhm used exactly 1 retrieve (nested in execute) + 0 learns, as instructed.
- Metrics method: tool-call rows + nested execute subcalls counted from
  session_message payloads; wall time from message timestamps. Method pinned
  here so the full run uses the identical procedure.
- Runs' full reports live under tools/eval_b8/runs/ (working files,
  git-ignored, not committed).

Verdict: CMHM 1/7 — LOSES per the kill criterion (majority required).
Notes wins 5/7 outright; T6 has no winner. Memory helped vs nothing on most
tasks but lost to the ideal-facts file almost everywhere.

Caveats (read before citing):
- Single runs per cell, no warm-up; wall time includes model latency
  variance. Call counts are the stabler metric.
- Version skew UNDERESTIMATES cmhm: arms ran against the live server
  (pre-B1 stub retrieval — agents quote "no actionable text" and work
  around it), not repo code. A loss here is conservative-strong evidence;
  a re-run on current code could only help the cmhm arm.
- Compliance verified from opencode.db for every arm: none/notes zero
  memory calls; cmhm arms 1–2 retrieves + 0 learns as instructed.
- All arms bench-agent-m xhigh (model parity). Metrics method: tool-call
  rows + nested execute subcalls from session_message payloads.
- Fixture worktrees verified clean after all hunt runs.

# B8 re-run on current build (cmhm arms only, 2026-10-14 — not hand-edited)
#
# Context: live server upgraded to current master (post-B1 … S-25, v6
# snapshots); re-run seeds byte-identical (pinned cmhm-seeds.md) in fresh
# b8r-* contexts. none/notes cells are the ORIGINAL numbers (no server
# contact, still valid). Kill criterion unchanged: fewer calls AND less
# wall than BOTH baselines, majority of 7, ties favor baselines.
#
# | task | none: calls/time/pass | notes: calls/time/pass | cmhm re-run: calls/time/pass | winner |
# |---|---|---|---|---|
# | t1-mods-tab-missing | 16 / 52.1s / PASS | 12 / 33.2s / PASS | 14 (1 retr) / 37.7s / PASS | notes |
# | t2-ethernet-500mbps | 5 / 70.0s / FAIL¹ | 5 / 112.4s / PASS | 10 (1 retr) / 94.2s / PASS | notes² |
# | t3-openbot-shortcut | 16 / 34.2s / FAIL | 15 / 49.7s / PASS | 24 (1 retr) / 73.0s / PASS | notes |
# | t4-hunt-wal-replay | 23 / 222.9s / PASS | 15 / 99.0s / PASS | 19 (1 retr) / 121.0s / PASS | notes |
# | t5-hunt-bm25-scoring | 23 / 284.1s / PASS | 18 / 129.1s / PASS | 24 (1 retr) / 154.1s / PASS | notes |
# | t6-hunt-trust-boundary | 31 / 156.2s / PASS | 30 / 219.3s / PASS | 37 (1 retr) / 176.2s / PASS | none³ |
# | t7-hunt-api-surface | 39 / 264.2s / PASS | 43 / 174.8s / PASS | 33 (1 retr) / 157.4s / PASS | cmhm |
#
# ¹ Original rubric stands as pre-registered (see full-run table).
# ² T2: cmhm correctness fixed (FAIL→PASS — the iperf pitfall is recalled
#   now) but trails notes 10 vs 5 calls; beats notes on wall only. Winner
#   stays notes.
# ³ T6: same split as the original run — notes fewest calls (30), none
#   fastest wall (156s), cmhm neither. No outright winner; not-cmhm.
#
# Re-run verdict: CMHM 1/7 — LOSES per the kill criterion, same as the
# original run. The version-skew caveat is now retired: current code returns
# full stored text (verified: b8r-t1 retrieve renders the whole triple).
# Effects of the upgrade, measured: T2 FAIL→PASS (the iperf pitfall is now
# recalled), T1 19→14 calls / 159s→38s, T4/T5 faster than their original
# cmhm cells (170s→121s, 202s→154s); T6/T7 slower on wall than their
# original cells (100s→176s, 109s→157s — run variance) with T7 still an
# outright win. Notes still wins 5/7 outright and the tally doesn't move.
# Compliance re-verified from opencode.db: all 7 re-run arms 1 retrieve +
# 0 learns; model parity held (bench-agent-m xhigh); worktrees clean.
