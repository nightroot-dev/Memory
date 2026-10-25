# CMHM Context OS Specification
## Version 1.0 — Formal Standalone Document

> **Status**: Specification (frozen alongside CMHM Memory Mathematics v1.0)
> **Companion documents**: CMHM Memory Mathematics Spec · CMHM Validation Protocol
> **Rule**: Every component has exactly one mathematical responsibility. If a component does two things, split it.

---

## Preamble

The CMHM Memory Engine solves *how* an agent remembers. It knows nothing about users, projects, sessions, or storage backends. That is intentional and must never change.

The **CMHM Context Runtime** (formerly "Context Manager") solves the orthogonal problem: *where* should memory be accessed, *for whom*, and *for how long*? It is the operating system that orchestrates the memory kernel.

These two subsystems interact through a clean API. Neither knows the internals of the other.

```
     Reasoning Layer          (What to think about — future)
            ▲
     Context Runtime          (Where to think — this document)
            ▲
     CMHM Memory Engine       (How to remember — frozen spec)
            ▲
     Storage Abstraction      (How to persist — pluggable backends)
```

**Layer responsibilities (the single-responsibility rule):**

| Layer | One Responsibility | Must NEVER do |
|---|---|---|
| **Reasoning Layer** | Decide what to think about | Store memory, manage storage |
| **Context Runtime** | Decide *where* to think | Perform reasoning, update causal equations |
| **CMHM Memory Engine** | Learn and retrieve causal memory | Route contexts, manage users |
| **Storage Layer** | Persist data safely | Understand memory semantics |

---

# Part 1 — Design Goals

These are non-negotiable. Every design decision in this document must satisfy all of them.

| # | Goal | Metric |
|---|---|---|
| G1 | **Isolated contexts** | Zero bytes of data cross context boundaries without an explicit, logged operation |
| G2 | **O(1) context lookup** | Registry lookup by ID is a hash table get — O(1) regardless of total context count |
| G3 | **Automatic load/unload** | No human intervention required; the scheduler manages RAM residency |
| G4 | **Crash recovery** | After any power loss, the system recovers to the last committed state by replaying the WAL |
| G5 | **Multi-user** | Millions of users, each with thousands of contexts, share one runtime without interference |
| G6 | **Multi-agent** | Multiple agents can operate in the same workspace with private and shared scopes |
| G7 | **Local-first** | Works fully offline on a single laptop; cloud sync is an optional storage backend |
| G8 | **Pluggable storage** | The Memory Engine and Registry never know whether storage is SQLite, Postgres, S3, or NAS |
| G9 | **Deterministic routing** | Given identical inputs and configuration, routing always produces the same Execution Plan |
| G10 | **Version-aware** | Every committed state has an immutable version number; rollback to any prior version is possible |
| G11 | **Event-sourced** | Current state is derived by replaying events from a snapshot; no destructive updates |
| G12 | **Privacy compliant** | Delete means cryptographic erasure + storage removal; audit log is maintained |
| G13 | **Millions of contexts** | 1 000 000 contexts, ≈100 MB each = 100 TB; only active contexts consume RAM |

---

# Part 2 — Mathematical Model

## 2.1 Context Space

Let $\mathcal{C}$ denote the set of all contexts:

$$\mathcal{C} = \{c_1, c_2, \ldots, c_n\}$$

The Context Runtime maintains $\mathcal{C}$ and is the sole authority for creating, transitioning, and destroying elements of it.

## 2.2 Context as a 5-Tuple

A context $c \in \mathcal{C}$ is defined as five **independent** objects. Keeping them independent prevents concept bleeding between concerns:

$$c = (I, \Lambda, \Sigma, \Omega, \Gamma)$$

| Component | Name | Contains |
|---|---|---|
| $I$ | **Identity** | UUID v7, owner, parent links, permissions (ACL) |
| $\Lambda$ | **Lifecycle** | Current FSM state ∈ {Creating, Active, Idle, Sleeping, Recovering, Archived, Deleted} |
| $\Sigma$ | **Semantic** | title, tags, embedding vector, aliases, routing metadata |
| $\Omega$ | **Operational** | RAM residency, WAL position, snapshot version, storage location, cache statistics |
| $\Gamma$ | **Graph attachment** | A pointer to the CMHM `CoreState` — not the graph itself |

**Critical**: $\Gamma$ is a pointer, never a copy. The Memory Engine owns the graph. The Context Runtime owns the pointer.

## 2.3 Memory Ownership

Every memory $m$ belongs to exactly one context:

$$m_i = (x_i, c_i) \quad \text{where } x_i = \text{content}, \; c_i = \text{owner context}$$

The ownership function $f: m \rightarrow c$ is injective. A memory cannot exist without a context:

$$\forall m, \; \exists! \; c \in \mathcal{C} : f(m) = c$$

## 2.4 Context State Machine

