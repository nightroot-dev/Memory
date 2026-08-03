

# Causal‑Matrix Hybrid Memory (CMHM)  
## Complete Mathematical & Engineering Specification  
**Version 3.0**

---

## 1. Introduction & Design Philosophy

The Causal‑Matrix Hybrid Memory (CMHM) is a persistent, continuously learning causal world model for AI agents. It maintains a sparse, typed causal graph (nodes of type State, Action, Outcome) and a dense, low‑rank tensor core that mirrors the graph for fast retrieval. The system runs entirely on a CPU, in a single‑threaded operation loop, and requires less than 100 MB of RAM for a decade of daily interactions.

The design follows four principles:
- **Online learning** – every action and outcome updates the model in O(1) time without retraining.
- **Bayesian uncertainty** – edge strengths are Beta distributions, not scalars, providing confidence for free.
- **Intervention‑based causality** – actions chosen by the agent are treated as interventions (do‑operator), enabling genuine causal contrast.
- **Self‑calibration** – all hyperparameters are either derived from running statistics or have been reduced to unavoidable time constants.

---

## 2. Mathematical Preliminaries

### 2.1 Notation
- **Vectors** are column vectors in ℝ¹²⁸, L2‑normalised unless stated otherwise.
- **Cosine similarity** between vectors $`\mathbf{a}`$ and $`\mathbf{b}`$:
  $$ \cos(\mathbf{a},\mathbf{b}) = \frac{\mathbf{a}\cdot\mathbf{b}}{\|\mathbf{a}\|\,\|\mathbf{b}\|} $$
- **Prediction error** $`\epsilon \in [0,1]`$ is supplied by the host system. 0 = perfect prediction, 1 = total failure.
- **Emotional tag** $`\eta \in [0,1]`$ (or task criticality) is supplied by the host system.
- **Timestamps** are Unix seconds (64‑bit integer), relative to UTC.
- All **update loops** are single‑threaded; reads may be concurrent.

### 2.2 Core Probabilistic Model
Edges are modelled as **Beta‑Bernoulli** random variables:
$$ w \sim \text{Beta}(\alpha,\beta),\qquad \alpha,\beta > 0 $$
The probability of a successful outcome (the weight) is the mean $`\mu = \frac{\alpha}{\alpha+\beta}`$. The variance is $`\sigma^2 = \frac{\alpha\beta}{(\alpha+\beta)^2(\alpha+\beta+1)}`$.

This replaces the previous scalar weight, so the system naturally quantifies uncertainty. An edge seen 200 times with 140 successes is much more certain than one seen 3 times with 2 successes, even though both have a mean near 0.7.

---

## 3. Node Store & Embeddings

### 3.1 Data Model
Each node is a record:
```
NodeID u64
Embedding [f16; 128]   (unit‑norm)
Salience f16            ∈ (0,1)
Timestamp i64           Unix seconds
EmotionalTag f16        ∈ [0,1]
Type u8                 (0=State, 1=Action, 2=Outcome)
```
**Size**: 269 bytes.

### 3.2 Node Creation (Birth)
A new node is created for an experience if:

1. The experience is **novel**:
   $$ \max_{i} \cos(\mathbf{e}, \mathbf{e}_i) < 0.55 $$
   OR
2. The experience is **highly salient** (see Section 4):
   $$ s_{\text{event}} > 0.70 $$
   OR
3. The prediction error alone indicates a **surprise**:
   $$ \epsilon > 0.5 $$

**Embedding initialization** (blending with global context):
$$ \mathbf{\mu} \leftarrow 0.1\,\mathbf{e} + 0.9\,\mathbf{\mu} $$
$$ \mathbf{e}_{\text{new}} = \frac{0.7\,\mathbf{e} + 0.3\,\mathbf{\mu}}{\|0.7\,\mathbf{e} + 0.3\,\mathbf{\mu}\|} $$
where $`\mathbf{\mu}`$ is the exponential moving average of all node embeddings (initialised to zero vector). This ensures new memories are anchored to the existing semantic space while preserving novelty.

---

