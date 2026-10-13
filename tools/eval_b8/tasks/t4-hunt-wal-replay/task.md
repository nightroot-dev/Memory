# T4 — Hunt persistence/WAL bugs (fixture: pre-R1 code)

## Prompt (all arms)

> READ-ONLY bug hunt. No file modifications, no fixes, no commits. Working
> directory: `<fixture>/` (Rust workspace: cmhm_core + cmhm_mcp).
>
> TARGET AREA: persistence layer — `cmhm_core/src/persistence.rs`,
> `cmhm_core/src/engine.rs` (start/worker_loop/process_learn, WAL append
> paths, snapshot/compaction), `cmhm_core/src/migration.rs`,
> `cmhm_core/src/core_state.rs`.
>
> HUNT FOR: crash-safety holes, WAL replay bugs (double-apply?
> checkpoint-then-reopen replays?), snapshot/compaction races, migration
> gaps. Report each finding as: file:line, mechanism, and why it matters.
> Rank findings by severity.

## Starting state

Fixture worktree at the pre-fix commit (WAL replay duplicates nodes across
restart — present by construction). No other info.

## Verifier (keyword rubric on the findings report — deterministic)

Must name ALL of (case-insensitive):
1. Replay/re-application: `replay` AND (`idempoten` OR `duplicat` OR `double-appl` OR `re-appl`).
2. The state that duplicates: `node` AND (`snapshot` OR `wal` OR `truncat` OR `watermark` OR `compaction`).

Tool calls + wall time recorded per arm.