$$\Lambda(c) \in \{\text{Creating}, \text{Active}, \text{Idle}, \text{Sleeping}, \text{Recovering}, \text{Archived}, \text{Deleted}\}$$

State transition function:

$$\delta : \Lambda \times \text{Event} \rightarrow \Lambda$$

(Fully specified in Part 4.)

## 2.5 Memory Function

Each context owns a memory graph:

$$M : \mathcal{C} \rightarrow \mathcal{G}$$

where $\mathcal{G}$ is the CMHM causal graph. Graphs of distinct contexts never overlap:

$$c_i \neq c_j \implies M(c_i) \cap M(c_j) = \emptyset$$

## 2.6 Active Set (RAM Residency)

The set of contexts currently loaded in RAM:

$$\mathcal{R} = \{c \in \mathcal{C} \mid \Lambda(c) = \text{Active}\}$$

$|\mathcal{R}|$ is bounded by the RAM budget; the scheduler enforces this bound.

## 2.7 Context Selection Function

Given a query $q$ and session context $(u, s, w, t)$:

$$c^* = \arg\max_{c \in \mathcal{C}} \; P(c \mid q, u, s, w, t)$$

where:
- $q$ = query embedding
- $u$ = user ID
- $s$ = session ID
- $w$ = workspace ID
- $t$ = current timestamp

## 2.8 Context Priority Function

Used by the scheduler to determine RAM residency:

$$\text{Priority}(c, t) = w_1 \cdot \text{Recency}(c,t) + w_2 \cdot \text{Frequency}(c) + w_3 \cdot \text{Importance}(c) + w_4 \cdot \text{UserIntent}(c) + w_5 \cdot \text{SystemLoad}$$

with $\sum_i w_i = 1$.

The scheduler periodically solves:

$$c_{\text{evict}} = \arg\min_{c \in \mathcal{R}} \; \text{Priority}(c, t)$$

and evicts $c_{\text{evict}}$ when $|\mathcal{R}|$ exceeds the RAM budget.

## 2.9 Execution Mathematics

A request $R$ processed by the Context Runtime is a 4-tuple:

$$R = (Q, C, P, T)$$

| Field | Type | Meaning |
|---|---|---|
| $Q$ | query | The original agent query |
| $C$ | context set | The context(s) selected by the router |
| $P$ | execution plan | Ordered sequence of operations to fulfill $Q$ |
| $T$ | tool requirements | Resources the plan needs (read, write, fork, ...) |

**The Memory Engine only ever receives $(P, C)$, never $(Q, T)$ directly.** This keeps the engine agnostic to routing decisions.

## 2.10 Context Lifetime Function

A context's expected lifetime informs archival policy:

$$\tau(c) = \text{CreationTime}(c) + \text{LifetimePolicy}(\text{type}(c))$$

where `type` ∈ {Session, Project, Workspace, Global} with default lifetimes Session < Project < Workspace < Global.

---

# Part 3 — Context Hierarchy

## 3.1 Why Not a Tree

A strict tree fails when shared research exists:

```
Company
├── AI Research
│   └── CMHM
│       ├── Experiment A
│       └── Experiment B ──────────┐
│                                  │ (shared contribution)
└── Video AI ◄─────────────────────┘
```

A tree cannot represent this. Experiment B cannot have two parents.

## 3.2 Directed Acyclic Graph (DAG)

Contexts form a DAG $\mathcal{H} = (\mathcal{C}, \mathcal{E})$ where:

- Vertices: contexts
- Edges: directed relationships typed as one of:

| Edge Type | Meaning |
|---|---|
| `DerivedFrom` | Child forked from parent; inherits parent's memory at fork time |
| `DependsOn` | Child reads from parent but cannot write to it |
| `SharedWith` | Bidirectional; both contexts can read a shared sub-graph |
| `ForkedFrom` | Snapshot fork; parent unchanged after fork |
| `MergedInto` | Marks the merge target; source is sealed after merge |

**Cycle invariant**: $\mathcal{H}$ must remain a DAG at all times. Any operation that would create a cycle is rejected with error `E_CYCLE`.

## 3.3 Context Types and Hierarchy

```
Organisation
│
├── Workspace
│   │
│   ├── Project
│   │   │
│   │   ├── Branch
│   │   │   │
│   │   │   └── Session
│   │   │       │
│   │   │       └── Task
│   │   │
│   │   └── Branch ...
│   │
│   └── Project ...
│
└── Workspace ...
```

Each level has its own memory scope. A query at the Session level searches only the Session; a query at the Project level searches Project + optionally its Branches.

## 3.4 Recursive Context Definition

$$c = (M(c), \{c_i : c_i \text{ is a child of } c\})$$

A context contains both its own memory graph and pointers to child contexts. The router can traverse this recursively.

## 3.5 Cross-Project Routing

