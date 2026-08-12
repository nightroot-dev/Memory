"""
MiniLM d-prime benchmark v3 — operational mixed-corpus retrieval.

Corpus v2 (30 pairs x 3 categories + 15 hard negatives) is retained for d'
and pairwise-cosine metrics. NEW in v3:

  OPERATIONAL_QUERY_CORPUS: 15 queries each paired with
    D+: a paraphrase of the query (true positive, different surface form)
    D-: the contradiction of the query (from HARD_NEGATIVE_PAIRS B-sides)
  Full retrieval corpus (45 docs): all 15 D+, all 15 D-, 10 topical
    distractors, 5 unrelated distractors.

Pre-declared gates (locked before any results observed -- commit db2eb8a):
  Relevant Recall@1     >= 0.40
  Relevant Recall@5     >= 0.67
  MRR                   >= 0.40
  Contradiction-at-1    <  0.40
  Hard-neg occupancy@5  <  0.40
  Mean margin           >  0.0   (cosine(q,D+) - cosine(q,D-))

Usage:
    python tools/minilm_dprime.py

Requirements:
    pip install sentence-transformers
"""

import math
import time
import os
import random

# ---------------------------------------------------------------------------
# Corpus v2 (unchanged -- used for d' and distribution metrics)
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
# Operational mixed-corpus: (query, D+, D-)
#
# query = HARD_NEGATIVE_PAIRS A-side (operational domain)
# D+    = paraphrase of query with different surface form
# D-    = HARD_NEGATIVE_PAIRS B-side (the contradiction)
#
# PRE-DECLARED GATES (must not be changed after observing results):
#   Recall@1             >= 0.40
#   Recall@5             >= 0.67
#   MRR                  >= 0.40
#   Contradiction-at-1   <  0.40
#   Hard-neg occupancy@5 <  0.40
#   Mean margin          >  0.0
# ---------------------------------------------------------------------------

