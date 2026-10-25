# B12 design note: two-session accumulation eval (locked pre-registration)

**Status:** design locked BEFORE any arm runs. Owner gate: checked against
the three definitions below before launch. No code changes; eval only.
**Question:** does CMHM's actual product claim — automatic accumulation of
facts across sessions — beat the realistic baseline (an agent writing its
own notes), given that B8/B8r/B10 measured it only against an ideal
hand-authored file?

## 1. Win rule (IDENTICAL to B10 — not redefined)

A task counts as a **cmhm outright win** only if ALL of:
1. cmhm session-2 rubric PASSES;
2. cmhm total tool calls < none AND < self-notes (ties favor baselines);
3. cmhm total wall time < none AND < self-notes (ties favor baselines).

Wall/calls measured exactly as B8/B10: tool-call rows + nested execute
subcalls from `session_message` payloads; wall = last − first message
timestamp per session, summed over the two sessions of the arm.
Reported but NOT gating: a calls-only tally (B10 showed wall noise from
LLM latency can dominate; the calls-only tally guards interpretation).

## 2. Sample size and inconclusive band (locked now)

- **n = 10 tasks** (b12-t1 … b12-t10), all run regardless of interim
  results — no early stopping.
- Let W = number of cmhm outright wins.
- **W ≥ 6 → B12 win:** the ideal-file baseline was the artifact; the
  product conversation reopens on honest evidence.
- **W ≤ 4 → loss:** the memory-quality track STANDS DOWN permanently.
  Remaining paths: narrow to what measurably works (lexical/identifier
  recall 1.00) or archive.
- **W = 5 → INCONCLUSIVE:** not a win. Raw table goes to the owner; no
  claim either way.

## 3. Self-notes arm fairness (instruction pre-registered verbatim)

Same agent type (general), same model (bench-agent-m xhigh), same task
prompts as the other arms. The ONLY difference is the notes instruction,
given identically in both sessions, verbatim:

> "As you work, maintain a file named NOTES.md in your work directory.
> Write down every fact you learn that would be needed to resume or redo
> this task later: root causes, file locations, commands, pitfalls,
> decisions. Keep it concise but complete. Do not write anything outside
> your work directory."

Session-2 prompt addition (verbatim): "A previous session worked on this
same task and may have left NOTES.md in the work directory. Read it first
if present."

No hand-authored seed content is given to self-notes — the arm writes its
own. The none arm gets no notes instruction and no memory access.

## 4. cmhm arm protocol (honest product shape — no hand-feeding)

- **No pre-seeded contexts.** Memory must accumulate from session 1 work.
  (B8's seeded contexts were the same ideal-file bias in different
  clothing; B12 removes it.)
- Session 1: learns unrestricted (agent follows the server instructions:
  learn outcome-changing events); retrieves ≤ 2.
- Session 2: retrieves ≤ 2; learns unrestricted.
- Compliance verified from opencode.db per arm (retrieve/learn counts).

## 5. Task family (10 two-session tasks)

Each task: session 1 = explore/diagnose a fixture and discover facts;
session 2 = a task that REQUIRES those facts (resume, extend, or fix a
related fault). Fixtures are new, small, deterministic, local (no
network); built and rubrics written BEFORE any arm runs, one
`task.md` + rubric per task under `tools/eval_b12/tasks/`. Rubric style:
file-state checks or keyword groups, deterministic, same as B8. The
session-2 rubrics are fixed before the first arm; nothing is tuned after
seeing results.

## 6. B11 condition (the trap, named and controlled)

**UPDATE 2026-10-17: B11 is FIXED** (commit `c48ee4a`: exclusive sidecar
lock + reload-merge save; acceptance repro green on the serving binary,
`docs/eval_b8` record 7a491b1). The original mitigation plan is downgraded
as follows:

- Arms still run strictly SEQUENTIALLY — one arm at a time, no parallel
  subagents running concurrently. **Exactly ONE cmhm_mcp server instance
  serves the whole eval** (B11's multi-writer clobber needs >=2 writers;
  single instance + sequential arms removes that class entirely).
- **Pilot (t1 × 3 arms × 2 sessions) is labelled a HARNESS CHECK, not a
  verdict.** It validates: runner mechanics, fixture copying, prompt
  verbatim-ness, registry tripwire, verify.py behaviour, DB metric
  extraction. It contributes no win/loss to W and carries no B12
  conclusion; its report is reviewed before the remaining 9 tasks run.
- **Model parity guard (pre-registered):** every arm session spawns with
  exactly `opencode/BENCH-MODEL-WITHHELD#xhigh` and this is
  verified from `session_v2.model` in opencode.db after each session.
  If that model cannot be spawned, the runner STOPS B12 and reports;
  no fallback model is authorised (B10 record's parity is bench-agent-m; no silent
  substitution).
- **Registry integrity check around every arm** becomes pure regression
  insurance (B11 reopened = any run's data void, re-run once): before and
  after each arm, record `registry.toml` entry set. Any unexplained
  disappearance = clobber = that arm VOID; second occurrence aborts B12.
- Statement for the record: **B12 runs with B11 FIXED** (`c48ee4a`). A
  cmhm loss in B12 is attributable to memory quality, not registry races.

## 7. What this note does not authorise

No engine or registry code changes; no reranker; no embedder work; no
changes to the B8/B10 records; no touching `docs/master_bug_list.md`
(B11 transplant remains the reviewer's).

## 8. Reporting

Raw table → measured vs assumed → not done, project standard. Registry
integrity log included. All reports, rubrics, and the registry log
committed before the verdict commit.