For cross-context queries, the router computes:

$$\text{TopK} = \underset{c \in \mathcal{C}}{\operatorname{argTopK}} \; P(c \mid q, u, s, w, t)$$

and produces a multi-context Execution Plan:

$$P = \left[\text{Retrieve}(c_1), \text{Retrieve}(c_2), \text{Merge}(r_1, r_2)\right]$$

This avoids loading all contexts — only the top-K are opened.

---

# Part 4 — Context Lifecycle (Finite State Machine)

## 4.1 States

| State | Meaning |
|---|---|
| `Creating` | Allocation in progress; not yet accessible |
| `Active` | Loaded in RAM; fully operational |
| `Idle` | In RAM but no recent requests; still fast |
| `Sleeping` | Checkpointed to storage; evicted from RAM |
| `Recovering` | After crash; WAL is being replayed |
| `Archived` | Read-only; moved to cold storage |
| `Deleted` | Cryptographically erased; entry removed from Registry |

## 4.2 Transition Table

| From | Event | Precondition | To | Postcondition | On Failure |
|---|---|---|---|---|---|
| — | `Create(id, config)` | ID unique in Registry | `Creating` | Manifest written | `E_DUPLICATE_ID` |
| `Creating` | `InitComplete` | Storage allocated, WAL initialised | `Active` | $\Gamma$ attached | `E_STORAGE_FAIL` → retry or `Deleted` |
| `Active` | `NoRequestFor(τ_idle)` | — | `Idle` | — | — |
| `Idle` | `Request` | — | `Active` | — | — |
| `Idle` | `NoRequestFor(τ_sleep)` | Dirty pages = 0 or checkpoint succeeds | `Sleeping` | Snapshot written, RAM freed | `E_CHECKPOINT_FAIL` → stay `Idle` |
| `Sleeping` | `Request` | Storage reachable | `Active` | Snapshot loaded, $\Gamma$ attached | `E_LOAD_FAIL` → `Recovering` |
| `Sleeping` | `PowerLoss` | — | `Recovering` | — | — |
| `Recovering` | `WALReplayComplete` | Consistency check passes | `Active` | — | `E_CORRUPT` → `Archived` with error flag |
| `Active` | `Archive(c)` | All children archived or detached | `Archived` | Moved to cold storage, read-only flag set | `E_HAS_CHILDREN` |
| `Archived` | `Request(read)` | — | `Active` | Loaded read-only | `E_NOT_FOUND` |
| `Active` | `Delete(c)` | No pending transactions; children detached | `Deleted` | Key destroyed, storage wiped, audit log written | `E_HAS_ACTIVE_TX` |
| `Archived` | `Delete(c)` | — | `Deleted` | Key destroyed, storage wiped, audit log written | — |

**No state can transition to `Deleted` while active transactions exist.**
**`Deleted` is a terminal state — no transitions out of it.**

## 4.3 Timer Defaults (Configurable)

| Timer | Default |
|---|---|
| $\tau_{\text{idle}}$ | 5 minutes of no requests |
| $\tau_{\text{sleep}}$ | 30 minutes in Idle |
| $\tau_{\text{archive}}$ | 90 days of no access |

---

# Part 5 — Routing

## 5.1 Routing as a Planning Problem

The router does not simply "choose a context." It produces a **Context Execution Plan** — an ordered, inspectable sequence of operations.

```
Query: "How did we solve the gradient instability?"

Execution Plan:
  1. Open Workspace(ws_ai_company)
  2. Score contexts: P(c | q, u, s, w, t)
  3. Select: [CMHM: 0.94, Math: 0.81, Video: 0.03]
  4. Load CMHM context → Retrieve(c_cmhm, q)
  5. Load Math context → Retrieve(c_math, q)
  6. Merge results by salience
  7. Return unified answer
```

Every step is logged. Plans are deterministic: the same inputs produce the same plan (G9).

## 5.2 Full Routing Posterior

$$P(c \mid q, u, s, w, t) \propto P(q \mid \Sigma_c) \cdot P(c \mid u) \cdot P(c \mid s) \cdot P(c \mid w) \cdot P(c \mid t)$$

Decomposed components:

| Factor | Meaning | Computation |
|---|---|---|
| $P(q \mid \Sigma_c)$ | Semantic match | cosine similarity between query embedding and $\Sigma_c$ embedding |
| $P(c \mid u)$ | User affinity | frequency of user $u$ accessing context $c$ |
| $P(c \mid s)$ | Session prior | context active in current session gets a prior boost |
| $P(c \mid w)$ | Workspace filter | contexts outside the workspace get prior 0 unless cross-workspace is explicitly requested |
| $P(c \mid t)$ | Temporal recency | exponential decay: $e^{-\lambda (t - t_{\text{last access}})}$ |

## 5.3 Context Score Function (v1 Implementation)

