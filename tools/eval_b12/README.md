# B12 eval harness — two-session accumulation family (10 tasks, pilot-first)

**Question:** does CMHM's automatic accumulation beat the realistic
self-notes baseline (agent writes its own notes and reads them next
session)? Design locked in `tools/eval_b8/b12-design.md` (committed
`da442bf`, reviewer-reviewed; win rule / n / band / verbatim notes arm /
B11 condition all locked). B11 is **fixed** since `c48ee4a`.

## Layout
- `make_fixtures.py` — deterministic fixture + task.md generator (committed; run once; byte-stable output).
- `tasks/b12-t<k>/task.md` — session-1 + session-2 prompt bodies + per-arm instruction blocks (verbatim).
- `tasks/b12-t<k>/fixtures/` — the task fixture.
- `verify.py` — session-2 rubrics (file-state in the run dir + report keyword groups). Pre-written; never tuned after runs.
- `runs/<task>/<arm>/` — per-arm work dir, shared by BOTH sessions of that arm (this is what persistence is tested on). `report-s1.md`, `report-s2.md` written by the runner.

## Arms (per task, identical prompts except the arm block)
1. **none** — prompt only.
2. **self-notes** — session 1 adds the verbatim note-taking instruction (b12-design §3); session 2 adds the verbatim "read NOTES.md first if present" line.
3. **cmhm** — session 1 adds: create a dedicated context (display_name exactly `b12-t<k>-cmhm`, type `session`) via `context_create` FIRST, then learn outcome-changing events per the server instructions (retrieves ≤ 2). Session 2 adds: `context_list` first, find the context whose display_name starts with `b12-t<k>-cmhm`, retrieve (≤ 2), then work. Arms may only touch contexts they created. NO hand-seeded content (b12-design §4).

**Arm block placement:** appended after the task prompt body; everything else
byte-identical across arms. No arm ever sees another arm's artifacts.

## Execution order (runner)
1. `python make_fixtures.py` → materializes tasks/. Fixture files committed.
2. **Pilot:** t1 × 3 arms × 2 sessions end-to-end before any other task.
   **The pilot is a HARNESS CHECK, not a verdict**: it validates mechanics
   (fixtures, verbatim prompts, tripwire, verify.py, DB metrics) only;
   results carry no W and no B12 conclusion. Pilot report reviewed before
   the remaining 9. Constraints: arms strictly sequential; exactly ONE
   cmhm_mcp server instance for the whole eval; every session spawns
   bench-agent-m xhigh (design §6 parity guard; failure = STOP, no fallback).
3. For each task, arms strictly sequential: for arm in [none, self-notes, cmhm]: fresh copy of fixtures/ into run dir → session 1 (subagent, fresh session) → save report-s1.md → session 2 (fresh session, same work dir) → save report-s2.md → `verify.py <task> <report-s2.md> <rundir>`.
4. **Registry integrity tripwire** around every cmhm arm: record the display_name list of `registry.toml` before and after. Unexplained disappearance = B11 reopened = that arm's data VOID (re-run once; second occurrence aborts B12). With the c48ee4a fix the list may only GROW or keep entries; Deleted tombstones are entries too and count as intact.
5. Metrics per session from opencode.db (read-only): total tool calls (type:"tool" events, including nested execute subcalls as in B10) and wall span (last − first message). Arm score = session1 + session2 sums. Win rule = b12-design §1 (PASS + fewer calls + less wall than BOTH baselines; ties to baselines). W ≥ 6/10 win; W ≤ 4 loss; W = 5 inconclusive (§2).
6. Model parity: every arm session bench-agent-m xhigh (checked from the DB sessions table).

## Honest-claims notes
- Session 2 of the `none` arm may legitimately re-discover everything; that cost in calls/time is exactly what the win rule prices. Correctness alone never wins anything (outright win needs calls AND wall).
- No early stopping: all 10 tasks run regardless of interim standings.
- Single runs are flagged as such; ±1 call tolerance is NOT applied — ties go to baselines by design.
