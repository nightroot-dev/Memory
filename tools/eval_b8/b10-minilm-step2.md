# Step 2 design note: MiniLM serving + B8 re-run (B10 continuation)

**Status:** Step-1 gates passed (b10-minilm-step1.md, committed 4f8639e).
**Scope:** flip the *serving* embedder to MiniLM and re-run the cmhm arms
of B8 on the same 7 tasks. Everything else unchanged. One step; stops at
the gate.

## Owner-visible facts before this step proceeds (no silent choices)

1. **Live default context goes dark for retrieval.** Existing live nodes
   are hash-128d; N3 refuses cross-model comparison, so MiniLM queries
   treat them as absent. Data stays on disk untouched; a later re-embed
   step could resurrect them (not authorised here). The eval itself uses
   fresh b10-* contexts, unaffected.
2. **Restart required** (same as the v1 snapshot step): owner restarts the
   opencode client; binary is the command from opencode.jsonc
   (`target/x86_64-pc-windows-gnu/release/cmhm_mcp.exe`). Plan: build the
   MSVC+onnx binary in its own target dir, then copy it over that path —
   no config change.
3. **Projection file must be present in the live data dir**
   (`clever-wolf/data/projections/minilm-l6-v2-int8.bin`) — copied from the
   repo copy (197 KB). Without it all live vectors would be native 384-dim.
4. First server start downloads the 14 MB MiniLM model (one-time, cached).

## Hypothesis

H1: with MiniLM serving, the cmhm arm beats `notes` on a majority of the 7
B8 tasks (the pre-registered kill criterion, unchanged).

H0: serving embedder alone does not close the gap; remaining losses go to
the rank side (attribution: 36 candidate / 50 rank across para groups).

## Pre-registered verdict rule (set before the run)

- **Clear win**: cmhm wins ≥ 4/7 outright (fewer calls AND less wall than
  BOTH baselines, ties to baselines as before). → Step 3 never happens for
  recall reasons; discuss shipping next.
- **Improved but not winning** (cmhm better than its hash re-run on ≥ 3
  tasks but still loses majority): STOP building; record; the rank-side
  Step-3 decision goes to the owner with these numbers.
- **Flat or worse** (cmhm vs its hash re-run improves on ≤ 2 tasks):
  serving switch is not justified; revert to hash binary, stand down per
  the standing kill rule.

**Method constraints (same as B8):** fresh b10-* contexts, byte-identical
seeds (tasks/*/cmhm-seeds.md), cmhm arms only, none/notes stand, 1–2
retrieves + 0 learns per arm, worktrees verified clean, compliance checked
from opencode.db, bench-agent-m xhigh parity.

## Execution order (this step only)

1. Copy projection file into live data dir (done in same session).
2. Build MSVC+onnx release cmhm_mcp (background).
3. Owner restarts client; probe to confirm model_id = minilm-l6-v2-int8.
4. Seed 7 b10-* contexts (seeding happens AFTER the switch — seeds must be
   embedded by the serving model).
5. Run 7 cmhm arms; collect; report; stop at gate.