Until a full Bayesian model is trained, v1 uses a weighted linear score:

$$\text{score}(c) = \alpha S_q + \beta S_h + \gamma S_t + \delta S_a$$

where:
- $S_q$ = cosine similarity of query to $\Sigma_c$ embedding
- $S_h$ = normalised access frequency (history)
- $S_t$ = normalised recency score
- $S_a$ = 1 if $c$ is currently Active, else 0
- $\alpha + \beta + \gamma + \delta = 1$, defaults: $0.5, 0.2, 0.2, 0.1$

The scoring interface accepts these weights as configuration so the model can be upgraded to a Bayesian posterior without changing the API.

## 5.4 Capability Check

Before executing any plan step, the runtime checks:

$$\text{CapabilityAllowed}(c, \text{op}) = \text{op} \in \text{Capabilities}(c)$$

If the capability is not present, the operation is rejected with `E_CAPABILITY_DENIED` before the Memory Engine is ever called.

## 5.5 Context Capabilities

Every context declares the set of permitted operations:

```
Capabilities = {Read, Write, Fork, Merge, Archive, Delete, Share, Train}
```

| Capability | Meaning |
|---|---|
| `Read` | Any agent can retrieve from this context |
| `Write` | Authorised agents can store into this context |
| `Fork` | A new derived context can be created from this one |
| `Merge` | This context can receive a merge from a derived context |
| `Archive` | This context can transition to Archived |
| `Delete` | This context can be permanently deleted |
| `Share` | This context's descriptor can be discovered by other workspaces |
| `Train` | The Memory Engine may run consolidation and skill compilation |

## 5.6 Context Descriptor

The descriptor is an **immutable, lightweight, safe-to-pass** summary. No operational data is exposed:

```
ContextDescriptor {
    context_id:      UUIDv7
    display_name:    String
    context_type:    Enum(Session, Project, Branch, Workspace, Global)
    owner:           UserID
    parent_ids:      Vec<UUIDv7>       // DAG edges
    capabilities:    BitSet
    current_state:   LifecycleState
    current_version: u64
    created_at:      Timestamp
    last_accessed:   Timestamp
}
```

The full `(I, \Lambda, \Sigma, \Omega, \Gamma)` manifest lives inside the runtime; only the Descriptor is shared with external callers.

---

# Part 6 — Storage Model

## 6.1 Separation of Concerns

Four distinct layers, each with one responsibility:

```
┌──────────────────────────────────────────────────────────┐
│                     Context Runtime                      │
│                                                          │
│  ┌────────────┐  ┌─────────────┐  ┌──────────────────┐ │
│  │  Registry  │  │   Locator   │  │      Cache       │ │
│  │ (metadata) │  │  (address)  │  │  (RAM residency) │ │
│  └─────┬──────┘  └──────┬──────┘  └────────┬─────────┘ │
│        │                │                   │           │
│        └────────────────┼───────────────────┘           │
│                         │                               │
│                  ┌──────▼──────┐                        │
│                  │  Storage    │                        │
│                  │  Provider   │                        │
│                  └──────┬──────┘                        │
│                         │                               │
│              ┌──────────▼──────────┐                    │
│              │  CMHM Memory Engine │                    │
│              └─────────────────────┘                    │
└──────────────────────────────────────────────────────────┘
```

### Registry

Stores metadata only. Never stores graph data.

```
Registry {
    context_id: UUIDv7          (primary key, O(1) lookup)
    owner:      UserID
    parent_ids: Vec<UUIDv7>
    state:      LifecycleState
    version:    u64
    created_at: Timestamp
    updated_at: Timestamp
    tags:       Vec<String>
    alias:      Option<String>
}
```

The Registry itself is persisted in a small, embedded key-value store (e.g. sled or SQLite) that is always loaded at startup.

### Locator

Maps a context ID to its physical storage coordinates. Decoupled from Registry so migrating from laptop to cloud only changes the Locator:

```
Locator {
    context_id:   UUIDv7
    backend_type: Enum(Local, SQLite, RocksDB, Postgres, S3, FoundationDB, ScyllaDB)
    connection:   String         (path, DSN, or URI)
    encryption:   EncryptionMeta
}
```

### Cache

LRU cache keyed by context ID. When a context is loaded, its `CoreState` lives here. The Cache implements the Priority eviction policy (§2.8).

### Storage Backends (Pluggable)

The `ContextStore` trait is the only interface the runtime uses:

```rust
trait ContextStore: Send + Sync {
    fn load_snapshot(&self, id: ContextId) -> Result<CoreState>;
    fn save_snapshot(&self, id: ContextId, state: &CoreState) -> Result<()>;
    fn append_wal(&self, id: ContextId, entry: WalEntry) -> Result<()>;
    fn read_wal(&self, id: ContextId, from: WalPosition) -> Result<Vec<WalEntry>>;
    fn delete_all(&self, id: ContextId) -> Result<()>;
}
```

