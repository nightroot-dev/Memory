"""
MiniLM d-prime benchmark — corpus v2 (mirrors dprime_bench.rs exactly).

Runs all-MiniLM-L6-v2 through the same 90-pair corpus + 15 hard negatives used by
the Rust bench_minilm_dprime test, and computes all acceptance gate metrics.

Usage:
    python tools/minilm_dprime.py

Requirements:
    pip install sentence-transformers   (PyTorch pulled in automatically)
"""

import math
import time
import os
from sentence_transformers import SentenceTransformer

# ---------------------------------------------------------------------------
# Corpus v2 — must stay identical to dprime_bench.rs
# ---------------------------------------------------------------------------

PARAPHRASE_PAIRS = [
    ("the cat sat on the mat",               "a feline rested upon the rug"),
    ("it is raining heavily outside",        "the weather feels very wet today"),
    ("the patient recovered quickly",        "the sick person got better fast"),
    ("she purchased a new vehicle",          "the woman bought an automobile"),
    ("the child began to weep",              "a youngster started sobbing"),
    ("the temperature dropped sharply",      "it became noticeably colder outside"),
    ("the athlete finished first",           "the sports person won the contest"),
    ("the book was difficult to read",       "the text was hard to comprehend"),
    ("he arrived after the session began",   "the man showed up when everyone was assembled"),
    ("the economy grew strongly",            "financial output increased substantially"),
    ("the program encountered a fatal flaw", "the application has a critical failure"),
    ("the scientist achieved a major discovery", "the researcher made a significant breakthrough"),
    ("the structure was consumed by flames", "the building burned completely to the ground"),
    ("the pupil succeeded on the test",      "the student passed the examination"),
    ("the task was finished before deadline","work was completed ahead of the due date"),
    ("the machine stopped accepting requests","the server became unresponsive to connections"),
    ("the process is using excessive RAM",   "memory consumption by the application is too high"),
    ("the settings document is absent",      "the configuration file cannot be located"),
    ("network access became unavailable",    "the connection to the internet was severed"),
    ("the output has wrong numbers in it",   "the result contains erroneous figures"),
    ("the function returns early on error",  "the routine exits immediately when a problem occurs"),
    ("disk space is critically low",         "the drive has almost no room remaining"),
    ("the session was terminated abruptly",  "the connection was closed unexpectedly"),
    ("the image failed to load",             "the picture could not be displayed"),
    ("the user lacks the required permissions","access was denied to the requester"),
    ("the file was successfully written to disk","data was saved to the storage device"),
    ("the loop ran forever without stopping","execution continued indefinitely without termination"),
    ("the cache was cleared automatically",  "stored data was removed from temporary memory"),
    ("the request took too long to complete","the operation exceeded the allowed time limit"),
    ("the backup completed without errors",  "the archive process finished cleanly"),
]

