# CMHM — Causal-Matrix Hybrid Memory

A local-first memory engine for AI agents built around one honest contract:

- **Exact lookup always works, engine-only.** If the words you search with appear
  in the stored memory, retrieval puts it at rank 1.
- **Vague (paraphrased) recall depends on the connecting agent's own LLM** doing
  a pick step over a shortlist. CMHM ships no model of its own beyond a small
  local embedder, makes no LLM calls, and never discusses pricing or quality of
  a model it does not ship.

No cloud, no API keys, no GPU. Everything runs on CPU in one Rust process,
surfaced to agents over the Model Context Protocol (MCP).

## What it is

CMHM stores experience as typed nodes (State, Action, Outcome) linked by
weighted causal edges with Beta-Bernoulli confidence, and retrieves the causal
chain that explains *why* something happened — not just what looks similar.
Snapshots and a CRC32C WAL make it crash-safe; a Context runtime (FSM
lifecycle, registry, LRU paging, router) separates per-project memory spaces.

Every recall number in this README comes from one fixed benchmark corpus,
`tools/corpus_l1.json` (n = 200: 100 lexical, 29 para-easy, 41 para-medium,
30 para-hard; paraphrase pairs share zero content tokens by construction;
frozen 2026-10-07). Full records and pre-registrations:
`tools/eval_b8/results.md` and the per-lever files next to it.

## The retrieval contract (read this first)

| Tier | What it requires | Measured result (with regime labels) |
|---|---|---|
| **1. Exact lookup** | The query's words appear in the stored memory text. | Rank-1 on **100/100** lexical queries; passed every recorded bench run (standing gate A). Engine-only, zero tokens, no LLM. |
| **2. Vague query, engine alone** | Nothing extra. | **2 of 30** rank-1 hits on the hard slice (every query word new vs. the stored text; single recorded run); hard-side pool recall 18/30. **We do not claim engine semantic recall.** |
| **3. Vague query, agent-assisted recall** | (a) at save time the caller's own phrasing is written into the memory text, and (b) at recall time the client agent's own LLM reads the <=20-candidate shortlist and picks. | **14/30 = 46.7%** rank-1 on the hard slice, Clopper-Pearson 95% CI **[28.3%, 65.6%]**. Hard slice only; single run; one bench-stand-in picker (`a stand-in client LLM (name withheld)`). Easy/medium slices with the pick over shipped pools: **unmeasured**. |

Label discipline we hold ourselves to:

- Tier 1 is a guarantee on this corpus's exact regime. Tier 3 is
  **agent-assisted recall**: the number is real but belongs to the stack
  (engine + your agent's model), is a single run on the hard slice, and depends
  on a model we do not ship. It is not "engine semantic recall".
- The best measured full-corpus stack (write-time keyword bridge + rerank) used
  keywords derived from the queries' own words — a **same-words ceiling** on
  this corpus, not a blind-regime claim. No drifted-phrasing full-corpus cell
  exists. We therefore publish no 100% figure and no overall recall number for
  tier 3.
- We make **no "better than a notes file" claim** at any store size; that
  comparison has not been measured.

## How tier 3 works (architecture: client-agent-as-reranker)

1. **Save.** A short suffix of the caller's own words joins the memory text (in
   the bench fixture: 20-60 characters, roughly 5-15 tokens per memory). This
   is a store-text discipline; zero engine changes and it is what makes
   candidates surfaceable at recall time.
2. **Recall.** The engine (local embedder + BM25 fusion) fills a candidate
   shortlist (<=20, with provenance and ranks).
3. **Pick.** Your agent's own LLM reads the shortlist and chooses. CMHM the
   process never calls any LLM; the engine alone still serves tier 1 without
   this step.

The shipped embedder is MiniLM-L6-v2 int8 (Apache-2.0, ~14 MB, downloads once
from Hugging Face on first run and caches locally; ~2 ms per embed on CPU).
A larger embedder (bge-base-en-v1.5, 768-d) also compiles but measured
swap-worse on this workload — it is available, not recommended.

## Known limits (all measured, none resolved)

1. **Save-side discipline is unmeasured with real agents.** In the recorded
   multi-agent run, agents completed their tasks with 1-2 retrieves and
   **zero learns on-task** — the tier-3 save rule has bench numbers but no
   in-the-wild evidence yet.
2. **Single run, single picker family.** All tier-3 numbers are one run with
   one bench stand-in (a personal-tier catalog model). No cross-model
   replication has been run; a second picker family is proposed, not executed.
3. **Pick visibility cap.** The shortlist is capped at 20 by the shipped fuse;
   in the gated D2-expanded runs, 8 of 26 pooled hard-side golds sat beyond the
   cap and were never shown to the picker (`lost_rerank_visibility = 8`).
4. **No drifted-phrasing full-corpus cell exists**, and no overall
   (easy/medium/hard) pick-over-shipped-pool number exists either.
5. **Cross-encoder rerank measured dead on this corpus** (net <= 1 of 30 hard
   pairs for two reranker classes); the LLM-class pick above is a different
   mechanism, and that measurement is what it is — do not read tier 3 as an
   embedder improvement.
6. **Single-user, single-process.** And one durability observation from the
   record: an unclean server kill can orphan context data directories on disk
   (registry flush lost).

## Install & setup