**Supported backends (by deployment tier):**

| Tier | Backend |
|---|---|
| Local / single-agent | Local filesystem (binary snapshots + WAL files) |
| Small team | SQLite |
| Medium production | RocksDB |
| Large production | PostgreSQL |
| Distributed | FoundationDB, ScyllaDB, CockroachDB, TiKV |

The CMHM engine never knows which backend is in use.

---

# Part 7 — Transactions

## 7.1 Why Transactions Matter

Without transactions, a compound operation (e.g., Store + UpdateEdge + UpdateSalience) can partially succeed. After a crash, the context state is inconsistent. Transactions make the entire compound operation atomic.

## 7.2 Transaction API (Defined Now, Implementation Deferred to v2)

```
BeginTx(context_id) → TxHandle

CommitTx(tx: TxHandle) → Result<Version>

RollbackTx(tx: TxHandle) → ()
```

Every write operation (Store, UpdateEdge, UpdateSalience, Consolidate) must be wrapped in a transaction.

## 7.3 Transaction Semantics

```
BEGIN

  Store(c, m1)        ← appended to WAL, not yet committed
  UpdateEdge(c, e1)   ← appended to WAL
  UpdateSalience(c)   ← appended to WAL

COMMIT

  WAL fsynced
  Version n+1 written to Registry
  In-memory state updated atomically
```

On crash between BEGIN and COMMIT: WAL entries are discarded on recovery (they never reached the committed version).

## 7.4 Isolation Level (v1)

Serialisable isolation per context. Two transactions on the same context cannot execute concurrently. Cross-context transactions are not supported in v1 (deferred to v2 with two-phase commit).

---

# Part 8 — Versioning and Event Sourcing

## 8.1 Immutable Event Stream

Every write to a context is an event appended to the WAL. The current state is always derivable by replaying events from the last snapshot:

```
Event Stream:  [e1, e2, e3, e4, ...]
Snapshot:      S_k  (state after replaying e1..ek)
Current State: replay(S_k, e_{k+1}, ..., e_n)
```

## 8.2 Version Numbers

Every committed transaction increments the context version:

$$v_{n+1} = v_n + 1$$

Version numbers are **immutable once committed** (Invariant 4). They are stored in the Registry.

## 8.3 Snapshot Policy

A new snapshot is written when:
- Version count since last snapshot > `snapshot_interval` (default: 1000 events)
- Context transitions from Active → Sleeping
- Explicit `Snapshot(context_id)` API call

## 8.4 Point-in-Time Recovery

To recover to version $k$:

```
1. Load snapshot S_j where j ≤ k
2. Replay WAL events e_{j+1} ... e_k
3. Stop. Return state at version k.
```

This enables "undo yesterday" and "compare last week's memory to today."

## 8.5 Algorithm Migration

When CMHM v2 changes the salience equation:

```
1. Keep the raw event stream (it records inputs, not derived state)
2. Create new context at version 0
3. Replay all events through the v2 engine
4. New CoreState reflects v2 semantics — no data loss
```

No migration scripts. No downtime. The event stream is the source of truth.

---

# Part 9 — Multi-Agent Model

## 9.1 The Four Memory Scopes

Agents must not accidentally interfere with each other. The runtime defines four scopes with explicit access rules:

| Scope | Visibility | Write Access |
|---|---|---|
| **Private** | Only the owning agent | Only the owning agent |
| **Shared** | Named agents or roles | Named agents with `Write` capability |
| **Workspace** | All agents in the workspace | All agents with `Write` capability |
| **Global** | Read-only; cross-organisation | No one (append-only by system) |

## 9.2 Memory Topology for Multi-Agent Workspace

```
Workspace Context (Shared)
│
├── Agent A Private Context
├── Agent B Private Context
├── Agent C Private Context
│
└── Shared Sub-Context (readable by A, B, C; writable by A and B)
```

An agent reading a Workspace context sees: its own Private memories + Shared memories + Workspace memories. It never sees another agent's Private context.

## 9.3 Concurrent Writes

When two agents write to the same Shared context simultaneously:

- The runtime serialises writes at the transaction level (§7.4).
- Each write is a separate transaction.
- No merge conflict at the CMHM level — the causal graph is additive; concurrent node/edge additions are always safe.
- Concurrent Beta updates to the same edge are serialised; last writer wins within a transaction.

## 9.4 Agent Identity

Every API call includes an `AgentDescriptor`:

```
AgentDescriptor {
    agent_id:    UUIDv7
    agent_type:  String     ("host-a", "host-b", "custom", ...)
    workspace:   UUIDv7
    session:     UUIDv7
    permissions: BitSet
}
```

The runtime uses this for capability checks and routing decisions. It is logged with every write for auditability.

---

# Part 10 — Failure Modes

