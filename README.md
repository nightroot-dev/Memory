# CMHM — Causal-Matrix Hybrid Memory

A local-first causal memory engine for AI agents. Runs on a laptop,
remembers what worked, no cloud, no API keys, no GPU.

## What it is

CMHM is a persistent memory layer that gives AI agents the ability to
learn from their own experience. It stores events as typed nodes
(State, Action, Outcome), links them with weighted causal edges, and
retrieves the causal chain that explains *why* something happened —
not just *what* looks similar.

It runs entirely on CPU, in a single Rust process, with no external
services. A decade of daily experience fits in roughly 80 MB of RAM.

## Why it's different

- **Causal, not just associative.** Stores `state → action → outcome`
  chains with Beta-Bernoulli confidence. Answers "what happened last
  time we saw this?" and "which action actually fixed it?"
- **Local-first.** No cloud dependency. Nothing leaves the machine.
- **Crash-safe.** WAL with CRC32C + versioned atomic snapshots.
  Survives process kills and power loss.
- **Model-agnostic.** Ships with a fast hash embedder (128-dim, zero
  deps) and optional MiniLM ONNX embeddings (384-dim, ~14 MB model).
- **Small.** ~80 MB per agent for a decade of daily use, measured.
- **Zero LLM calls.** No tokens spent on memory. No API keys.

## Install

Requires Rust 1.75+.

    git clone <repo-url>
    cd Memory
    cargo build --release

Optional MiniLM support (semantic embeddings via ONNX Runtime):

    # Requires MSVC toolchain on Windows; see docs/ONNX_SETUP.md.
    cargo build --release -p cmhm_core --features onnx

## Quick start

    cargo run --example quickstart -p cmhm_core

See `cmhm_core/examples/quickstart.rs` for the ~40-line example.

Minimal use:

    use cmhm_core::{Config, engine, NodeType, CallerRole, text_to_embedding};

    let mut cfg = Config::default();
    cfg.data_dir = "/path/to/data".into();

    let handle = engine::start(cfg)?;

    // Learn: (state, action, outcome) with prediction error
    handle.learn_with_text_context(
        text_to_embedding("database connection pool exhausted").to_vec(),
        text_to_embedding("increased pool size to 100").to_vec(),
        text_to_embedding("connection timeouts stopped").to_vec(),
        0.2, 0.5, "hash-128d",
        0.5, String::new(), None,
        CallerRole::User, None, None,
        Some((
            "database connection pool exhausted".into(),
            "increased pool size to 100".into(),
            "connection timeouts stopped".into(),
        )),
    )?;

    // Retrieve causal chain for a new situation
    let result = handle.retrieve(
        text_to_embedding("connections are failing again").to_vec(),
        "hash-128d",
        NodeType::State,
        "connections are failing again",
    )?;
    println!("{}", result.explanation);

## MCP integration

`cmhm_mcp` is a Model Context Protocol stdio server. Configure it in
your MCP host (Claude Code, Cursor, OpenCode, etc.) to give any AI
agent persistent memory:

    {
      "mcpServers": {
        "cmhm": {
          "command": "path/to/cmhm_mcp.exe",
          "args": ["--config", "path/to/config.toml"]
        }
      }
    }

Exposes 12 tools including `memory_learn_text`, `memory_retrieve_text`,
`context_create`, `context_route`, `memory_feedback`.

## Benchmarks

Measured on n=80 scenarios (40 lexical, 40 paraphrase) via
`cmhm_core/tests/minilm_bench.rs`:

| Embedder   | Lex P@1 | Para P@1 | Overall P@1 | Overall MRR |
|------------|---------|----------|-------------|-------------|
| hash-128d  |  0.4750 |   0.0750 |      0.2750 |      0.3208 |
| MiniLM     |  0.4250 |   0.4000 |      0.4125 |      0.5813 |

**MiniLM paraphrase P@1: 0.4000 vs hash-128d 0.0750 — 5.3× improvement.**

Prior work (Session B, `bm25_fusion_bench`, clean corpus):
hash-128d + BM25 reaches 0.925 lexical P@1 and 0.488 overall P@1.

Caveats are documented in `task.md`. Numbers were measured after the
store/query dim fix in commit `987706b`; earlier MiniLM numbers in the
ledger were contaminated by that bug and are marked historical.

Test suite: 274 core + 61 MCP tests, 0 failures.

## Architecture

    cmhm_core/   — engine: nodes, edges, retrieval, WAL, snapshots
    cmhm_mcp/    — MCP stdio server exposing 12 tools
    data/        — runtime storage (WAL, snapshots, projections)
    tools/       — projection training, ORT setup, corpora validation

Key subsystems:
  - Node store: typed nodes with 128-dim embeddings, Beta bandit priors
  - Edge store: Beta-Bernoulli causal edges with time decay
  - Retrieval: dense cosine + BM25 (RRF fusion) + Dijkstra causal walk
  - Persistence: WAL v4 with CRC32C, 3-generation atomic snapshots
  - Context OS: FSM lifecycle, registry, LRU cache, router
  - Projection: per-model native → 128-dim matrices, auto-loaded at start

## Status

Pre-1.0. Production-ready for single-user local agents. Not yet
multi-tenant, not yet multi-process. See `task.md` for open items.

## License

MIT. See LICENSE.
