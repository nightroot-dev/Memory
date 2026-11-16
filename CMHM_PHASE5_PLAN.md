# CMHM Phase 5 — Experience OS

Status: **in progress** (last updated 2026-11-16). This is the working plan for
the Phase 5 slices. It is agent-facing; `CONTRIBUTING.md` still owns the house
rules, and `docs/master_bug_list.md` / `task.md` stay owner-maintained.

## Problem

The store remembers outcomes, but it does not distinguish *what was decided*
from *what failed and must not be repeated*. Retrieval is a single hybrid
ranking with no query-shape awareness, and the agent can only find a past
mistake after it has already acted. Phase 5 adds a typed experience layer, a
query router, a second-stage reranker, and a hard pre-flight veto.

## Scope

In scope:

- Typed experience metadata on nodes: disposition, blocked process, trigger
  signature, decision payload.
- Symbol-aware query routing and a bounded Stage 1 candidate pool.
- A fine-tuned cross-encoder reranker (int8 ONNX) over the Stage 1 pool.
- `preflight_check` (hard veto) and `record_experience` (typed write) MCP tools.
- Snapshot v7 with a deterministic v6 → v7 migration.

Out of scope: DAG fork/merge, cross-context transactions, cloud storage,
RAM-tier scheduling (still deferred per the build-status doc), and any change
to published bench records.

## Design

### 1. Typed experience memory

`Node`/`NodeRecord` gain additive fields:

- `trigger_signature`: normalized symptom/trigger text (lowercase, punctuation
  collapsed, stopword-free). Indexed for retrieval in place of legacy text when
  present.
- `decision_payload`: the decision or fix that follows the trigger.
- `disposition`: `Neutral` (default), `BlockedProcess`, `ExperienceRule`.
- `blocked`: `BlockedProcess { action_signature, root_cause, recommended_fix,
  failure_count }` for hard failures.

Only `BlockedProcess` can veto. `ExperienceRule` is advisory by design.

### 2. Persistence

- `SNAPSHOT_VERSION = 7`; `migrate_v6_to_v7` is deterministic and gives old
  nodes neutral defaults (no fabricated experience).
- Experience annotations are written as a separate idempotent WAL record
  (`0x08`), so replay after a crash cannot double-apply them.

### 3. Query router and Stage 1

`QueryMode::classify` splits exact-symbol/error-shaped queries from
natural-language symptoms:

| Mode | BM25 | Dense |
| --- | --- | --- |
| `LexicalDominant` | 0.75 | 0.25 |
| `SemanticBalanced` | 0.40 | 0.60 |

BM25 is min-max normalized per query; an exact-identifier boost is applied
before truncation. Stage 1 keeps `STAGE1_CANDIDATE_POOL = 25` candidates for
Stage 2.

### 4. Stage 2 reranker

- Base: `cross-encoder/ettin-reranker-68m-v1`
  (ModernBERT → CLS pooling → Dense(512) → LayerNorm → Dense(1)).
- Trained on trigger-only pairs; the runtime scores `(query, trigger_signature)`,
  never the decision payload.
- Exported as `model_quantized.onnx` (int8, output name `logits`) with tokenizer
  and config files. Loaded through `cmhm_core::reranker::ExperienceReranker`.