Every failure mode has a defined expected behaviour. "Undefined" is not acceptable.

| Failure | Detection | Expected Behaviour |
|---|---|---|
| **Storage unavailable** | Write/read timeout | Buffer in WAL; retry with exponential backoff; surface `E_STORAGE_UNAVAILABLE` after N retries |
| **Registry corruption** | Checksum mismatch on load | Load last valid registry snapshot; replay registry WAL; if unrecoverable, enter read-only mode |
| **Power loss during checkpoint** | Dirty WAL on startup | Transition to `Recovering`; replay WAL from last committed snapshot; consistency check; transition to `Active` |
| **Partial WAL replay** | WAL entry checksum mismatch | Stop replay at last valid entry; surface `E_WAL_CORRUPT`; flag context version as `uncertain` |
| **Context deleted while in use** | Request arrives after `Delete` | Return `E_CONTEXT_DELETED`; caller must handle and create a new context if needed |
| **Two agents writing simultaneously** | Lock contention | Serialise transactions (§7.4); no silent data loss |
| **Cache eviction during retrieval** | Context moved to `Sleeping` mid-request | Reload from storage transparently; request latency increases but correctness is preserved |
| **Version mismatch after upgrade** | Schema version in manifest ≠ engine version | Run migration if upgrader exists; else enter `Recovering` with `E_VERSION_MISMATCH` |
| **DAG cycle creation attempt** | Parent chain traversal | Reject with `E_CYCLE` before any write |
| **Out-of-RAM** | Cache budget exceeded | Evict lowest-Priority context; if no evictable context exists (all Active with pending tx), return `E_OUT_OF_MEMORY` |

---

# Part 11 — Scalability

## 11.1 Context Count Model

A production deployment at scale:

```
10 000   Users
× 100    Projects per user          = 1 000 000 Projects
× 10     Sessions per project day   = 10 000 000 Sessions
×  5     Tasks per session          = 50 000 000 Task contexts
```

Not all contexts are active simultaneously. The runtime handles this through scheduling (§2.8).

## 11.2 RAM Budget

| Tier | Contexts in RAM | RAM Budget | Context RAM Size |
|---|---|---|---|
| Laptop (dev) | 2–5 | 1 GB | ~200 MB each |
| Server (medium) | 100–500 | 32 GB | ~64 MB each |
| Server (large) | 1 000–5 000 | 128 GB | ~25 MB each |

RAM budget is configurable. The scheduler enforces it.

## 11.3 Storage Tiers

```
HOT   — RAM Cache      : ≤500 active contexts, sub-millisecond access
WARM  — SSD            : sleeping contexts, snapshot files, WAL; ~1-10 ms load time
COLD  — HDD / Object   : archived contexts; minutes to load
DEEP  — Cloud / Tape   : long-term compliance storage; retrieved on demand only
```

## 11.4 Registry Size

The Registry stores only metadata. At 1 000 000 contexts × ~512 bytes each = ~512 MB. This fits in RAM on any server. For 100 000 000 contexts, the Registry is partitioned (consistent hashing) across nodes.

## 11.5 Bottleneck Analysis

| Operation | Bottleneck | Mitigation |
|---|---|---|
| Context lookup by ID | None — O(1) hash map | — |
| Routing score computation | Linear in #contexts scored | Score only workspace-filtered subset |
| Snapshot load (cold) | Disk I/O | Prefetch on session open signal |
| Concurrent writes | Lock contention per context | Serialised per-context; no global lock |
| Registry startup | Full load into RAM | Lazy-load from storage; build hot index first |

---

# Part 12 — API Specification

## 12.1 Context Lifecycle API

### `CreateContext`
```
Input:
  owner:       UserID
  parent_ids:  Vec<ContextId>     (optional; empty = root context)
  config:      ContextConfig      (type, capabilities, lifetime policy)

Output:
  descriptor:  ContextDescriptor

Guarantees:
  - UUID assigned; O(1) lookup available immediately
  - Storage allocated atomically; if storage fails, context is not registered

Errors:
  E_DUPLICATE_ID       (should never occur with UUID v7; log and retry)
  E_STORAGE_FAIL       (backend unavailable)
  E_CYCLE              (parent_ids would create a cycle in the DAG)
  E_QUOTA_EXCEEDED     (user context count limit reached)
```

### `OpenContext`
```
Input:
  context_id:  ContextId
  agent:       AgentDescriptor

Output:
  descriptor:  ContextDescriptor

Guarantees:
  - If Sleeping: loaded from storage into RAM; state → Active
  - If Active/Idle: state → Active; no-op on storage
  - Capability check: agent must have at least Read

Errors:
  E_NOT_FOUND
  E_CONTEXT_DELETED
  E_CAPABILITY_DENIED
  E_LOAD_FAIL           → context enters Recovering automatically
```