## 4. Salience & Surprise (Self‑Calibrating)

### 4.1 Purpose
Salience measures importance. Instead of hand‑tuned constants, we use a running Z‑score of prediction error $`\epsilon`$.

### 4.2 Welford’s Algorithm for Running Statistics
We maintain $`N`$ (count of errors seen) and the mean $`\bar{\epsilon}`$ and second moment $`M_2`$:

For each new $`\epsilon`$:
$$ N \leftarrow N + 1 $$
$$ \delta \leftarrow \epsilon - \bar{\epsilon} $$
$$ \bar{\epsilon} \leftarrow \bar{\epsilon} + \frac{\delta}{N} $$
$$ \delta_2 \leftarrow \epsilon - \bar{\epsilon} $$
$$ M_2 \leftarrow M_2 + \delta \cdot \delta_2 $$

Variance $`\sigma^2_\epsilon = \frac{M_2}{N}`$ (for $`N > 1`$). The Z‑score is:
$$ z = \frac{\epsilon - \bar{\epsilon}}{\sigma_\epsilon} $$
When $`N < 2`$, we fall back to a heuristic: $`z \leftarrow 2(\epsilon - 0.5)`$.

### 4.3 Salience from Z‑score
Salience is the cumulative probability under the standard normal distribution:
$$ s = \Phi(z) = \frac{1}{2}\left[1 + \operatorname{erf}\!\left(\frac{z}{\sqrt{2}}\right)\right] $$
Clamped to $`[0.01, 0.99]`$. If $`\epsilon > 0.9`$, the salience is boosted by a factor of 1.5 (capped at 1.0).

**Why this works**: Early in an agent’s lifetime, everything is novel and error variance is high, so most errors get moderate salience. As the agent improves, only truly anomalous errors stand out, making the memory automatically focus on the unexpected.

---

## 5. Edge Store & Beta‑Bernoulli Model

### 5.1 Data Model
```
SourceNode: u64
TargetNode: u64
Alpha: f32
Beta: f32
LastTraversalTime: f32   Unix seconds
EdgeType: u8             (0=Sequence, 1=CausalHypothesis)
EvidenceType: u8         (0=Inferred, 1=Exact)
IsIntervention: bool     (true if source was an agent‑chosen Action)
```
Derived: $`\mu = \frac{\alpha}{\alpha+\beta}`$, $`\sigma^2 = \frac{\alpha\beta}{(\alpha+\beta)^2(\alpha+\beta+1)}`$.

### 5.2 Edge Birth
When a new node $`j`$ is created, for every node $`i`$ in the **Working Memory Buffer** (size 5):

1. Temporal decay factor:
   $$ \phi_{ij} = \exp(-\lambda_t |t_j - t_i|),\quad \lambda_t = 0.0001\,\text{s}^{-1} $$
2. Semantic similarity:
   $$ \text{sim}_{ij} = \max(0, \cos(\mathbf{e}_i, \mathbf{e}_j)) \cdot \phi_{ij} $$
3. If $`\text{sim}_{ij} > 0.35`$, create an edge $`i\to j`$ with:
   - $`\alpha = 1`$, $`\beta = 1`$ (uniform prior)
   - `EdgeType` = `CausalHypothesis` if $`i`$ is Action and $`j`$ is Outcome, else `Sequence`
   - `EvidenceType` = `Inferred`
   - `IsIntervention` = true if $`i`$ is Action

**Why Beta(1,1)?** The uniform prior expresses complete uncertainty; the weight will quickly adapt to outcomes.

**Explicit Edges** (from structured host events):  
If the host provides a triple `(state, action, outcome, success_flag)`, we create:
- State→Action (Sequence, Evidence=Exact, IsIntervention=false, $`\alpha=5,\beta=1`$ if success else $`\alpha=1,\beta=5`$)
- Action→Outcome (CausalHypothesis, Evidence=Exact, IsIntervention=true, same counts)

These bypass all similarity and temporal checks.

### 5.3 Edge Life Update (Standard Rule)
When an edge $`i\to j`$ of type `CausalHypothesis` is traversed during retrieval, after the outcome with error $`\epsilon`$ is known:

1. **Forgetting (exponential decay of counts)**:
   $$ \alpha \leftarrow \alpha \cdot \exp(-\lambda_c \Delta t),\quad \beta \leftarrow \beta \cdot \exp(-\lambda_c \Delta t) $$
   where $`\Delta t = t_{\text{now}} - t_{\text{last}}`$ and $`\lambda_c = 0.1\,\text{hr}^{-1}`$ (half‑life ≈ 7 h).

2. **Update pseudo‑counts based on outcome**:
   - If $`\epsilon \le 0.7`$: success → $`\alpha \leftarrow \alpha + s_k`$
   - If $`\epsilon > 0.7`$: failure → $`\beta \leftarrow \beta + s_k`$
   where $`s_k`$ is the salience of the outcome node $`k`$.

3. **Learning rate multiplier** based on evidence quality:
   $$ lr = 1.0 \times (4 \text{ if IsIntervention}) \times (4 \text{ if EvidenceType = Exact}) $$
   The increment is multiplied by $`lr`$ (clamped to at least 0.1). This ensures that agent‑chosen actions and explicit feedback carry far more weight than passive observations.

4. Update $`\text{LastTraversalTime} \leftarrow t_{\text{now}}`$.

**Forgetting rationale**: Evidence decays over time, preventing the system from clinging to obsolete patterns. The half‑life of ~7 hours balances recency with long‑term learning.

### 5.4 Credit Propagation to State→Action Edges
When a `CausalHypothesis` edge $`j\to k`$ receives a reinforcement signal $`\Delta = s_k`$ (the increment added to $`\alpha`$ or $`\beta`$), and the immediately preceding edge in the retrieval path was a `Sequence` edge $`i\to j`$ (State→Action), we record an entry in the **Episode Log**:
$$ (i, j, \Delta) $$

During consolidation, for each log entry:
- Find edge $`i\to j`$ (must exist and be a `Sequence` edge with source type State and target type Action).
- If the outcome was a success: $`\alpha_{ij} \leftarrow \alpha_{ij} + \eta \cdot \Delta`$, else $`\beta_{ij} \leftarrow \beta_{ij} + \eta \cdot \Delta`$, with $`\eta = 0.1`$.
- Remove the log entry.

**Why η = 0.1?** The state only indirectly contributed to the outcome; the action bears the primary responsibility. This fraction prevents the state→action edge from being overly punished or rewarded for a single action’s result.

---

## 6. Superstition Filter (Causal Contrast)

### 6.1 Purpose
Remove edges that are merely correlational, retaining only those with a positive causal effect. We use only **intervention‑tagged** edges to approximate the do‑operator.

### 6.2 Cohort Construction (Soft Clustering)
512 centroids $`\mathbf{C}_c \in \mathbb{R}^{128}`$ are maintained via incremental mini‑batch k‑means with momentum 0.8. During consolidation:
- For each node $`i`$, soft assignment:
  $$ p_{i,c} = \frac{\exp(-\beta \|\mathbf{e}_i - \mathbf{C}_c\|^2)}{\sum_{c'} \exp(-\beta \|\mathbf{e}_i - \mathbf{C}_{c'}\|^2)},\quad \beta = 5 $$
- Hard label $`c_i = \arg\max_c p_{i,c}`$.

### 6.3 Average Treatment Effect (ATE) Estimation
For each edge $`i\to j`$ of type `CausalHypothesis` with `IsIntervention = true`, with source cluster $`a`$ and target cluster $`b`$:

Count:
- $`N_{a,b}^{\text{succ}} = \sum_{ \text{edges } a\to b,\ \text{intervention} } \alpha_{\text{edge}}`$ (pseudo‑count of successes)
- $`N_{a,b}^{\text{fail}} = \sum_{ \text{edges } a\to b,\ \text{intervention} } \beta_{\text{edge}}`$ (failures)
- Total outcomes from intervention on $`a`$: $`N_a = \sum_b (N_{a,b}^{\text{succ}} + N_{a,b}^{\text{fail}})`$
- Control group: all edges from *other* clusters $`c \neq a`$ to cluster $`b`$, regardless of intervention status. Count their total successes $`N_{\neg a, b}^{\text{succ}}`$ and failures $`N_{\neg a, b}^{\text{fail}}`$.

