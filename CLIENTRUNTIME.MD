
---

# CMHM — User Flow, Integration & Runtime

## 1. User Flow (How a Person Experiences CMHM)

CMHM is **never visible** to the end user. It is an internal memory brain for the AI agent. The user interacts with their AI tool normally (CLI, IDE, chatbot), and the tool becomes progressively smarter over time because of the memory. There is no “wrapper” that modifies the user’s input; the agent itself manages all interactions with CMHM.

### 1.1 The Retrieve–Act–Learn Cycle

Every agent–user conversation follows three phases. The user sees only the final response.

**Phase 1 — Retrieve**  
When the user makes a request, the AI agent (before calling the LLM) sends a query embedding to CMHM.  
- *How it’s sent*: either via an **MCP tool call** (the agent calls `memory_retrieve`) or by an **internal function call** in the agent’s code.  
- *What comes back*: a structured causal chain — what happened in similar past situations, which actions worked, which failed, and confidence levels.

**Phase 2 — Reason & Act**  
The agent uses the retrieved causal context to plan its response. It may also consult other tools (like RAG for documents). The agent then calls the LLM, **constructing the prompt entirely within the agent’s own logic** — CMHM never injects text into the prompt.

**Phase 3 — Learn**  
After the action is taken and the outcome is observed (success, failure, error magnitude), the agent sends a learning signal to CMHM: `(state_embedding, action_embedding, outcome_embedding, prediction_error)`. CMHM updates its causal graph and becomes smarter for the next cycle.

The user simply types `"Fix the billing bug"` and receives a better answer because the agent now remembers that “quick refunds caused anger, but immediate escalation resolved the issue quickly.” There is **no prompt wrapper, no text injection, no modification of the user’s original message**.

---

## 2. Integration Methods

CMHM provides exactly two integration paths. Both are completely transparent to the user.

### 2.1 MCP (Model Context Protocol) — For Existing Tools

CMHM ships as a lightweight MCP server binary. Any MCP‑compatible host (Claude Code, Cursor, Zed, etc.) can launch this binary as a tool provider. The host sees two tools:

- `memory_retrieve`
- `memory_learn`

The AI model itself decides *when* to call these tools. The user’s message is never altered; the model simply receives the tool’s result (a causal chain) in its context, exactly like any other tool output.

#### MCP Server Architecture

```
┌─────────────────────┐
│  AI Host (Claude)   │
│  launches cmhm-mcp   │
└────────┬────────────┘
         │ JSON‑RPC over stdio / TCP
         ▼
┌─────────────────────┐
│    cmhm-mcp          │
│  (thin adapter)      │
│  translates MCP      │
│  calls to core API   │
└────────┬────────────┘
         │ internal channel
         ▼
┌─────────────────────┐
│   CMHM Core Engine   │
│  (single-threaded)   │
│  node/edge/graph     │
└─────────────────────┘
```

The `cmhm-mcp` binary is a thin layer; all memory logic stays inside the core engine. The MCP server runs in the same process as the core engine (or as a sidecar), making latencies sub‑millisecond.

**Key guarantee:** The AI host never modifies the user’s prompt. It merely passes tool outputs back to the model as structured context.

### 2.2 Embedded Agent Loop — For Custom Agents

When you build your own agent (in Rust, Python, etc.), you embed CMHM directly as a library. The agent’s main loop explicitly calls the memory engine:

```python
# Pseudocode for a custom agent loop
while True:
    user_msg = input("You: ")
    # 1. Embed user message (using any model)
    q_emb = embed(user_msg)
    # 2. Retrieve causal context
    chain = memory.retrieve(q_emb, model="model-a")
    # 3. Build prompt (NOT by CMHM – by the agent)
    prompt = build_prompt(user_msg, chain, other_tools)
    # 4. Get LLM response
    response = llm(prompt)
    print("Agent:", response)
    # 5. After outcome is known (e.g., from user feedback or task completion)
    outcome_emb = embed(outcome_text)
    memory.learn(state_emb=q_emb, action_emb=action_emb, outcome_emb=outcome_emb, error=0.1, model="model-a")
```

CMHM’s Rust library exposes a simple API (C FFI, Python bindings via PyO3). The library creates a single‑threaded worker in the background, and the agent talks to it via a channel. The memory engine never touches the prompt; the agent developer is fully responsible for building the final LLM call.

---

## 3. Runtime System — Deep Dive

The CMHM runtime is a **single‑threaded, event‑driven** memory processor. It is completely self‑contained, does not touch the network (unless explicitly configured), and stores all state in RAM with optional disk persistence.

### 3.1 Core Architecture