### `CloseContext`
```
Input:
  context_id:  ContextId

Output:
  version:     u64        (version after checkpoint)

Guarantees:
  - Dirty state is checkpointed before RAM is freed
  - State → Sleeping

Errors:
  E_PENDING_TRANSACTIONS  (cannot close with uncommitted tx)
  E_CHECKPOINT_FAIL       → retried; context stays Idle
```

### `DeleteContext`
```
Input:
  context_id:  ContextId
  agent:       AgentDescriptor

Output:
  audit_id:    AuditLogId

Guarantees:
  - Encryption key destroyed
  - All storage (WAL, snapshots, node store, edge store, dense core) deleted
  - Registry entry removed
  - Audit log entry written and immutable
  - Irreversible

Errors:
  E_HAS_ACTIVE_TRANSACTIONS
  E_HAS_ACTIVE_CHILDREN      (must delete or detach children first)
  E_CAPABILITY_DENIED
```

### `ArchiveContext`
```
Input:
  context_id:  ContextId

Output:
  archive_uri: String     (where the archived data lives)

Guarantees:
  - Context becomes read-only
  - Moved to cold storage
  - State → Archived

Errors:
  E_HAS_CHILDREN    (children must be archived or detached first)
```

### `ForkContext`
```
Input:
  source_id:   ContextId
  config:      ContextConfig

Output:
  fork_descriptor: ContextDescriptor

Guarantees:
  - New context created with a snapshot of source at current version
  - DAG edge ForkFrom(new → source) added
  - Source context is unmodified
  - Capability: source must have Fork

Errors:
  E_CAPABILITY_DENIED
  E_CYCLE
```

### `MergeContext`
```
Input:
  source_id:   ContextId    (the fork to merge back)
  target_id:   ContextId    (the parent to receive changes)

Output:
  merge_version: u64

Guarantees:
  - Delta since fork (new nodes/edges learned in source) is applied to target
  - Source is sealed (read-only) after merge
  - DAG edge MergedInto(source → target) added
  - Capability: target must have Merge; source must have Write

Errors:
  E_CONFLICT     (if target received incompatible writes; resolution is manual)
  E_CAPABILITY_DENIED
```

## 12.2 Memory Operations API

### `Store`
```
Input:
  context_id:       ContextId
  agent:            AgentDescriptor
  state_embedding:  [f32; 128]
  action_embedding: [f32; 128]
  outcome_embedding:[f32; 128]
  prediction_error: f32            ∈ [0.0, 1.0]
  emotional_tag:    f32            ∈ [0.0, 1.0]
  model_id:         String

Output:
  node_ids: (StateId, ActionId, OutcomeId)
  version:  u64

Guarantees:
  - Atomic; wrapped in an implicit transaction
  - WAL entry written before in-memory update
  - Capability: Write required

Errors:
  E_CAPABILITY_DENIED
  E_CONTEXT_NOT_ACTIVE
  E_EMBEDDING_DIMENSION_MISMATCH
```

### `Retrieve`
```
Input:
  context_id:      ContextId
  agent:           AgentDescriptor
  query_embedding: [f32; 128]
  model_id:        String
  top_k:           u32             (default: 5)

Output:
  results: Vec<RetrievalResult>
    RetrievalResult {
      path:        Vec<(NodeId, EdgeId)>
      explanation: String
      confidence:  f32
      salience:    f32
    }

Guarantees:
  - Thompson-sampled Dijkstra search (see Memory Mathematics spec)
  - Does not modify context state
  - Capability: Read required

Errors:
  E_CAPABILITY_DENIED
  E_CONTEXT_NOT_ACTIVE
```

## 12.3 Infrastructure API

### `Route`
```
Input:
  agent:       AgentDescriptor
  query:       String
  query_emb:   [f32; 128]
  scope:       Enum(Session, Project, Workspace, CrossWorkspace)

Output:
  plan:        ExecutionPlan
    ExecutionPlan {
      steps:    Vec<PlanStep>
      contexts: Vec<ContextDescriptor>
      estimated_latency_ms: u32
    }

Guarantees:
  - Deterministic given identical inputs + config (G9)
  - Capability check performed; forbidden contexts excluded from plan
  - No memory engine calls; purely metadata operations

Errors:
  E_NO_MATCHING_CONTEXT
```

### `Checkpoint`
```
Input:
  context_id: ContextId

Output:
  version:    u64
  size_bytes: u64

Guarantees:
  - Dirty in-memory state flushed to storage
  - WAL truncated at checkpoint point
  - Does NOT evict context from RAM
```

### `Snapshot`
```
Input:
  context_id: ContextId

Output:
  snapshot_id: SnapshotId
  version:     u64

Guarantees:
  - Point-in-time consistent snapshot written to storage
  - Used as the base for future WAL replay
```

