# T6 — Hunt projection/embed/auth bugs (fixture: pre-C-01/C-04 code)

## Prompt (all arms)

> READ-ONLY bug hunt. No file modifications, no fixes, no commits. Working
> directory: `<fixture>/` (Rust workspace: cmhm_core + cmhm_mcp).
>
> TARGET AREA: model/projection/trust plumbing —
> `cmhm_core/src/projection.rs` (project, from_bytes, registry,
> load_from_dir), `cmhm_core/src/embed.rs` + text_embed (hash embedder
> normalization/determinism), `cmhm_core/src/authority.rs`
> (authenticate_and_clamp, role privilege logic), `cmhm_core/src/config.rs`
> (defaults vs documented values), engine birth paths.
>
> HUNT FOR: trust-boundary bugs (who decides authority?), model-identity
> mismatches (vectors compared across geometries?), silent fallbacks that
> change security posture. Report each finding as: file:line, mechanism, and
> why it matters. Rank by severity (trust issues first).

## Starting state

Fixture worktree at the pre-fix commit (an attacker-influenced caller role
and a model-identity fallback are present by construction).

## Verifier (keyword rubric — deterministic)

Must name ALL of (case-insensitive):
1. Trust bypass: `caller_role` OR `caller role` OR `bypass` OR `privilege` OR
   `agent` with `default`.
2. Identity fallback: `model_id` AND (`fallback` OR `default` OR `mismatch`
   OR `geometry`).

Tool calls + wall time recorded per arm.
