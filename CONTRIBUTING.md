# Contributing

CMHM is a local-first causal memory engine for AI coding agents (Rust,
MCP over stdio). Thanks for helping keep it lean and honest.

## Build and test

Windows (the shipped platform), Rust stable:

    cargo build
    cargo test -p cmhm_core --lib                # engine library suite
    cargo test -p cmhm_mcp --test tool_tests     # MCP tool layer suite

CI (`.github/workflows/ci.yml`) runs exactly those two suites on every
push to master, windows-latest with default features. The optional onnx
embedder is exercised only in local benches (see `.cargo/config.toml`
comments for the MSVC build recipe) — not in CI.

## House rules

- Never stage blindly (`git add -A`). Some directories are personal
  runtime state and must never be committed or shipped: `backups/`,
  `tools/eval_b12/runs/`, `tools/eval_l3/__pycache__/`, `data/`,
  `contexts/`.
- Frozen documents: `CMHM_Context_OS_Spec_v1.0.md`,
  `MATHCONTEXT.MD` (frozen specs v1.0) — propose changes via issues, not
  direct edits. `CONTEXT.MD` is the engineering reference; also prefer
  issues. `docs/master_bug_list.md` and `task.md` are owner-maintained.
- Scope check: consult `CMHM_Context_OS_Spec_v1.0_BUILD_STATUS.md`
  before building anything. What is deferred there (DAG fork/merge,
  cross-context ACID transactions, cloud storage backends, RAM-tier
  scheduling) needs a real, existing user before it gets built.
- Claims discipline: the engine provides "agent-assisted recall" —
  retrieval plus graph walk; the calling agent does the reading and
  selection. Published bench records are never edited after the fact;
  new claims need a new pre-registered run.

## Bench and tuning changes

Retrieval-facing behavior changes (weights, caps, ranking) require the
eval harness: pre-register a fixture with pass criteria as a note under
`tools/eval_*`, run it once, commit the raw output plus the verdict.
No post-hoc edits to published records.