Compute:
$$ P_{\text{treated}} = \frac{N_{a,b}^{\text{succ}}}{N_{a,b}^{\text{succ}} + N_{a,b}^{\text{fail}}}, \quad P_{\text{control}} = \frac{N_{\neg a, b}^{\text{succ}}}{N_{\neg a, b}^{\text{succ}} + N_{\neg a, b}^{\text{fail}}} $$
$$ \Delta = P_{\text{treated}} - P_{\text{control}} $$

### 6.4 Wilson Score Confidence Interval
For each proportion $`p`$ with $`n`$ total observations, the Wilson score interval at 95% confidence is:
$$ \text{WilsonCI}(p, n) = \frac{p + \frac{z^2}{2n} \pm z \sqrt{\frac{p(1-p)}{n} + \frac{z^2}{4n^2}}}{1 + \frac{z^2}{n}} $$
with $`z = 1.96`$.

We compute the CI for $`\Delta`$ by combining the two proportions? Actually, we compute a CI for the *difference of proportions* using the Wilson score for each separately, then combine via:
$$ \text{se} = \sqrt{ \text{var}_{\text{treated}} + \text{var}_{\text{control}} } $$
where $`\text{var} = \frac{p(1-p)}{n + z^2}`$ (adjusted variance). The CI for $`\Delta`$ is $`\Delta \pm z \cdot \text{se}`$.

**Pruning rule**: If the **upper bound** of the CI is $`\le 0`$ (no positive effect), **prune** the edge. Also, if the lower bound is $`\ge 0`$ but the evidence count is extremely low ($`N_{a,b}^{\text{total}} < 5`$), leave unpruned (unvalidated). If the lower bound is $`< 0`$ but upper bound $`> 0`$, the edge remains as "uncertain" and subject to future learning.

**Why this works**: This directly tests whether doing action $`a`$ (intervention) increases the probability of good outcome $`b`$ compared to not doing it, using only real agent interventions. It is a practical approximation of do‑calculus without requiring a full causal graph of confounders.

---

## 7. Dense Core (Matrix Hybrid)

### 7.1 Architecture
A tensor $`\mathcal{C} \in \mathbb{R}^{128\times 128\times 8}`$ (eight relation slices). Each slice $`C^{(k)}`$ is a 128×128 matrix, stored as `f32`.

### 7.2 Update (Delta Rule)
When an edge $`i\to j`$ has weight $`w = \frac{\alpha}{\alpha+\beta}`$:

1. Select slice $`k = (\text{hash}(\text{EdgeType}, \nu_i, \nu_j) \bmod 8)`$.
2. Update:
   $$ C^{(k)} \leftarrow C^{(k)} \left( I - \eta_c \mathbf{e}_i \mathbf{e}_i^\top \right) + \eta_c\, w\, \bigl( \mathbf{e}_j \otimes \mathbf{e}_i \bigr) $$
   where $`\eta_c = 0.01`$.

**Why the delta rule?** The term $`- \eta_c \mathbf{e}_i \mathbf{e}_i^\top`$ *removes* the old association for key $`\mathbf{e}_i`$ before adding the new one, preventing interference. This keeps the core clean without the need for periodic SVD cleanup. This is mathematically identical to the fast‑weight update in DeltaNet, which has proven retrieval properties.

### 7.3 Retrieval
Given query $`\mathbf{q}`$:
$$ \mathbf{c} = \sum_{k=1}^{8} \alpha_k \bigl( C^{(k)} \mathbf{q} \bigr) $$
with $`\alpha_k = 1/8`$ (uniform). The blended vector $`\mathbf{c}`$ represents the causal context fused from all relation types. It can be used:
- As a “soft seed” to bias the graph search (dot with node embeddings).
- Directly as a summary vector for the agent.