```
┌────────────────────────────────────────────┐
│              CMHM RUNTIME                   │
│                                             │
│  ┌───────────┐         ┌─────────────────┐ │
│  │  MCP /    │         │  WORKER THREAD  │ │
│  │  Embedded │────────▶│  (single)        │ │
│  │  API      │  MPSC   │                 │ │
│  └───────────┘  queue  │  processes all   │ │
│                         │  commands        │ │
│                         └───────┬─────────┘ │
│                                 │            │
│              ┌──────────────────┘            │
│              ▼                               │
│  ┌─────────────────────────┐                 │
│  │      CORE STATE          │                 │
│  │  • Node Store (HashMap)  │                 │
│  │  • Edge Store (adj lists)│                 │
│  │  • Dense Core (tensor)   │                 │
│  │  • Cohort Centroids      │                 │
│  │  • Salience Tracker      │                 │
│  │  • Episode Log (vec)     │                 │
│  └────────────┬────────────┘                 │
│               │                               │
│  ┌────────────▼────────────┐                 │
│  │   PERSISTENCE (optional) │                 │
│  │   • Write‑Ahead Log     │                 │
│  │   • Periodic Snapshots  │                 │
│  └─────────────────────────┘                 │
└────────────────────────────────────────────┘
```

All client interactions (MCP calls, embedded API calls) are converted into `Command` messages and pushed into a **multi‑producer, single‑consumer (MPSC) queue**. The single worker thread pops commands and executes them sequentially. This eliminates any need for locks or atomics on the core state.

### 3.2 Command Loop

The worker thread runs an infinite loop:

1. Receive a `Command` from the MPSC channel.
2. Match on the command variant:
   - `Retrieve` → run retrieval algorithm (read‑only), send the result back via a `oneshot` channel.
   - `Learn` → create nodes/edges if needed, apply edge life update, record episode log, update dense core, send acknowledgment.
   - `ExplicitEdge` → create explicit edges (from structured feedback).
   - `Snapshot` → serialize state to disk.
   - `Shutdown` → break loop and stop.
3. After each command, check consolidation triggers (time, node count, cumulative reinforcement). If any trigger is met, run the full consolidation sequence inline (this blocks the worker thread, but it’s designed to complete in <100 ms for typical single‑agent graphs).

**Key property:** The graph state is mutated only by this one thread, so there are no concurrent modifications, no deadlocks, and no data corruption.

### 3.3 Persistence (Write‑Ahead Log & Snapshots)

Every mutation command (`Learn`, `ExplicitEdge`) is first serialised and appended to a Write‑Ahead Log (WAL) file. The WAL entry format:

```
[length: u32 LE] [command_code: u8] [payload: bytes] [CRC32: u32 LE]
```

After the WAL write is fsynced, the worker applies the mutation in‑memory. This ensures durability even if the process crashes immediately after.

Periodically (every 1,000 commands or every 5 minutes), the worker writes a full snapshot of the entire `CoreState` to a separate file. After the snapshot is safely written, the WAL is truncated to the point of that snapshot.

**Crash recovery:**
1. Load the latest snapshot.
2. Read the WAL from the snapshot’s sequence number onward.
3. Replay all remaining commands.
4. Corrupted WAL entries are skipped; the system will lose only those entries, but the graph remains consistent.

### 3.4 Embedding & Projection Pipeline

The CMHM runtime **does not embed text**. It expects clients to provide pre‑computed 128‑dimensional float32 embeddings. Before any embedding is stored or queried, the worker applies a cross‑model projection (if a projection matrix for that model exists) using a simple linear layer:

$$ \mathbf{e}_{\text{proj}} = \operatorname{normalize}(\mathbf{P}_m \mathbf{e}_{\text{raw}} + \mathbf{b}_m) $$

The projection matrices are loaded from disk at startup (one file per model). If no projection is found for a given model ID, the raw embedding is assumed to already be in the shared space.

### 3.5 Consolidation Scheduling

Consolidation is not a separate daemon; it runs on the worker thread **after any command** if one of three triggers fires:

- Time since last consolidation > 24 h (configurable).
- Total node count exceeds 8,000.
- Cumulative absolute reinforcement (`sum |Δ|`) since last consolidation exceeds 50.

Consolidation executes atomically: centroids update, episode log propagation, superstition filter (Wilson score causal test), edge pruning, node culling with bridging, duplicate merging, and skill compilation. Because the worker thread is the only mutator, no other operations can interfere during consolidation.

### 3.6 Performance & Resource Usage

Designed for a laptop‑scale single agent:
- **Node insertion**: ~5 µs
- **Edge update**: ~1 µs
- **Retrieval (3‑hop)**: <50 µs
- **Full consolidation (100k nodes)**: <100 ms
- **Memory resident**: ~80 MB
- **CPU usage**: <0.1% of one core under typical agent interaction rates
- **Disk usage**: WAL + snapshot ~10 MB for a mature graph

### 3.7 Deployment Forms

CMHM can be compiled and used as:
- **Standalone MCP binary**: `cmhm-mcp` (bundled with the core engine) that speaks MCP over `stdin/stdout` or TCP. Configuration via a TOML file.
- **Rust library crate**: `cmhm_core` that exposes an `Engine` struct with `retrieve()` and `learn()` methods, and a background worker thread. Usable from other Rust binaries.
- **Python wheel** (via PyO3 / maturin): `import cmhm; engine = cmhm.Engine(config_path)` exposing the same API for Python agent loops.

All three forms share exactly the same core state and worker thread implementation.

---