- int8 export uses `per_channel=True, reduce_range=True`: plain dynamic
  quantization saturated the Dense(1) head and inverted rankings on this model.
  The fp32 graph is made self-contained (torch's external `.data` inlined) and
  `special_tokens_map.json` is written explicitly because fastembed requires it.
- `rerank_top_k` is stable and drops invalid/NaN scores; a reranker failure
  must degrade to Stage 1 order, never to an empty result.

### 5. Pre-flight veto

`preflight_check(planned_action, target?, context?, context_id?, user_override?)`:

1. Retrieve over `NodeType::State` with the action + target + context as query.
2. Abstain/error → fail closed (the caller is told to resolve, not to proceed).
3. For each of the top 25 candidates, compute calibrated confidence
   `sigmoid((raw − 0.45) × 8)`.
4. A `BlockedProcess` veto requires **both**
   `confidence >= 0.78` and action Jaccard overlap `>= 0.34`; a shared tool
   name alone never vetoes.
5. `user_override: true` grants a one-time `OVERRIDE_GRANTED`; below threshold
   the response is `ALLOWED_NEAR_MISS` (advisory, never silent).

`record_experience` writes the typed node and is WAL-logged like any learn.

## Data pipeline

`tools/mine_git_triplets.py` mines external repositories only (10 repos under
`tools/phase5_data/repos`), one commit per row, with a
`@@CMHM_COMMIT_7f3a@@` marker in the generated audit corpus. Each row carries
`query`, `positive` (`trigger :: decision`) and a hard `negative`.

Produced corpus: **3,620 triplets** — train 3,170 / val 268 / test 182;
val = vitejs/vite, test = tokio-rs/tokio. This is below the 5,000-row target;
the exact count is reported rather than padded.

### Quarantine and audit gate

Never read, sample, paraphrase, or train on: `tools/eval_b8..b13/`,
`tools/eval_d2/`, `tools/eval_l3/`, `tools/eval_w1/`, `tools/corpus_l1.json`,
`tools/eval_b13/corpus_b13_discipline.json`, `tools/validated_pairs.json`.
Held-out Gate 3 tests may be *run*, not mined.

Before training, the candidate corpus must pass
`scripts/audit_corpus_overlap.ps1` (zero attempt-1/2/3 reuse, Jaccard < 0.30,
no verbatim run ≥ 6 words, no internal duplicates) and an embedding check with
max cosine < 0.80 against held-out queries. Training is blocked until the
official audit log exists and says PASS.

Result (2026-11-16): **PASS** — 803,640 pairwise checks, max Jaccard 0.0526,
0 violations, 0 attempt-1/2/3 exact matches. Log:
`tools/phase5_data/overlap_audit.log`. The script extracts 0 strings from
`dprime_bench.rs` (its corpus uses tuples, not `query:` fields); the held-out
(96 strings) and three attempt lists (126 strings) are enforced.

## Acceptance gates

| Gate | Command | Criterion |
| --- | --- | --- |
| Core suite | `cargo test -p cmhm_core --lib` | all green |
| MCP tools | `cargo test -p cmhm_mcp --test tool_tests` | all green |
| Phase 5 experience | `cargo test -p cmhm_core --test phase5_experience` | 1/1 |
| Query router | `cargo test -p cmhm_core --test phase5_query_router` | 1/1 |
| Pre-flight | `cargo test -p cmhm_mcp --test phase5_preflight` | 2/2 |
| Rerank bench | `cargo test -p cmhm_core --features onnx --test phase5_rerank_bench -- --ignored --nocapture` | Stage 1 top-25 ≥ 0.85, Stage 2 Recall@5 ≥ 0.85, no degradation; mean/max rerank latency reported (target < 25 ms mean) |
| Held-out Gate 3 | pre-registered run | record exact Recall@1/3/5; no post-hoc edits |

The rerank bench **has been executed**; Gate 3 is recorded below.

### Gate 3 record (2026-11-16, pre-registered run, no post-hoc edits)

Phase 3 held-out benchmark (32 fresh scenarios, 128 docs, hash-128d):

| Metric | Value | Gate | Verdict |
| --- | --- | --- | --- |
| Recall@1 | 0.6250 [0.4688, 0.7812] | ≥ 0.40 | PASS |
| Recall@5 | 1.0000 | ≥ 0.67 | PASS |
| MRR | 0.8125 [0.7344, 0.8906] | ≥ 0.40 | PASS |
| Contradiction@1 | 0.0000 | < 0.40 | PASS |
| Hard-neg occupancy@5 | 0.0000 | < 0.40 | PASS |
| Mean margin | 0.2812 | > 0.00 | PASS |

6/6 gates passed, 0 breaches. Phase 4 ONNX bench passed (1/1; MiniLM int8
single-query p50 3.47 ms, clean-corpus Recall@1 100%). Phase 5 rerank bench:
Stage 1 top-25 1.0000, Stage 1 R@1/3/5 0.8901/0.9890/1.0000, Stage 2
0.8901/0.9835/1.0000, latency mean 2797 ms / max 4612 ms. Regression:
`cargo test -p cmhm_core` all bins green (244 lib), `cargo test -p cmhm_mcp`
green (74 tool tests, pre-flight 2/2).

### Rerank bench result (2026-11-16)

Held-out tokio split, 182 rows, release profile, CPU (16 logical cores), int8 model:

| Metric | Stage 1 | Stage 2 |
| --- | --- | --- |
| top-25 recall | 1.0000 | — |
| Recall@1 | 0.8901 | 0.8901 |
| Recall@3 | 0.9890 | 0.9835 |
| Recall@5 | 1.0000 | 1.0000 |

Latency (int8, CPU): mean 2797 ms, max 4612 ms; the earlier debug/2-thread
run measured mean 4013 ms. The 25 ms target is **not met on CPU**. Isolated ORT
probes on the RTX 4060: int8/CUDA 813 ms (slow int8 kernels), fp32/CUDA 56.7 ms
for the 25-document batch and 60.7 ms for one pair, i.e. launch/memcpy-bound.
Getting under 25 ms needs an fp16 export plus graph capture, a smaller model,
or a relaxed target — an owner decision, not silently claimed.

Stage 2 is neutral on this bench (R@1 equal, R@3 -0.55 pt, R@5 equal); the
reranker does not pay for its latency here.

### Stage 2 disposition (2026-11-16)

Stage 2 is **disabled by default** and stays behind an explicit opt-in:

- `reranker_enabled = false` in `config.toml`; enabling it also requires
  `reranker_dir` pointing at the exported model directory.
- Default builds do not compile the reranker (`onnx` feature off), so the hot
  path is Stage 1 only: 3.47 ms p50 embedding and sub-5 ms retrieval on the
  benches above. Enabling it in a non-onnx build is a startup error, not a
  silent fallback.
- When enabled, `retrieval::retrieve` reranks the top-25 fused candidates
  before seed/chain selection, so the chain, seed, and candidate texts all
  reflect the reranked order. The reranked block carries calibrated
  probabilities; the untouched tail keeps Stage 1 scores.
- Rationale: Stage 2 was neutral on the held-out bench (R@1 equal, R@3
  -0.55 pt, R@5 equal) at 2,797 ms CPU / 56.7 ms CUDA fp32, so it does not earn
  its cost on the default path. It remains available for deployments that
  measure a gain.

The corpus is closed at **3,620 triplets** (train 3,170 / val 268 / test 182);
no further mining. Stage 1 already meets the recall target and more data is not
the bottleneck.

## Slice status

| Slice | Content | Status |
| --- | --- | --- |
| 1 | Typed experience, normalization, v7 migration, WAL 0x08 | done, tested |
| 2 | Query router, weighted Stage 1, pool 25, identifier boost | done, tested |
| 3 | Fine-tune + int8 ONNX export + Rust Stage 2 wiring | done; Stage 2 shipped opt-in (default off); bench PASS on recall, latency target unmet (see above) |
| 4 | `preflight_check` + `record_experience`, fail-closed paths | done, tested |
| 5 | Gate 3 rerun, metrics report, docs/archive hygiene | done (verification complete; commit pending owner) |

## Risks / open items

- Corpus is closed at 3,620 rows (below the original 5,000 target by decision);
  Stage 1 already exceeds the recall target, so more mining is not planned.
- The official overlap audit is O(candidates × references) in PowerShell and
  takes tens of minutes; it is the gate, so it is not shortcut.
- The running `cmhm` server is a pre-Phase-5 binary; the existing v6 store
  migrates on next start. Do not point a new binary at the live store without
  a backup.
- `cargo fmt` has a noisy pre-existing baseline; do not reformat globally.

## File map

- `cmhm_core/src/node.rs` — typed fields + trigger normalization.
- `cmhm_core/src/learn_ledger.rs`, `consolidation.rs` — typed writes.
- `cmhm_core/src/retrieval.rs` — router, weighted fusion, pool, Stage 2 hook.
- `cmhm_core/src/reranker.rs` — `Stage2Reranker`, `ExperienceReranker`.
- `cmhm_core/src/migration.rs`, `persistence.rs` — snapshot v7.
- `cmhm_core/src/engine.rs` — WAL `0x08`, record/get paths.
- `cmhm_core/src/context/runtime.rs` — typed runtime access.
- `cmhm_mcp/src/{schema,tools,lib}.rs` — tool contracts + dispatch.
- `tools/mine_git_triplets.py`, `tools/train_experience_reranker.py` — data/model.
- Tests: `phase5_experience.rs`, `phase5_query_router.rs`,
  `phase5_rerank_bench.rs`, `cmhm_mcp/tests/phase5_preflight.rs`.