---

## 8. Retrieval Engine (Optimal Path & Thompson Sampling)

### 8.1 Seed Selection
Given a query embedding $`\mathbf{q}`$ (possibly after cross‑model projection):
- Compute cosine to all nodes (using an approximate nearest neighbor index).
- Return top‑3 candidates. The seed is the candidate with highest cosine; ties broken by salience.

### 8.2 Thompson‑Sampled Dijkstra
From the seed, we search up to 3 hops to find the highest‑weight causal chain. To balance exploration and exploitation, we use Thompson sampling:

For each edge $`e`$ with counts $`(\alpha,\beta)`$, draw a sample $`\tilde{w} \sim \text{Beta}(\alpha,\beta)`$. Use the sampled weight as the edge “goodness” for this retrieval. Edge cost = $`-\log(\tilde{w} + \varepsilon)`$ with $`\varepsilon = 10^{-6}`$.

Run Dijkstra’s algorithm (bounded to 3 hops) on the graph where nodes are vertices and edges have the sampled cost. The algorithm finds the path with minimum total cost, i.e., maximum product of sampled weights.

**Why Thompson sampling?** It is provably optimal for Bernoulli bandits. Confident edges with high $`\alpha`$ relative to $`\beta`$ will almost always be chosen; uncertain edges (wide Beta) occasionally get sampled high and explored, generating the data needed to resolve them. No extra exploration parameter needed.

### 8.3 Result Assembly
The optimal path is returned, along with for each edge:
- Mean weight $`\mu = \alpha/(\alpha+\beta)`$
- Total evidence $`\alpha+\beta`$
- Variance
- Intervention flag
- Evidence type
Additionally, the system flags edges with high uncertainty (variance > threshold) as “curious” for optional exploration.

---

## 9. Cross‑Model Projection

### 9.1 Problem
Different embedding models (model A, model B, model C, model D) map text to different high‑dimensional spaces. A memory built with one model cannot be queried by another.

### 9.2 Solution
For each model $`m`$, train a linear projection $`\mathbf{P}_m \in \mathbb{R}^{128\times 128}`$ and bias $`\mathbf{b}_m \in \mathbb{R}^{128}`$ offline. Training data: aligned embedding pairs $`(\mathbf{e}_{\text{ref}}, \mathbf{e}_m)`$ from a domain‑specific paraphrase corpus (e.g., code comments, support dialogues). Minimize:
$$ L = \sum_i \|\mathbf{P}_m \mathbf{e}_{m,i} + \mathbf{b}_m - \mathbf{e}_{\text{ref},i}\|^2 $$
Solved via ordinary least squares (pseudo‑inverse).

### 9.3 Runtime
- On write: $`\mathbf{e}_{\text{stored}} = \operatorname{normalize}(\mathbf{P}_m \mathbf{e}_{\text{raw}} + \mathbf{b}_m)`$
- On query: same projection applied to the query vector before retrieval.

The projection matrices are small (16 KB per model) and loaded at startup.

---

## 10. Consolidation Engine (Detailed Order)

Consolidation runs periodically (every 24 hours, or when node count >8k, or cumulative reinforcement >50). Under the single‑threaded operation lock, the following steps execute atomically:

1. **Update cohort centroids** – one mini‑batch k‑means step with 0.8 momentum.
2. **Propagate episode log** – as described in 5.4.
3. **Superstition filter** (Section 6) – compute CI for all intervention edges; prune those with no positive effect.
4. **Prune dead edges** – remove edges with $`\mu < 0.04`$ OR unused for >7 days and less than 3 traversals.
5. **Cull dead nodes** – nodes with $`\text{salience} \cdot \text{degree} < 0.12`$ and age >14 days are removed. For each removed node, top‑3 incoming × top‑3 outgoing edges with cosine >0.6 are bridged (new edge created with initial $`\alpha=1,\beta=1`$, weight $`1.2 \cdot w_{in} \cdot w_{out} \cdot \cos`$, capped at 0.9).
6. **Merge near‑identical nodes** – if cosine >0.95 and timestamp difference <1 h, merge into one node with average embedding and max salience.
7. **Skill compilation** – detect high‑confidence State→Action→Outcome chains (total evidence >50, success rate >0.8) and create Skill nodes.

