# T5 — Hunt retrieval/BM25 bugs (fixture: pre-F1 code)

## Prompt (all arms)

> READ-ONLY bug hunt. No file modifications, no fixes, no commits. Working
> directory: `<fixture>/` (Rust workspace: cmhm_core + cmhm_mcp).
>
> TARGET AREA: retrieval path — `cmhm_core/src/retrieval.rs` (retrieve(),
> seed scoring, RRF fusion, conflict detection, abstention),
> `cmhm_core/src/bm25.rs` (tokenizer, scoring, rrf_fuse, index maintenance
> on node updates/removals), `cmhm_core/src/node_store.rs`
> (nearest/nearest_by_type, cosine, superseded filtering).
>
> HUNT FOR: scoring bugs where one signal wrongly dominates another,
> fusion/rank bugs, tokenizer inconsistencies between dense and BM25 paths,
> index maintenance bugs on re-index/remove. Report each finding as:
> file:line, mechanism, and why it matters. Rank by severity.

## Starting state

Fixture worktree at the pre-fix commit (BM25 length accounting corrupts
average document length on every re-index — present by construction).

## Verifier (keyword rubric — deterministic)

Must name ALL of (case-insensitive):
1. Fusion/scale problem: `rrf` OR `fusion` OR `reciprocal`.
2. The dominated signal: `authority` OR `total_length` OR `avgdl` OR
   `average document length` OR `scale` OR `mismatch` OR `suppress`.

Tool calls + wall time recorded per arm.