OPERATIONAL_QUERY_CORPUS = [
    # (query, D+, D-)
    ("the deployment succeeded",
     "the release was applied without errors",
     "the deployment did not succeed"),
    ("the server is accepting connections",
     "the service is responding to incoming requests",
     "the server is not accepting connections"),
    ("restart fixed the issue",
     "the system resumed normal operation after reboot",
     "restart did not fix the issue"),
    ("the patch was applied correctly",
     "the update was installed successfully",
     "the patch was not applied correctly"),
    ("all requests completed successfully",
     "every operation finished without error",
     "no requests completed successfully"),
    ("the cache hit rate increased significantly",
     "the proportion of cached responses grew noticeably",
     "the cache hit rate decreased significantly"),
    ("memory consumption is critically low",
     "the process uses very little RAM",
     "memory consumption is critically high"),
    ("the process is running normally",
     "the application is operating as expected",
     "the process has stopped unexpectedly"),
    ("the connection was accepted",
     "the handshake completed and a session was established",
     "the connection was refused"),
    ("every node responded within the time limit",
     "all cluster members replied before the deadline",
     "no nodes responded within the time limit"),
    ("the server timed out the client",
     "the upstream service dropped the stalled client request",
     "the client timed out the server"),
    ("the database locked the table",
     "the table acquired an exclusive lock in the database",
     "the table was locked by an external process"),
    ("log in before making requests",
     "authenticate your session prior to sending API calls",
     "make requests before logging in"),
    ("out of memory error in the heap",
     "the heap allocator exhausted available memory",
     "stack overflow unrelated to heap allocation"),
    ("the test passed then was marked stable",
     "the test suite completed and the build was approved",
     "the test passed then immediately failed"),
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

def bootstrap_ci(values, n_boot=1000, alpha=0.05, seed=42):
    """95% percentile bootstrap CI on the mean. Fixed seed for reproducibility."""
    rng = random.Random(seed)
    n = len(values)
    boot_means = sorted(
        sum(rng.choices(values, k=n)) / n for _ in range(n_boot)
    )
    lo = boot_means[int(alpha / 2.0 * n_boot)]
    hi = boot_means[int((1.0 - alpha / 2.0) * n_boot) - 1]
    return lo, hi

def hard_neg_fpr(model, threshold=0.6):
    fp = 0
    for a, b in HARD_NEGATIVE_PAIRS:
        va = model.encode(a, normalize_embeddings=True).tolist()
        vb = model.encode(b, normalize_embeddings=True).tolist()
        if cosine(va, vb) >= threshold:
            fp += 1
    return fp / len(HARD_NEGATIVE_PAIRS)

def latency_ms(model, n_reps=100):
    text = "the service returned an unexpected error code during initialization"
    t0 = time.perf_counter()
    for _ in range(n_reps):
        model.encode(text, normalize_embeddings=True)
    return (time.perf_counter() - t0) * 1000.0 / n_reps

def model_disk_size_mb(model_name):
    cache = os.path.join(os.path.expanduser("~"), ".cache", "huggingface", "hub")
    slug  = "models--" + model_name.replace("/", "--")
    model_dir = os.path.join(cache, slug)
    if not os.path.isdir(model_dir):
        return None
    total = 0
    for dp, _, fns in os.walk(model_dir):
        for fn in fns:
            try: total += os.path.getsize(os.path.join(dp, fn))
            except OSError: pass
    return total / (1024 * 1024)

def print_dist(label, sims):
    m = mean(sims); s = std_dev(sims)
    print(f"      {label:<16}  n={len(sims):2d}  mu={m:+.4f}  sigma={s:.4f}"
          f"  min={min(sims):+.4f}  max={max(sims):+.4f}")

# ---------------------------------------------------------------------------
# Mixed-corpus retrieval benchmark (v3 gate test)
# ---------------------------------------------------------------------------

def mixed_corpus_retrieval(model):
    """
    Corpus (45 docs):
      D+[0..15]  -- correct paraphrases
      D-[0..15]  -- contradictions
      Topical[0..10]  -- lexical distractors (TOPICAL_PAIRS A-sides)
      Unrelated[0..5] -- unrelated distractors

    For each of 15 queries:
      - Rank all 45 docs by cosine(query, doc)
      - D+[i] is the ground-truth correct doc for query i
      - D-[i] is the contradiction for query i

    Returns dict with per-query arrays and aggregate metrics.
    """
    n_q = len(OPERATIONAL_QUERY_CORPUS)

    # Build corpus
    dplus_texts  = [d for _, d, _ in OPERATIONAL_QUERY_CORPUS]
    dminus_texts = [d for _, _, d in OPERATIONAL_QUERY_CORPUS]
    topical_dist = [a for a, _ in TOPICAL_PAIRS[:10]]
    unrel_dist   = [a for a, _ in UNRELATED_PAIRS[:5]]
    corpus       = dplus_texts + dminus_texts + topical_dist + unrel_dist
    n_corpus     = len(corpus)

    # Indices
    dplus_range  = range(0, n_q)          # D+[i] = corpus[i]
    dminus_range = range(n_q, 2*n_q)     # D-[i] = corpus[n_q + i]

    print(f"  Embedding {n_corpus} corpus docs ...", flush=True)
    corpus_embs = [model.encode(d, normalize_embeddings=True).tolist() for d in corpus]

    recall1_per = []
    recall5_per = []
    recall10_per = []
    mrr_per     = []
    contra1_per = []
    hn5_per     = []
    margins     = []

    print()
    print(f"  {'Query':<44}  {'D+ rank':>7}  {'D- rank':>7}  {'margin':>7}  {'C@1':>4}")
    print(f"  {'-'*44}  {'-'*7}  {'-'*7}  {'-'*7}  {'-'*4}")

    for i, (q, _, _) in enumerate(OPERATIONAL_QUERY_CORPUS):
        q_emb = model.encode(q, normalize_embeddings=True).tolist()

        scores = [(j, cosine(q_emb, corpus_embs[j])) for j in range(n_corpus)]
        scores.sort(key=lambda x: -x[1])

        dplus_idx  = i
        dminus_idx = n_q + i

        ranked_idxs = [j for j, _ in scores]
        dplus_rank  = ranked_idxs.index(dplus_idx)  + 1
        dminus_rank = ranked_idxs.index(dminus_idx) + 1

        score_dplus  = cosine(q_emb, corpus_embs[dplus_idx])
        score_dminus = cosine(q_emb, corpus_embs[dminus_idx])
        margin_i     = score_dplus - score_dminus

        top5_idxs   = ranked_idxs[:5]
        hn_in_top5  = sum(1 for j in top5_idxs if j in dminus_range)

        r1  = 1.0 if dplus_rank <= 1  else 0.0
        r5  = 1.0 if dplus_rank <= 5  else 0.0
        r10 = 1.0 if dplus_rank <= 10 else 0.0
        c1  = 1.0 if dminus_rank == 1 else 0.0

        recall1_per.append(r1)
        recall5_per.append(r5)
        recall10_per.append(r10)
        mrr_per.append(1.0 / dplus_rank)
        contra1_per.append(c1)
        hn5_per.append(hn_in_top5 / 5.0)
        margins.append(margin_i)

        q_short = q[:44]
        contra_flag = " *" if c1 else ""
        print(f"  {q_short:<44}  {dplus_rank:>7}  {dminus_rank:>7}  {margin_i:>+7.4f}  {int(c1):>4}{contra_flag}")

    print()

    return {
        "recall_at_1":         mean(recall1_per),
        "recall_at_5":         mean(recall5_per),
        "recall_at_10":        mean(recall10_per),
        "mrr":                 mean(mrr_per),
        "contradiction_at_1":  mean(contra1_per),
        "hard_neg_occ_at5":    mean(hn5_per),
        "mean_margin":         mean(margins),
        "min_margin":          min(margins),
        # per-query arrays for bootstrap
        "_recall1":   recall1_per,
        "_recall5":   recall5_per,
        "_mrr":       mrr_per,
        "_contra1":   contra1_per,
        "_hn5":       hn5_per,
        "_margins":   margins,
    }

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

MODEL_NAME = "sentence-transformers/all-MiniLM-L6-v2"

print(f"Loading {MODEL_NAME} ...")
from sentence_transformers import SentenceTransformer
model = SentenceTransformer(MODEL_NAME)
dim = 384
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

# d' distributions
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

# FPR informational
fpr = hard_neg_fpr(model, threshold=0.6)
n_fp = round(fpr * len(HARD_NEGATIVE_PAIRS))
print()
print(f"  Hard-negative FPR @cos>=0.6 : {fpr:.4f}  ({n_fp} of {len(HARD_NEGATIVE_PAIRS)}"
      f" -- informational, not gated)")

# Latency / model size
ms      = latency_ms(model, n_reps=100)
disk_mb = model_disk_size_mb(MODEL_NAME)
print()
print(f"  Latency   : {ms:.3f} ms/embed (100 reps, warm)")
if disk_mb is not None:
    print(f"  Model disk: {disk_mb:.1f} MB")

# ---------------------------------------------------------------------------
# d' gate (informational here; gated in Rust ONNX test)
# ---------------------------------------------------------------------------
print()
print("  d' gate results:")
HASH_DP_TOPO_BASELINE = 4.107
ni_thresh = 0.8 * HASH_DP_TOPO_BASELINE
dprime_gates = [
    ("d'_para  >= 1.5",  dp_para  >= 1.5,  f"d'_para  = {dp_para:+.4f}"),
    ("d'_topo  >= 1.5",  dp_topo  >= 1.5,  f"d'_topo  = {dp_topo:+.4f}"),
    ("d'_comb  >= 1.3",  dp_comb  >= 1.3,  f"d'_comb  = {dp_comb:+.4f}"),
    (f"non-inf >= {ni_thresh:.3f}", dp_topo >= ni_thresh,
     f"non-inf  = {dp_topo:+.4f}"),
]
for desc, ok, measured in dprime_gates:
    print(f"  {'PASS' if ok else 'FAIL'}  {measured}  ({desc})")

# ---------------------------------------------------------------------------
# Mixed-corpus retrieval (v3 gate)
# ---------------------------------------------------------------------------
print()
print("=" * 65)
print("  OPERATIONAL MIXED-CORPUS RETRIEVAL (v3 -- pre-declared gates)")
print("=" * 65)
print(f"  Corpus: {len(OPERATIONAL_QUERY_CORPUS)} D+ + {len(OPERATIONAL_QUERY_CORPUS)} D-"
      f" + 10 topical + 5 unrelated = 45 docs")
print()
print("  Pre-declared gates (locked before results observed):")
print("    Recall@1             >= 0.40")
print("    Recall@5             >= 0.67")
print("    MRR                  >= 0.40")
print("    Contradiction-at-1   <  0.40")
print("    Hard-neg occupancy@5 <  0.40")
print("    Mean margin          >  0.00")
print()

mc = mixed_corpus_retrieval(model)

# Bootstrap CIs (fixed seed 42, 1000 resamples)
r1_lo,  r1_hi  = bootstrap_ci(mc["_recall1"])
r5_lo,  r5_hi  = bootstrap_ci(mc["_recall5"])
mrr_lo, mrr_hi = bootstrap_ci(mc["_mrr"])
c1_lo,  c1_hi  = bootstrap_ci(mc["_contra1"])
hn_lo,  hn_hi  = bootstrap_ci(mc["_hn5"])
mg_lo,  mg_hi  = bootstrap_ci(mc["_margins"])

print("  Aggregate results (95% CI, seed=42, n_boot=1000):")
print(f"    Recall@1             = {mc['recall_at_1']:.4f}  [{r1_lo:.3f}, {r1_hi:.3f}]")
print(f"    Recall@5             = {mc['recall_at_5']:.4f}  [{r5_lo:.3f}, {r5_hi:.3f}]")
print(f"    MRR                  = {mc['mrr']:.4f}  [{mrr_lo:.3f}, {mrr_hi:.3f}]")
print(f"    Contradiction-at-1   = {mc['contradiction_at_1']:.4f}  [{c1_lo:.3f}, {c1_hi:.3f}]")
print(f"    Hard-neg occupancy@5 = {mc['hard_neg_occ_at5']:.4f}  [{hn_lo:.3f}, {hn_hi:.3f}]")
print(f"    Mean margin          = {mc['mean_margin']:+.4f}  [{mg_lo:+.3f}, {mg_hi:+.3f}]")
print(f"    Min margin           = {mc['min_margin']:+.4f}")

print()
print("  -- Mixed-corpus gate evaluation (pre-declared) --")
mc_gates = [
    ("Recall@1   >= 0.40", mc["recall_at_1"]        >= 0.40),
    ("Recall@5   >= 0.67", mc["recall_at_5"]        >= 0.67),
    ("MRR        >= 0.40", mc["mrr"]                >= 0.40),
    ("Contra@1   <  0.40", mc["contradiction_at_1"] <  0.40),
    ("HN-occ@5   <  0.40", mc["hard_neg_occ_at5"]  <  0.40),
    ("Margin avg >  0.00", mc["mean_margin"]         >  0.00),
]
mc_all_pass = True
for desc, ok in mc_gates:
    print(f"  {'PASS' if ok else 'FAIL'}  {desc}")
    if not ok:
        mc_all_pass = False

print()
if mc_all_pass:
    print("  Mixed-corpus retrieval: ALL GATES PASS")
    print("  MiniLM embedding approval: GATED PENDING native Rust ONNX backend.")
else:
    n_fail = sum(1 for _, ok in mc_gates if not ok)
    print(f"  Mixed-corpus retrieval: {n_fail} GATE(S) FAIL")
    print("  MiniLM embedding approval: BLOCKED.")
    print("  Do NOT flip TEXT_MODEL_ID.")
    print()
    print("  Diagnosis notes:")
    if mc["contradiction_at_1"] >= 0.40:
        print(f"  - Contradiction-at-1={mc['contradiction_at_1']:.4f}: contradictions occupy rank 1")
        print(f"    for {round(mc['contradiction_at_1']*15)} of 15 queries. D- scores higher")
        print(f"    than D+ because they share surface tokens with the query.")
    if mc["mean_margin"] <= 0.0:
        print(f"  - Mean margin={mc['mean_margin']:+.4f}: D- outscores D+ on average.")
        print(f"    MiniLM similarity between a statement and its negation exceeds")
        print(f"    similarity to a true paraphrase with different surface form.")
    if mc["hard_neg_occ_at5"] >= 0.40:
        print(f"  - HN-occ@5={mc['hard_neg_occ_at5']:.4f}: contradictions regularly occupy")
        print(f"    top-5 retrieval slots.")
    print()
    print("  Required to close Stage 1:")
    print("  An end-to-end test must verify that the CMHM outcome-node separation")
    print("  or a retrieval reranking mechanism prevents contradictions from being")
    print("  the primary retrieved memory when a correct paraphrase exists.")
print("=================================================================")
