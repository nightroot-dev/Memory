# B8 eval harness — protocol (approved task list, 7 tasks)

## Question
Does CMHM memory reduce tool calls + time to a correct diagnosis, vs baselines?
Kill criterion: losing to the plain-notes arm on a majority of tasks means
"not memory yet" — no patch follows a loss, only a design question.

## Layout
- `tasks/<id>/task.md` — prompt + starting state + verifier spec (this file).
- `tasks/<id>/NOTES.md` — notes-file arm only. NEVER shown to other arms.
- `tasks/<id>/cmhm-seeds.md` — entries learned into isolated `b8-<id>`
  contexts before CMHM-arm runs. NEVER shown to other arms.
- `tasks/<id>/fixtures/` — prepared fixture (machine tasks) or pointer to a
  fixture worktree (hunt tasks, see Fixtures below).

## Fixtures
| task | fixture |
|---|---|
| t1-mods-tab-missing | `tasks/t1-*/fixtures/` (logs + mod tree + stray DLLs) |
| t2-ethernet-500mbps | `tasks/t2-*/fixtures/` (symptom + iperf + adapter notes) |
| t3-openbot-shortcut | `tasks/t3-*/fixtures/OpenBot` (depth-1 clone, 2026-10-13) |
| t4-hunt-wal-replay | worktree `../Memory_b8t4` (`69f603a^`, pre-R1) |
| t5-hunt-bm25-scoring | worktree `../Memory_b8t5` (`abbc5a8^`, pre-F1) |
| t6-hunt-trust-boundary | worktree `../Memory_b8t67` (`0c8bba4^`, pre C-01/C-04) |
| t7-hunt-api-surface | worktree `../Memory_b8t67` (same; different prompt/verifier) |

Machine-task arms work on a per-arm COPY of `fixtures/` (pristine fixture
must stay intact — verifier item 1 for T1 depends on it). Hunt-task arms
work read-only in the fixture worktree (fresh `git status` must stay clean;
the runner checks `git status --short` is empty afterwards and discards runs
that dirty the tree).

## Arms (per task)
1. **none** — prompt only. No CMHM tools, no NOTES.md.
2. **notes** — prompt + the task's NOTES.md pasted after it. No CMHM tools.
3. **cmhm** — prompt + `use context_id b8-<id> for memory calls` (context
   pre-populated from `cmhm-seeds.md` by the runner, then the runner hands
   the arm the context id). No NOTES.md.

Each arm runs as one fresh subagent session in the fixture (work) dir.
The agent's final message is saved as `runs/<task>/<arm>/report.md`.

## Metrics (collected after each run, never self-reported)
- Tool calls: count of `type:"tool"` messages in `session_message` for the
  arm's session id (opencode.db, read-only).
- Wall time: `time_completed - time_created` span of the arm's messages.
- Correctness: `verify.py <task-id> <report.md>` exit 0/1 + file-state
  checks in `task.md` where specified.

## Results
`results.md` table (this file is written by the runner, by hand after the
runs — one row per task×arm: calls, time, pass/fail, notes). Win condition:
CMHM arm uses fewer calls AND less time than BOTH baselines on a majority
of the 7 tasks. Ties favor the baselines (memory must earn its overhead).

## Pilot first
T4 × 3 arms validates the harness end-to-end (smallest task, crispest
verifier). Full 7-task eval only after the pilot passes review.