Requires Rust 1.75+.

    git clone <repo-url>
    cd Memory
    cargo build --release

That default build ships the zero-dependency hash embedder and serves tier 1
plus BM25 exact search. For tier 3 (MiniLM semantic candidates), Windows /
MSVC setup:

    powershell -ExecutionPolicy Bypass -File tools/setup_ort.ps1   # ONNX Runtime DLL, once
    cargo +stable-x86_64-pc-windows-msvc --target x86_64-pc-windows-msvc build --release -p cmhm_core --features onnx

Three things ride along; see `docs/ONNX_SETUP.md` (10 minutes, covered end to
end): MSVC toolchain, the ORT DLL, and the 384->128 PCA projection matrix
(`data/projections/minilm-l6-v2-int8.bin`; git-ignored build output —
regenerate with `python tools/train_minilm_projection.py` after
`pip install fastembed numpy`). `engine::start()` auto-loads every `*.bin`
there, keyed by filename stem as the model id. Missing or mismatched
projection/model fail closed (typed `ProjectionError`), never silently.

## Quickstart

    cargo run --example quickstart -p cmhm_core

See `cmhm_core/examples/quickstart.rs` for the ~40-line example
(`learn_with_text_context`, `retrieve`, explanation strings).

## MCP integration

`cmhm_mcp` is a stdio MCP server. Configure it in your MCP host (Claude Code,
Cursor, OpenCode, etc.):

    {
      "mcpServers": {
        "cmhm": {
          "command": "path/to/cmhm_mcp.exe",
          "args": [],
          "env": { "CMHM_CONFIG": "path/to/config.toml" }
        }
      }
    }

14 tools: `memory_learn_text` / `memory_retrieve_text` (string in/out, the
recommended path), `memory_learn` / `memory_retrieve` (vector API),
`memory_feedback`, `memory_promote`, `context_create` / `context_list` /
`context_delete` / `context_route`, `memory_stats` / `memory_list` /
`memory_get`, and `project_detect` (derives the project context id from the
environment).

All values and their defaults (router weights, retrieval blend, snapshot
cadence, context lifecycle timeouts) live in `config.toml`, documented inline.

## Operator cost of tier 3

Derived from the committed bench fixtures by `tools/eval_l3/cost_probe.py`
(4 chars/token convention):

| Step | Cost |
|---|---|
| Engine storage & retrieval | Zero tokens. Retrieve p50 ~1.3-1.8 ms, p99 < 3 ms at a 10k-learn store (single recorded runs). ~5 KB per node on disk. |
| Save-time keyword suffix | One-time ~5-15 tokens per memory (committed fixture mean ~29 chars). |
| Pick over a plain shortlist (the tier-3 published regime) | ~243 input + ~4 output tokens per vague recall (per-item median; the committed real batch files for that leg total ~7,800 input tokens across its 30 items). |
| Pick when the shortlist carries keyword-suffixed memory text | ~434 input tokens per vague recall (per-item median); ~44,900 input tokens across its committed 100-item leg. |
| Embed (MiniLM) | 0 tokens, ~2 ms per text CPU-local. |

The pick is paid in the tokens of *your* agent's model — its pricing, its
context. If you never steer, the engine costs no tokens, period.

## Benchmarks & evaluation protocol

- Corpus protocol and zero-overlap rule: `docs/CORPUS_PROTOCOL.md`; checker
  `tools/check_corpus_l1.py`.
- Every gate, pre-registration, verdict, and deviation for the recall levers
  (write-time keywords, query expansion, embedder swap / LLM rerank) is in
  `tools/eval_b8/results.md` (appended-only) and its per-lever record files.
  Gates were registered before runs and were never re-targeted.
- Bench command family (repo root):

      cargo +stable-x86_64-pc-windows-gnu test --release -p cmhm_core --test ops_bench_v2 -- --ignored --nocapture
      cargo +stable-x86_64-pc-windows-msvc test -p cmhm_core --target x86_64-pc-windows-msvc --features onnx --test <bench> -- --ignored --nocapture

- Workspace test suite, full protocol run (`--no-fail-fast`, sensitive to any
  FAILED): **453 passed / 0 failed / 8 ignored across 55 test binaries**
  (2026-10-19 run record; raw output committed next to the records).

## Architecture

    cmhm_core/     engine: nodes, causal edges, retrieval (dense + BM25 RRF fusion + Dijkstra causal walk), WAL, snapshots
    cmhm_mcp/      MCP stdio server (14 tools)
    cmhm_gateway/  gateway
    cmhm_sidecar/  registry sidecar (writer lock, durability)
    data/          runtime storage (WAL, snapshots, contexts, projections)
    tools/         corpus, eval fixtures and records, projection training, ORT setup
    docs/          protocols (corpus, phase approval), setup guides

Single-user local agents. Not yet multi-tenant, not yet multi-process. Status
and open items are governed by the appended-only records, not this README.

## License

- CMHM itself: MIT (`LICENSE`).
- Shipped embedder: MiniLM-L6-v2 int8 — Apache-2.0.
- Bench stand-ins only: the picker/embedder stand-in models from the
  opencode catalog (personal-tier catalog models) are **personal-tier**; they were
  used for measurement only, carry no license exposure into distribution, and
  nothing from them ships. Agents connect their own model for tier 3.
