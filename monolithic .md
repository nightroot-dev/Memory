# Causal‑Matrix Hybrid Memory (CMHM) — Complete Specification v3.1

**Final Monolithic Document**  
*Handoff‑ready for engineering implementation*

---

## Table of Contents

1. Introduction & Design Philosophy
2. Mathematical Reference
3. Core State & Data Structures
4. Subsystem Specifications (Node, Edge, Salience, Birth, etc.)
5. Dense Core & Retrieval Engine
6. Cross‑Model Projection
7. Consolidation Engine
8. Concurrency, Persistence & Crash Recovery
9. User Flow & Integration
10. Runtime System Specification
11. Failure Scenarios & Resilience
12. Performance Budget & Configuration
13. Appendix: Derivations & Proofs

---

## 1. Introduction & Design Philosophy

The Causal‑Matrix Hybrid Memory (CMHM) is a single‑user, CPU‑resident, continuously learning memory substrate for AI agents. It maintains a typed causal graph (State, Action, Outcome) and a dense tensor core, enabling agents to recall what worked, what failed, and why.

**Principles**  
- **Online learning** – every action and outcome updates the model in O(1) time.  
- **Bayesian uncertainty** – edge strengths are Beta distributions, giving confidence for free.  
- **Intervention‑based causality** – agent‑chosen actions are treated as interventions, enabling do‑calculus‑style causal contrast.  
- **Self‑calibration** – salience, surprise, and thresholds are derived from running statistics.  
- **Zero‑prompt‑wrapper** – memory is never injected into the user’s prompt; it is a native tool (MCP) or embedded module.

**What CMHM is not**  
- Not a vector database – it stores causal structure, not raw text.  
- Not a RAG wrapper – it does not intercept or modify prompts.  
- Not a document store – for static knowledge bases, use a separate RAG component.

---

## 2. Mathematical Reference

### 2.1 Core Probabilistic Edge Model

Every directed edge is modelled as a Beta distribution. The edge weight and confidence are derived from pseudo‑counts $`(\alpha,\beta)`$.

**Mean weight (used for retrieval)**  
$$ \mu = \frac{\alpha}{\alpha+\beta} $$

**Variance (uncertainty)**  
$$ \sigma^2 = \frac{\alpha\beta}{(\alpha+\beta)^2(\alpha+\beta+1)} $$

**Thompson sampling draw**  
$$ \tilde{w} \sim \text{Beta}(\alpha,\beta) $$

### 2.2 Salience – Self‑Calibrating Surprise

Salience measures importance relative to the agent’s own history. We use Welford’s online algorithm to maintain running statistics of prediction error $`\epsilon`$.

**Welford update**  
$$ \delta_1 = \epsilon - \bar{\epsilon} $$
$$ \bar{\epsilon} \leftarrow \bar{\epsilon} + \frac{\delta_1}{N}, \quad N \leftarrow N+1 $$
$$ \delta_2 = \epsilon - \bar{\epsilon} $$
$$ M_2 \leftarrow M_2 + \delta_1 \cdot \delta_2 $$
$$ \sigma_\epsilon = \sqrt{\frac{M_2}{N-1}} \quad (N\ge 2) $$

**Z‑score**  
$$ z = \frac{\epsilon - \bar{\epsilon}}{\sigma_\epsilon} $$
(For $`N < 100`$, fallback: $`z = 2(\epsilon-0.5)`$.)

**Salience**  
$$ s = \Phi(z) = \frac{1}{2}\left[1 + \operatorname{erf}\!\left(\frac{z}{\sqrt{2}}\right)\right] $$
Clamped to $`[0.01, 0.99]`$. If $`\epsilon > 0.9`$, boost by 1.5× (max 1.0).

### 2.3 Node Birth

Given normalised embedding $`\mathbf{e}`$, error $`\epsilon`$, emotion $`\eta`$:

**Maximum cosine similarity**  
$$ c_{\max} = \max_i \cos(\mathbf{e}, \mathbf{e}_i) $$

**Birth condition** (any true → create node)  
$$ (c_{\max} < 0.55) \lor (s > 0.70) \lor (\epsilon > 0.5) $$

**Embedding blend**  
$$ \mathbf{\mu} \leftarrow 0.1\,\mathbf{e} + 0.9\,\mathbf{\mu} $$
$$ \mathbf{e}_{\text{new}} = \frac{0.7\,\mathbf{e} + 0.3\,\mathbf{\mu}}{\|0.7\,\mathbf{e} + 0.3\,\mathbf{\mu}\|} $$

### 2.4 Edge Birth

For a new node $`j`$ and each $`i`$ in working memory:

**Temporal decay**  
$$ \phi_{ij} = \exp(-\lambda_t |t_j - t_i|), \quad \lambda_t = 0.0001\,\text{s}^{-1} $$

**Directed similarity**  
$$ \text{sim}_{ij} = \max(0, \cos(\mathbf{e}_i, \mathbf{e}_j)) \cdot \phi_{ij} $$

If $`\text{sim}_{ij} > 0.35`$, create edge with $`\alpha=1, \beta=1`$.

**Explicit (structured) edges** from host:
- State→Action: Sequence, Exact, $`\alpha=5,\beta=1`$ (success) or $`1,5`$ (failure).
- Action→Outcome: CausalHypothesis, Exact, Intervention, same counts.

### 2.5 Edge Life Update (Reinforcement & Forgetting)

For a CausalHypothesis edge with outcome error $`\epsilon`$ and outcome salience $`s_k`$:

**Forgetting**  
$$ \alpha \leftarrow \alpha \cdot e^{-\lambda_c \Delta t}, \quad \beta \leftarrow \beta \cdot e^{-\lambda_c \Delta t} $$
$`\lambda_c = 0.1\,\text{hr}^{-1}`$, clamped to min 0.01.

**Base increment**  
$$ \Delta = s_k $$

**Learning rate multiplier**  
$$ lr = 1.0 \times (4 \text{ if is\_intervention}) \times (4 \text{ if Evidence=Exact}), \quad lr_{\min}=0.1 $$

**Update**  
- If $`\epsilon \le 0.7`$ (success): $`\alpha \leftarrow \alpha + \Delta \cdot lr`$
- Else (failure): $`\beta \leftarrow \beta + \Delta \cdot lr`$

**Episode log entry**  
If the preceding edge was State→Action, store $`(s,a,\Delta\cdot lr, \text{was\_success})`$.

### 2.6 Credit Propagation

During consolidation, for each log entry $`(s,a,\delta,\text{success})`$:
- Locate Sequence edge $`s\to a`$.
- If success: $`\alpha_{s,a} \leftarrow \alpha_{s,a} + \eta_{\text{prop}} \cdot \delta`$
- If failure: $`\beta_{s,a} \leftarrow \beta_{s,a} + \eta_{\text{prop}} \cdot \delta`$
with $`\eta_{\text{prop}} = 0.1`$.

### 2.7 Superstition Filter (Causal Contrast)

**Cohort assignment** – 512 centroids, soft k‑means with momentum 0.8, $`\beta=5`$.

**Intervention edge counting** – accumulate $`\alpha,\beta`$ into `succ[a][b]` and `fail[a][b]` for intervention edges only.

**Average Treatment Effect (ATE)**  
$$ p_{\text{treated}} = \frac{\text{succ}[a][b]}{\text{succ}[a][b]+\text{fail}[a][b]} $$
$$ p_{\text{control}} = \frac{\sum_{c\neq a}\text{succ}[c][b]}{\sum_{c\neq a}(\text{succ}[c][b]+\text{fail}[c][b])} $$
$$ \Delta = p_{\text{treated}} - p_{\text{control}} $$

**Wilson score CI for Δ** – Newcombe’s method, 95% confidence. Prune if upper bound ≤ 0. Skip if treated sample < 5.

### 2.8 Dense Core

8 matrices $`C^{(k)} \in \mathbb{R}^{128\times128}`$. Slice $`k = \text{hash}(\nu_i,\nu_j,\text{edge\_type}) \bmod 8`$.

**Delta rule update**  
$$ C^{(k)} \leftarrow C^{(k)}(I - \eta_c \mathbf{e}_i \mathbf{e}_i^\top) + \eta_c\, w\, (\mathbf{e}_j \otimes \mathbf{e}_i) $$
$`\eta_c = 0.01`$.

**Dense retrieval**  
$$ \mathbf{c} = \frac{1}{8}\sum_{k=0}^{7} C^{(k)}\mathbf{q} $$

### 2.9 Retrieval

**Seed selection** – mixed score $`0.7\cos(\mathbf{q},\mathbf{e}_i) + 0.3\cos(\mathbf{c},\mathbf{e}_i)`$.

**Thompson‑sampled optimal path** – edge cost $`-\log(\tilde{w}+\varepsilon)`$, Dijkstra bounded depth 3.

### 2.10 Cross‑Model Projection

**Training** – linear least squares $`\min \|\mathbf{P}_m\mathbf{E}_m+\mathbf{b}_m - \mathbf{E}_{\text{ref}}\|^2`$.