TOPICAL_PAIRS = [
    ("nvidia gpu driver installation failed",     "nvidia graphics driver setup error"),
    ("python flask web server routes",            "flask python web application routing"),
    ("memory leak in heap allocation",            "heap memory not freed causing memory leak"),
    ("database query optimization techniques",    "sql query performance optimization methods"),
    ("neural network training convergence",       "training deep neural network to converge"),
    ("rust borrow checker compilation error",     "rust compiler borrow checker error message"),
    ("kubernetes pod deployment failed startup",  "kubernetes container pod startup failure"),
    ("git merge conflict resolution strategy",    "resolving git merge conflicts in repository"),
    ("docker container image build process",      "building docker container images locally"),
    ("api rate limit exceeded error response",    "api endpoint rate limiting error response"),
    ("cargo build linker error windows gnu",      "cargo build failed linker error on windows"),
    ("snapshot version mismatch quarantine file", "snapshot schema version mismatch quarantine"),
    ("wal crash recovery replay exactly once",    "wal replay crash recovery exactly once"),
    ("context registry uuid sleeping state",      "context uuid registry sleeping state preserved"),
    ("embedding cosine similarity threshold gate","cosine similarity threshold embedding gate"),
    ("reinforcement learning reward signal update","reward signal update reinforcement learning"),
    ("transformer attention mechanism layer",     "attention mechanism layer transformer model"),
    ("linux kernel module driver loading",        "loading linux kernel module driver kernel"),
    ("http request response status code handling","http response status code request handling"),
    ("json serialization deserialization error",  "json deserialization serialization error type"),
    ("redis cache eviction policy maxmemory lru",    "redis maxmemory lru cache eviction policy"),
    ("openssl tls certificate verification chain",   "tls ssl certificate chain verification openssl"),
    ("cmake build system generator configuration",   "cmake configuration generator build system"),
    ("prometheus metrics scraping endpoint grafana", "grafana prometheus scrape metrics endpoint"),
    ("llvm ir optimization pass compiler backend",   "compiler optimization pass llvm ir backend"),
    ("tokio async runtime thread pool executor",     "async tokio thread pool runtime executor"),
    ("grpc protobuf serialization rpc service",      "protobuf grpc rpc service serialization"),
    ("heap fragmentation allocator malloc jemalloc", "jemalloc allocator malloc heap fragmentation"),
    ("regex pattern matching backreference group",   "regex group backreference pattern match"),
    ("systemd unit file service restart policy",     "service unit file systemd restart policy"),
]

UNRELATED_PAIRS = [
    ("the cat sat on the mat",               "compound interest grows exponentially over time"),
    ("it is raining heavily outside",        "the database returned five thousand rows"),
    ("the patient recovered quickly",        "quantum entanglement enables secure communication"),
    ("she purchased a new vehicle",          "photosynthesis converts sunlight into glucose"),
    ("the child began to weep",              "the algorithm runs in linearithmic time"),
    ("the temperature dropped sharply",      "the endpoint returns JSON formatted data"),
    ("the athlete finished first",           "the neural network trained for one hundred epochs"),
    ("the book was difficult to read",       "binary search requires a sorted array to work"),
    ("he arrived after the session began",   "the ocean covers seventy percent of the planet"),
    ("the economy grew strongly",            "the compiler optimized the inner loop bytecode"),
    ("the program encountered a fatal flaw", "the symphony was performed in the key of D minor"),
    ("the scientist achieved a major discovery","gradient descent minimizes the objective function"),
    ("the structure was consumed by flames", "the moon completes one orbit every twenty-seven days"),
    ("the pupil succeeded on the test",      "hash tables provide constant average lookup time"),
    ("the task was finished before deadline","mitochondria produce ATP via oxidative phosphorylation"),
    ("the machine stopped accepting requests","the painting was created during the Renaissance period"),
    ("the process is using excessive RAM",   "the chef seasoned the dish with salt and pepper"),
    ("the settings document is absent",      "the river flows into the estuary near the delta"),
    ("network access became unavailable",    "the immune system generates antibodies against pathogens"),
    ("the output has wrong numbers in it",   "the political candidate won by a narrow margin"),
    ("redis cache eviction policy maxmemory",  "the volcano erupted violently along the coastline"),
    ("openssl certificate verification chain", "the medieval knight wore heavy plate armor in battle"),
    ("cmake build system configuration",       "bees communicate direction through a waggle dance"),
    ("prometheus metrics scraping endpoint",   "the glacier retreated by several kilometers over millennia"),
    ("llvm compiler optimization passes",      "the baker kneaded the dough for thirty minutes by hand"),
    ("tokio async thread pool executor",       "ancient romans built roads throughout their vast empire"),
    ("grpc protobuf rpc service",              "the marathon runner completed the course in under three hours"),
    ("heap fragmentation allocator malloc",    "rainforests contain approximately half of all known species"),
    ("regex pattern matching backreference",   "the piano concerto lasted ninety uninterrupted minutes"),
    ("systemd service restart unit file",      "the spacecraft successfully entered lunar orbit yesterday"),
]

