# CMHM Context OS Spec v1.0 — Build Status Map

Companion file, frozen-date snapshot: 2026-10-25.

`CMHM_Context_OS_Spec_v1.0.md` is a frozen specification and is unchanged.
This file maps its sections to the implementation so a reader can tell,
in one glance, what is running and what is deferred. The spec's own
Part 14 is the authoritative deferral list; this file adds code pointers
and where reality reduces scope.

Claims discipline that applies repo-wide: the system provides
"agent-assisted recall" — the runtime routes and retrieves; the agent
reads, selects, and decides. No autonomous-recall claim is made anywhere.

## Built and running (in the released portable build)

| Spec section | Status | Code |
|---|---|---|
| §2.2 / §6.1 Registry — metadata store, O(1) lookup | Built, plus hardening: corrupt-file quarantine, delete-transition validation, cross-instance reload-merge with sidecar lock | `cmhm_core/src/context/registry.rs` |
| §4 Lifecycle FSM (incl. `Recovering`) | Built; fires on unclean WAL at startup | `cmhm_core/src/context/fsm.rs`, `cmhm_core/src/engine.rs` |
| §5.3 v1 routing — weighted linear score (as v1 prescribes) | Built | `cmhm_core/src/context/router.rs`, `router_tests.rs` |
| §12.3 Route API | Built; exposed as MCP tool `context_route`; learn's routing tags feed it | `cmhm_mcp/src/tools.rs` |
| §6.1 Cache / RAM residency | Built; eviction is plain LRU | `cmhm_core/src/context/cache.rs` |
| §7.4 v1 isolation — writes serialised per context | Built (per-context writer lock; concurrent reads allowed) | `cmhm_core/src/context/runtime.rs`, `cmhm_core/tests/scope_lock.rs` |
| §8 WAL + snapshot + crash recovery | Built; WAL compacted at each clean startup | `cmhm_core/src/engine.rs` |
| §8 versioning + schema migrations | Built (schema chain through snapshot v6) | `cmhm_core/src/migration.rs` |
| §12.1 DeleteContext — cryptographic erasure | Built (key destruction; ciphertext unrecoverable by construction; reads fail closed after shred) | `cmhm_core/src/crypto.rs`, `persistence.rs`, `cmhm_core/tests/crypto_shred.rs` |
| §6.1 context types (Session/Project/Branch/Workspace/Global) | Built as registry metadata + MCP tool parameter | `registry.rs`, `cmhm_mcp/src/schema.rs` |
| Runtime plumbing behind the MCP tools | Built: `project_detect`, `context_route`, `context_list`, `context_delete`, `memory_learn(_text)`, `memory_retrieve(_text)`, `memory_list`, `memory_get`, `memory_stats` | `cmhm_mcp/src/tools.rs` |

## Partial — built in reduced form

| Spec item | Reality |
|---|---|
| §2.8 priority scheduler (5-weight evict-the-minimum) | Eviction is plain LRU; the weighted priority function is not implemented |
| §9 multi-agent scopes + §5.5 capabilities | Tenant id and caller role are recorded; the Private/Shared/Workspace/Global ACL and capability-bitset enforcement is not implemented |
| §6.1 per-context at-rest encryption (`EncryptionProvider`) | Scope-sealed crypto + crypto-shredding exist (`crypto.rs`); the pluggable per-context provider abstraction does not |
| §5.2 full Bayesian routing posterior | v1 linear scorer only — exactly what §5.3 prescribes for v1 |

## Not built — deferred (spec Part 14 is the source of truth)

- Context DAG hierarchy: parent links, `ForkContext` / `MergeContext` APIs (Part 3, §12.1)
- Cross-context ACID transactions (Part 7 — "defined now, implementation deferred to v2")
- Storage backends beyond the local filesystem (Part 6 tiers: SQLite/Postgres/object stores)
- Full point-in-time replay tool (§8.4 baseline is recovery replay only)
- OS-like preemptive scheduling; registry partitioning / consistent hashing
- The scale targets of Part 11 (million-context, RAM-tiered fleets) are design targets, not measurements

## Doc set status

| Document | Status |
|---|---|
| Memory Mathematics (companion: `MATHCONTEXT.MD`) | Frozen (v1.0) |
| CMHM Context OS Specification (`CMHM_Context_OS_Spec_v1.0.md`) | Frozen (v1.0) — unchanged by this file |
| CMHM Validation Protocol | Not yet written as a formal doc; pre-registered, per-run protocols in this repo (e.g. the B13 ship-default run) are the working precursor |
| Engine/engineering docs (`CONTEXT.MD`, `RUNTIME.MD`, `CLIENTRUNTIME.MD`) | Describe the shipped subsystems |

## Delivered-stack note

The released portable build runs the engine with the onnx/MiniLM embedder
(main exe; dependency-free hash-embedder build shipped as a fallback exe).
Retrieval quality numbers live in the README and are governed by the
pre-registration discipline; this file makes no performance claims.