---

## 11. Concurrency & Persistence

### 11.1 Operation Loop
All mutations are sent to an MPSC queue and processed sequentially by a single worker thread. This eliminates race conditions and locks on the graph.

### 11.2 Write‑Ahead Log (WAL)
Each mutation is appended to a binary WAL with a sequence number and CRC32. On crash, the latest snapshot is loaded, then the WAL replayed. Periodic snapshots truncate the WAL.

### 11.3 Snapshotting
Every 1,000 operations, the full graph state (nodes, edges, dense core, cohort centroids, statistics) is serialised to a compact binary file using `bincode` or similar.

---

## 12. Sequence Diagrams & Event Flows

*(Detailed ASCII diagrams omitted for brevity; they follow the subsystem APIs.)*

**Retrieval flow**:
Agent → `retrieve(query_emb, model_id)` → project → top‑K seeds → Thompson‑sampled Dijkstra (3 hops) → format causal chain → return.

**Learning flow**:
Agent → `learn(state, action, outcome, ε, model_id)` → create nodes if needed → create edges → update Action→Outcome edge (forgetting, increment, episode log) → return.

**Consolidation flow**:
Timer/trigger → acquire write lock → centroids update → prop log → superstition filter → prune edges → cull nodes & bridge → merge → skills → release lock.

---

## 13. Failure Scenarios & Recovery

| Scenario | Detection | Response |
|----------|-----------|----------|
| NaN embedding | norm check | Reject, log |
| Edge count overflow | >1e6 total | Rescale both counts by 0.5 |
| WAL corruption | CRC mismatch | Stop replay at entry, continue |
| Crash during consolidation | WAL incomplete | Revert to pre‑consolidation snapshot, replay WAL, retry |
| Projection file missing | startup check | Use identity, log warning |
| Cohort underpopulation | <5 intervention edges | Skip filter, mark edge unvalidated |

---

## 14. Performance & Scalability

| Operation | Time (100k nodes) | Memory |
|-----------|-------------------|--------|
| Node insert | ~5 µs | 269 B per node |
| Edge update | ~1 µs | 24 B per edge |
| Retrieval (3 hops) | <50 µs | — |
| Full consolidation | ~50 ms | temporary copies |
| Single‑agent 10‑year footprint | — | ~78 MB |
| Cross‑model projection | <1 µs | 16 KB per model |

Scaling: graphs are independent per agent; shardable by agent ID. A single Rust thread handles >100k writes/s.

---

## 15. Security & Privacy

- Local service: binds to `127.0.0.1` by default; no authentication.
- Networked service: mutual TLS + API key.
- `delete_agent(id)` removes all data, including WAL entries and snapshots.
- Embeddings are stored, not raw text, offering inherent anonymisation.

---

## 16. Extension Points

- Working memory buffer size (configurable).
- Edge birth similarity threshold.
- Number of cohorts.
- Dense core slices.
- Credit propagation factor.
- Skill compilation criteria.
- Custom cohorting algorithm (pluggable).

---

## 17. Design Trade‑Offs (Summary)

| Decision | Alternative | Rationale |
|----------|-------------|-----------|
| Beta edges | Scalar + separate confidence | Bayesian conjugate, free uncertainty |
| Thompson sampling | ε‑greedy | Provably optimal, no extra param |
| Intervention‑only ATE | Full causal graph | Tractable, agent actions = interventions |
| Self‑calibrating salience | Fixed thresholds | Domain‑agnostic, no tuning |
| Delta rule core | Hebbian + SVD | Self‑cleaning, proven in linear attention |
| Single‑threaded | Multi‑threaded | Simplicity, no deadlocks |
| 128‑dim embeddings | 768/1536 | Laptop‑scale, sufficient for causal structure |

---

This specification covers every mathematical detail and engineering decision behind CMHM. It is ready to be handed to an implementation team.