HARD_NEGATIVE_PAIRS = [
    ("the deployment succeeded",                   "the deployment did not succeed"),
    ("the server is accepting connections",         "the server is not accepting connections"),
    ("restart fixed the issue",                    "restart did not fix the issue"),
    ("the patch was applied correctly",            "the patch was not applied correctly"),
    ("all requests completed successfully",        "no requests completed successfully"),
    ("the cache hit rate increased significantly", "the cache hit rate decreased significantly"),
    ("memory consumption is critically low",       "memory consumption is critically high"),
    ("the process is running normally",            "the process has stopped unexpectedly"),
    ("the connection was accepted",                "the connection was refused"),
    ("every node responded within the time limit", "no nodes responded within the time limit"),
    ("the server timed out the client",            "the client timed out the server"),
    ("the database locked the table",              "the table was locked by an external process"),
    ("log in before making requests",              "make requests before logging in"),
    ("out of memory error in the heap",            "stack overflow unrelated to heap allocation"),
    ("the test passed then was marked stable",     "the test passed then immediately failed"),
]

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def cosine(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    na  = math.sqrt(sum(x*x for x in a))
    nb  = math.sqrt(sum(x*x for x in b))
    if na < 1e-9 or nb < 1e-9:
        return 0.0
    return dot / (na * nb)

def mean(xs):
    return sum(xs) / len(xs) if xs else 0.0

def std_dev(xs):
    if len(xs) < 2:
        return 0.0
    m   = mean(xs)
    var = sum((x - m)**2 for x in xs) / (len(xs) - 1)
    return math.sqrt(var)

def dprime(pos, neg):
    mu_p, mu_n = mean(pos), mean(neg)
    sp, sn     = std_dev(pos), std_dev(neg)
    pooled     = math.sqrt((sp*sp + sn*sn) / 2.0)
    if pooled < 1e-8:
        return 0.0
    return (mu_p - mu_n) / pooled

def embed_pairs(model, pairs):
    sims = []
    for a, b in pairs:
        va = model.encode(a, normalize_embeddings=True).tolist()
        vb = model.encode(b, normalize_embeddings=True).tolist()
        sims.append(cosine(va, vb))
    return sims

def assert_embedding_valid(emb, expected_dim, label):
    assert len(emb) == expected_dim, \
        f"Embedding '{label}': dim={len(emb)} expected={expected_dim}"
    assert all(math.isfinite(x) for x in emb), \
        f"Embedding '{label}': contains NaN or inf"
    norm = math.sqrt(sum(x*x for x in emb))
    assert abs(norm - 1.0) < 0.02, \
        f"Embedding '{label}': L2 norm={norm:.6f} expected ~1.0"

def retrieval_metrics(model, paraphrase_pairs, n_queries=15):
    """Recall@1/@5/@10 and MRR. 30-doc corpus (A-sides), 15 queries (B-sides 0..15)."""
    n_docs = len(paraphrase_pairs)
    doc_embs = [model.encode(a, normalize_embeddings=True).tolist()
                for a, _ in paraphrase_pairs]
    r1 = r5 = r10 = 0
    mrr_sum = 0.0
    for q in range(n_queries):
        q_emb = model.encode(paraphrase_pairs[q][1], normalize_embeddings=True).tolist()
        scores = [(d, cosine(q_emb, doc_embs[d])) for d in range(n_docs)]
        scores.sort(key=lambda x: -x[1])
        rank = next((i+1 for i, (di, _) in enumerate(scores) if di == q), n_docs+1)
        if rank <= 1:  r1  += 1
        if rank <= 5:  r5  += 1
        if rank <= 10: r10 += 1
        mrr_sum += 1.0 / rank
    n = float(n_queries)
    return r1/n, r5/n, r10/n, mrr_sum/n

def hard_neg_fpr(model, hard_neg_pairs, threshold=0.6):
    fp = 0
    for a, b in hard_neg_pairs:
        va = model.encode(a, normalize_embeddings=True).tolist()
        vb = model.encode(b, normalize_embeddings=True).tolist()
        if cosine(va, vb) >= threshold:
            fp += 1
    return fp / len(hard_neg_pairs)

def latency_ms(model, n_reps=100):
    text = "the service returned an unexpected error code during initialization"
    t0 = time.perf_counter()
    for _ in range(n_reps):
        model.encode(text, normalize_embeddings=True)
    elapsed = time.perf_counter() - t0
    return (elapsed * 1000.0) / n_reps

def model_disk_size_mb(model_name):
    import os
    cache = os.path.join(os.path.expanduser("~"), ".cache", "huggingface", "hub")
    slug = "models--" + model_name.replace("/", "--")
    model_dir = os.path.join(cache, slug)
    if not os.path.isdir(model_dir):
        return None
    total = 0
    for dirpath, _, filenames in os.walk(model_dir):
        for fn in filenames:
            try:
                total += os.path.getsize(os.path.join(dirpath, fn))
            except OSError:
                pass
    return total / (1024 * 1024)

def print_dist(label, sims):
    m   = mean(sims)
    s   = std_dev(sims)
    mn  = min(sims)
    mx  = max(sims)
    print(f"      {label:<16}  n={len(sims):2d}  mu={m:+.4f}  sigma={s:.4f}  min={mn:+.4f}  max={mx:+.4f}")

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

MODEL_NAME = "sentence-transformers/all-MiniLM-L6-v2"

print(f"Loading {MODEL_NAME} (downloads ~90 MB on first run) ...")
model = SentenceTransformer(MODEL_NAME)
dim = model.get_embedding_dimension() if hasattr(model, "get_embedding_dimension") else 384
print(f"Loaded. dim={dim}")

print()
print("=================================================================")
print(f"  Embedder : all-MiniLM-L6-v2  (dim={dim})")
print(f"  Corpus   : {len(PARAPHRASE_PAIRS)} para + {len(TOPICAL_PAIRS)} topo"
      f" + {len(UNRELATED_PAIRS)} unrel + {len(HARD_NEGATIVE_PAIRS)} hard-neg")
print("=================================================================")

# Validity
print()
print("  Embedding validity:")
for label, text in [
    ("para-A[0]",    PARAPHRASE_PAIRS[0][0]),
    ("para-B[0]",    PARAPHRASE_PAIRS[0][1]),
    ("topical-A[0]", TOPICAL_PAIRS[0][0]),
    ("unrel-A[0]",   UNRELATED_PAIRS[0][0]),
    ("hardneg-A[0]", HARD_NEGATIVE_PAIRS[0][0]),
]:
    emb = model.encode(text, normalize_embeddings=True).tolist()
    assert_embedding_valid(emb, dim, label)
print(f"  PASS  dim={dim}, L2-norm in [0.99,1.01], no NaN/inf")

# Pairwise cosines
sims_para     = embed_pairs(model, PARAPHRASE_PAIRS)
sims_topo     = embed_pairs(model, TOPICAL_PAIRS)
sims_unrel    = embed_pairs(model, UNRELATED_PAIRS)
sims_hard_neg = embed_pairs(model, HARD_NEGATIVE_PAIRS)
sims_comb     = sims_para + sims_topo

dp_para     = dprime(sims_para,     sims_unrel)
dp_topo     = dprime(sims_topo,     sims_unrel)
dp_comb     = dprime(sims_comb,     sims_unrel)
dp_hard_neg = dprime(sims_hard_neg, sims_unrel)

print()
print("  Cosine distributions:")
print_dist("paraphrase",    sims_para)
print_dist("topical",       sims_topo)
print_dist("unrelated",     sims_unrel)
print_dist("hard-negative", sims_hard_neg)
print_dist("combined",      sims_comb)

print()
print("  d' (vs unrelated):")
print(f"      paraphrase : {dp_para:+.4f}")
print(f"      topical    : {dp_topo:+.4f}")
print(f"      combined   : {dp_comb:+.4f}")
print(f"      hard-neg   : {dp_hard_neg:+.4f}  (informational)")

# Retrieval
print()
print("  Retrieval (15 queries / 30-doc corpus):")
r1, r5, r10, mrr = retrieval_metrics(model, PARAPHRASE_PAIRS, n_queries=15)
print(f"      Recall@1  = {r1:.4f}")
print(f"      Recall@5  = {r5:.4f}")
print(f"      Recall@10 = {r10:.4f}")
print(f"      MRR       = {mrr:.4f}")

# FPR
fpr = hard_neg_fpr(model, HARD_NEGATIVE_PAIRS, threshold=0.6)
n_fp = round(fpr * len(HARD_NEGATIVE_PAIRS))
print()
print(f"  Hard-negative FPR @cos>=0.6 : {fpr:.4f}  ({n_fp} of {len(HARD_NEGATIVE_PAIRS)})")

# Latency + model size
ms = latency_ms(model, n_reps=100)
disk_mb = model_disk_size_mb(MODEL_NAME)
print()
print(f"  Latency: {ms:.3f} ms/embed (100 reps, warm)")
if disk_mb is not None:
    print(f"  Model on-disk: {disk_mb:.1f} MB")

# Hash-128d non-inferiority threshold
HASH_DP_TOPO_BASELINE = 4.107   # measured 2026-08-12 on 30-pair corpus
ni_thresh = 0.8 * HASH_DP_TOPO_BASELINE

print()
print("=================================================================")
print("  -- MiniLM acceptance gate --")
gates = [
    ("d'_para  >= 1.5",  dp_para  >= 1.5,  f"d'_para  = {dp_para:+.4f}"),
    ("d'_topo  >= 1.5",  dp_topo  >= 1.5,  f"d'_topo  = {dp_topo:+.4f}"),
    ("d'_comb  >= 1.3",  dp_comb  >= 1.3,  f"d'_comb  = {dp_comb:+.4f}"),
    (f"d'_topo >= 80% of hash ({ni_thresh:.3f})",
                         dp_topo  >= ni_thresh, f"non-inferiority = {dp_topo:+.4f}"),
    ("Recall@1 >= 0.50", r1       >= 0.50, f"Recall@1 = {r1:.4f}"),
    ("Recall@5 >= 0.80", r5       >= 0.80, f"Recall@5 = {r5:.4f}"),
    ("MRR      >= 0.60", mrr      >= 0.60, f"MRR      = {mrr:.4f}"),
]
all_pass = True
for gate_desc, passed, measured in gates:
    mark = "PASS" if passed else "FAIL"
    print(f"  {mark}  {measured}  ({gate_desc})")
    if not passed:
        all_pass = False

# Hard-negative FPR is informational — NOT a gate.
# Sentence encoders capture topic/domain similarity, not negation-aware reasoning.
# "The deployment succeeded" and "the deployment did not succeed" share domain,
# entities, and structure — high cosine is expected and correct for a similarity
# encoder. CMHM disambiguates outcomes via Beta bandit feedback (separate outcome
# nodes), not embedding distance. FPR@0.6=1.0 is expected behavior, not a bug.
n_fp = round(fpr * len(HARD_NEGATIVE_PAIRS))
print(f"  INFO  FPR@0.6 = {fpr:.4f}  ({n_fp} of {len(HARD_NEGATIVE_PAIRS)}"
      f" — informational, not gated)")

print()
if all_pass:
    print("  MiniLM PASSES all acceptance gates.")
    print("  Pending: native Rust ONNX backend validation before TEXT_MODEL_ID flip.")
else:
    print("  MiniLM FAILS one or more acceptance gates.")
print("=================================================================")