**Runtime projection** – $`\mathbf{e}_{\text{proj}} = \operatorname{normalize}(\mathbf{P}_m\mathbf{e}_{\text{raw}} + \mathbf{b}_m)`$.

---

## 3. Core State & Data Structures

```rust
struct CoreState {
    // Node Store
    nodes: SlotMap<u64, Node>,
    node_index: Box<dyn NearestNeighborIndex>,
    
    // Edge Store
    out_edges: HashMap<u64, Vec<(u64, Edge)>>,
    in_edges: HashMap<u64, Vec<(u64, Edge)>>,
    
    // Working Memory (capacity 5)
    working_memory: VecDeque<u64>,
    
    // Episode Log
    episode_log: Vec<EpisodeLogEntry>,
    
    // Salience
    salience_tracker: SalienceTracker,
    
    // Dense Core
    dense_core: Box<[f32; 128*128*8]>,
    
    // Centroids
    centroids: Box<[[f32; 128]; 512]>,
    
    // Bookkeeping
    last_consolidation: Instant,
    cumulative_reinforce: f32,
    cmd_seq: u64,
    
    // Persistence
    wal: Option<WalWriter>,
    
    // Projections
    projections: HashMap<String, Projection>,
}
```

### 3.1 Node Layout (272 bytes aligned)
```
Embedding [f16; 128]  → 256 B
Salience f16          →   2 B
Timestamp i64         →   8 B
EmotionalTag f16      →   2 B
Type u8               →   1 B
_padding              →   3 B
```

### 3.2 Edge Layout (16 bytes packed)
```
Alpha f32, Beta f32, LastTraversalTime f32, type+flags u8
```

---

## 4. Subsystem Specifications

### 4.1 Node Store
- **Create**: normalise, compute salience, check birth condition, blend, push to working memory, update index.
- **Delete**: only during consolidation; removes all incident edges.
- **Nearest neighbor**: default brute‑force SIMD up to 200k nodes; pluggable HNSW/LSH trait.

### 4.2 Edge Store
- **Create**: for each working memory neighbour, compute similarity; if above threshold, instantiate Beta(1,1).
- **Explicit**: host provides state/action/outcome IDs; edges created with Exact evidence.
- **Delete**: during consolidation; remove from adjacency lists; dense core removal via weight=0 update.

### 4.3 Working Memory Buffer
- `VecDeque<u64>`, capacity 5. Push evicts oldest if full.

### 4.4 Episode Log
- `Vec<(state_id, action_id, delta, was_success)>`. Drained during consolidation.

### 4.5 Salience Engine
- Welford accumulators. Fallback heuristic for first 100 samples.

### 4.6 Node Birth Algorithm (exact)
1. Normalise raw embedding.
2. Compute salience via Salience Engine.
3. Query nearest node; get `cos_max`.
4. If birth condition met: create node, blend embedding, push to working memory, trigger edge birth, update index.

### 4.7 Edge Birth Algorithm (exact)
For each `i` in working memory (excl. new node):
- Compute temporal decay and cosine similarity.
- If >0.35, create edge with Beta(1,1), type based on node types.

### 4.8 Edge Life Update Algorithm
1. Apply forgetting to alpha/beta.
2. Determine success/failure from `epsilon` threshold.
3. Compute `increment = salience * lr`.
4. Update counts.
5. Log episode if previous edge was State→Action.

### 4.9 Credit Propagation Algorithm
Drain episode log. For each entry, locate Sequence edge state→action; apply `η_prop * delta` to alpha or beta.

### 4.10 Superstition Filter Algorithm
1. Update centroids (mini‑batch k‑means, momentum 0.8).
2. Compute hard labels for all nodes.
3. Build intervention‑only count matrix `succ[512][512]`, `fail[512][512]`.
4. For each intervention edge, compute ATE and Wilson CI; prune if upper bound ≤ 0.

### 4.11 Edge & Node Death
- **Edge death**: if mean weight < 0.04 OR (unused >7 days AND evidence < 3). Dense core removal.
- **Node death**: if `salience * degree < 0.12` AND age >14 days. Bridge top‑3 in/out edges.

### 4.12 Skill Compilation
Detect high‑confidence S→A→O chains (evidence >50, success >0.8). Create Skill node.

---

## 5. Dense Core & Retrieval Engine

### 5.1 Dense Core Update
Delta rule applied to the appropriate slice on every edge weight change or deletion.

### 5.2 Retrieval Flow
1. Apply cross‑model projection to query.
2. Compute dense context vector.
3. Seed selection with mixed score.
4. Thompson‑sampled Dijkstra (depth 3).
5. Path assembly with confidence and evidence.

