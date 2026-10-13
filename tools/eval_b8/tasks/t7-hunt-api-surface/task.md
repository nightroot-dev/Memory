# T7 — Hunt MCP/context API bugs (fixture: pre-gate-hardening code)

## Prompt (all arms)

> READ-ONLY bug hunt. No file modifications, no fixes, no commits. Working
> directory: `<fixture>/` (Rust workspace: cmhm_core + cmhm_mcp).
>
> TARGET AREA: API surface — `cmhm_mcp/src/*.rs` (tool handlers,
> ingestion gates, promotion paths), cmhm_core context runtime
> (create/sleep/wake/evict, registry, router), config defaults vs docs.
>
> HUNT FOR: unclamped inputs reaching scoring, sleeping/dead contexts served
> as live, lock/poisoning hazards, parser gaps, novelty/gate miscalibration.
> Report each finding as: file:line, mechanism, and why it matters. Rank by
> severity. At least TWO distinct defects required for a passing report.

## Starting state

Fixture worktree at the pre-fix commit (several handler/runtime defects
present by construction).

## Verifier (keyword rubric — deterministic)

The report must name at least TWO of these five areas (case-insensitive),
each with a `cmhm_mcp/src` or context-runtime file path nearby:
1. Gate clamping: `clamp`.
2. Sleeping contexts: `sleep` OR `wake` OR `evict` OR `loaded`.
3. Lock/poisoning: `mutex` OR `poison` OR `lock`.
4. Manifest parsing: `manifest` OR `Cargo.toml` AND (`section` OR `package`).
5. Novelty/gate range: `novelty` AND (`clamp` OR `1.0` OR `range`).

Tool calls + wall time recorded per arm.