### `Statistics`
```
Input:
  context_id: ContextId

Output:
  stats: ContextStats {
    node_count:       u64
    edge_count:       u64
    dense_core_bytes: u64
    wal_entries:      u64
    version:          u64
    last_accessed:    Timestamp
    ram_bytes:        u64
    snapshot_count:   u64
  }
```

### `BeginTx / CommitTx / RollbackTx`
_(See Part 7)_

---

# Part 13 — Formal Invariants

These are machine-testable guarantees the Context Runtime must uphold at all times. Any state that violates an invariant is a bug.

| # | Invariant | Test |
|---|---|---|
| **I1** | A memory belongs to exactly one context | `∀m: |{c : m ∈ M(c)}| = 1` |
| **I2** | An Active context has a valid Graph attachment | `Λ(c)=Active ⟹ Γ(c) ≠ null` |
| **I3** | A Deleted context never transitions to Active | `Λ(c)=Deleted ⟹ δ(c, any_event) = Deleted` |
| **I4** | A context version is immutable once committed | `v_n committed ⟹ v_n never changes` |
| **I5** | A WAL entry is replayed at most once | `replay_count(e) ≤ 1 ∀e` |
| **I6** | Routing is deterministic | `Route(i, cfg) = Route(i, cfg)` for identical inputs |
| **I7** | The context DAG is acyclic | `∀c: c ∉ ancestors(c)` |
| **I8** | No data crosses context boundaries silently | `Read(c_a, key) never returns data from c_b (b≠a) without explicit plan step` |

---

# Part 14 — What is Deferred to v2

These features are **architecturally anticipated** — the schema and interfaces are designed to support them, but the implementation is not required in v1.

| Feature | v1 Status | v2 Path |
|---|---|---|
| Full Bayesian routing | Weighted linear score | Upgrade `ContextScorer` trait implementation |
| Cross-context ACID transactions | Not supported | Two-phase commit across context stores |
| Merge/Fork algorithms | API defined; merge logic minimal | Full delta-CRDT merge of CMHM graphs |
| OS-like scheduling | LRU + Priority score | Full preemptive scheduler |
| Per-context encryption | Schema ready | Implement `EncryptionProvider` trait |
| Event sourcing with full replay | WAL replay for recovery only | Full point-in-time replay tool |
| Context Graph traversal for routing | Single-context routing | Multi-hop DAG traversal in Execution Plans |
| Multi-agent conflict resolution | Last-write-wins per tx | MVCC with agent-level vector clocks |
| Registry partitioning | Single-node | Consistent hashing across registry shards |

---

# Part 15 — Three-Spec Framework

This document is Specification 2 of 3. The three documents must be developed and maintained independently:

| Spec | Status | Scope |
|---|---|---|
| **CMHM Memory Mathematics** | ✅ Frozen | Beta-Bernoulli edges, Thompson-sampled Dijkstra, Welford salience, delta-rule dense core, Wilson-score causal filter, consolidation |
| **CMHM Context OS Specification** (this document) | ✅ v1.0 | Context Runtime: routing, lifecycle, storage, transactions, versioning, multi-agent, failure modes, API |
| **CMHM Validation Protocol** | 🔲 To be written | The five experiments, metrics, ground-truth generation, baselines, measurement methodology |

**The memory mathematics must remain stable while the Context Runtime evolves. The validation protocol must remain fixed so improvements are measured consistently.**

---

# Appendix A — Glossary

| Term | Definition |
|---|---|
| **Context** | An isolated unit of memory, identified by UUID, owning exactly one CMHM graph |
| **Context Runtime** | The OS-like subsystem that routes, schedules, loads, unloads, and manages lifecycle of contexts |
| **Registry** | The metadata store; maps context IDs to manifests |
| **Locator** | Maps context IDs to physical storage addresses |
| **Execution Plan** | An ordered sequence of operations produced by the Router for a given query |
| **Context Descriptor** | An immutable, lightweight, shareable summary of a context |
| **WAL** | Write-Ahead Log; the durability mechanism; events are appended before in-memory updates |
| **Snapshot** | A serialised point-in-time copy of a `CoreState`; base for WAL replay |
| **Capability** | A declared permission on a context (Read, Write, Fork, Merge, ...) |
| **DAG** | Directed Acyclic Graph; the structure of context relationships |
| **Recovering** | A lifecycle state entered after crash; WAL replay proceeds within this state |
| **Event Sourcing** | Architecture where current state is derived from a replay of all events, not destructive updates |
| **Thompson Sampling** | Sampling from Beta(α,β) to produce exploration-exploiting edge weights in retrieval |
| **CoreState** | The in-memory representation of a CMHM memory graph (NodeStore, EdgeStore, DenseCore, etc.) |

---

*CMHM Context OS Specification v1.0*
*Frozen alongside CMHM Memory Mathematics v1.0*
*Next: CMHM Validation Protocol*