---

## 6. Cross‑Model Projection

- **Training**: offline linear least squares on aligned embedding pairs.
- **Storage**: per‑model `.bin` file with matrix (128×128) and bias (128).
- **Runtime**: normalised linear projection before all storage/retrieval.

---

## 7. Consolidation Engine

### 7.1 Triggers
- Time > 24 h
- Nodes > 8,000
- Cumulative |Δ_reinforce| > 50

### 7.2 Procedure (atomic)
1. Update centroids.
2. Propagate episode log.
3. Superstition filter.
4. Prune dead edges.
5. Cull dead nodes & bridge.
6. Merge near‑identical nodes.
7. Skill compilation.

---

## 8. Concurrency, Persistence & Crash Recovery

### 8.1 Concurrency Model
Single‑threaded worker loop, MPSC command queue from clients. All mutations sequential; retrievals also dispatched through queue (simplicity, sufficient for low QPS). No locks on core state.

### 8.2 Write‑Ahead Log
Every mutation appended to binary WAL with CRC32. Fsynced for durability (configurable).

### 8.3 Snapshot
Periodic full‑state serialisation (bincode). WAL truncated after successful snapshot.

### 8.4 Crash Recovery
Load snapshot → replay WAL from snapshot’s sequence number. Skip corrupted entries.

---

## 9. User Flow & Integration

### 9.1 The Retrieve–Act–Learn Cycle
1. **Retrieve**: agent queries CMHM before LLM call.
2. **Act**: agent uses causal context in its reasoning.
3. **Learn**: after outcome, agent sends feedback to CMHM.

**No prompt modification by CMHM.**

### 9.2 Integration Methods

| Method | Mechanism | User Visible? |
|--------|-----------|---------------|
| **MCP tool** | CMHM runs as native tool server; LLM calls `memory_retrieve`/`memory_learn` | Invisible |
| **Embedded agent loop** | CMHM library linked into agent process; explicit API calls | Invisible (developer‑side) |

There is no third wrapper option.

### 9.3 MCP Server Architecture
Standalone `cmhm-mcp` binary. Thin adapter that translates MCP JSON‑RPC calls to core engine commands. Host launches binary; model decides when to call tools.

---

## 10. Runtime System Specification

### 10.1 Thread Model
One worker thread. MPSC channel from client handlers. Sequential command processing.

### 10.2 Command Types
`Retrieve`, `Learn`, `ExplicitEdge`, `Snapshot`, `Shutdown`.

### 10.3 Core Loop
```rust
loop {
    cmd = rx.recv();
    process(cmd);
    if should_consolidate() { consolidate(); }
}
```

### 10.4 Embedding & Projection Pipeline
No text embedding inside CMHM. Clients supply 128‑dim f32 arrays. Projection applied if model‑specific matrix exists.

### 10.5 Error Handling
- NaN embedding: reject command.
- OOM: panic (host restarts from snapshot).
- Disk full: error returned, command not applied, host alerted.

### 10.6 Configuration (TOML)
All thresholds and rates configurable at startup.

---

## 11. Failure Scenarios & Resilience

| Scenario | Detection | Response |
|----------|-----------|----------|
| NaN embedding | `is_nan()` check | Reject command |
| Edge count overflow | `alpha+beta > 1e6` | Rescale both by 0.5 |
| WAL corruption | CRC mismatch | Stop replay at entry, continue |
| Crash during consolidation | WAL incomplete | Revert to pre‑consolidation snapshot, replay, retry |
| Projection file missing | Startup check | Use identity, log warning |
| Cohort underpopulation | <5 intervention edges | Skip filter for that edge |

---

## 12. Performance Budget & Configuration

**Targets** (100k nodes, 500k edges)
- Node creation: <10 µs
- Edge creation: <5 µs per edge
- Retrieval: <100 µs p99
- Consolidation: <50 ms
- Memory: ~80 MB

**Tuning knobs** (configurable)
- Working memory size (default 5)
- Similarity threshold (0.35)
- Novelty threshold (0.55)
- Forgetting rate λ_c (0.1 hr⁻¹)
- Consolidation interval / triggers
- WAL sync mode

---

## 13. Appendix: Derivations & Proofs

- **Beta distribution properties**: mean, variance, sample.  
- **Welford’s algorithm**: standard proof of correctness.  
- **Wilson score interval**: Newcombe (1998).  
- **Thompson sampling optimality**: regret bounds for Bernoulli bandits.  
- **Delta rule as fast weight**: equivalence to linear attention mechanisms.

---

**End of CMHM Complete Specification v3.1**  
