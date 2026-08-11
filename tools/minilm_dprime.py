"""
MiniLM-L6-v2 d' benchmark — same corpus as cmhm_core/tests/dprime_bench.rs.

Uses sentence-transformers (all-MiniLM-L6-v2) to compute cosine similarities
for 60 fixed pairs (20 paraphrase / 20 topical / 20 unrelated) and reports d'
values against the hash-128d baseline measured in the Rust benchmark.

Run:
    python minilm_dprime.py

hash-128d baseline (from Rust bench_hash128d_dprime):
    d'_paraphrase = +0.1931   (semantically blind)
    d'_topical    = +3.3996   (keyword hashing: strong)
    d'_combined   = +1.1966

MiniLM acceptance gate:
    d'_paraphrase >= 1.5
    d'_topical    >= 1.5
    d'_combined   >= 1.3
"""

import math
import numpy as np
from sentence_transformers import SentenceTransformer

# ---------------------------------------------------------------------------
# Corpus — identical to dprime_bench.rs
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
]

# ---------------------------------------------------------------------------
# Metric
# ---------------------------------------------------------------------------

def embed_cosines(model, pairs):
    """Embed both sides of each pair, return list of cosine similarities."""
    a_texts = [p[0] for p in pairs]
    b_texts = [p[1] for p in pairs]
    A = model.encode(a_texts, normalize_embeddings=True, show_progress_bar=False)
    B = model.encode(b_texts, normalize_embeddings=True, show_progress_bar=False)
    return [float(A[i] @ B[i]) for i in range(len(pairs))]


def dprime(pos, neg):
    """SDT d' = (mu_pos - mu_neg) / sigma_pooled."""
    mu_p, mu_n = np.mean(pos), np.mean(neg)
    s_p = np.std(pos, ddof=1) if len(pos) > 1 else 0.0
    s_n = np.std(neg, ddof=1) if len(neg) > 1 else 0.0
    pooled = math.sqrt((s_p ** 2 + s_n ** 2) / 2)
    return float((mu_p - mu_n) / pooled) if pooled > 1e-8 else 0.0


def report_dist(label, sims):
    a = np.array(sims)
    print(f"      {label:<14}  n={len(a):2d}  mu={a.mean():+.4f}  "
          f"sigma={a.std(ddof=1):.4f}  min={a.min():+.4f}  max={a.max():+.4f}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    print("Loading all-MiniLM-L6-v2 (downloads ~90 MB on first run) ...")
    model = SentenceTransformer("all-MiniLM-L6-v2")
    dim = model.get_sentence_embedding_dimension()
    print(f"Loaded. dim={dim}")
    print()

    sims_para  = embed_cosines(model, PARAPHRASE_PAIRS)
    sims_topo  = embed_cosines(model, TOPICAL_PAIRS)
    sims_unrel = embed_cosines(model, UNRELATED_PAIRS)
    sims_comb  = sims_para + sims_topo

    dp_para = dprime(sims_para, sims_unrel)
    dp_topo = dprime(sims_topo, sims_unrel)
    dp_comb = dprime(sims_comb, sims_unrel)

    print("=================================================================")
    print(f"  Embedder : all-MiniLM-L6-v2  (dim={dim})")
    print(f"  Corpus   : {len(PARAPHRASE_PAIRS)} paraphrase + "
          f"{len(TOPICAL_PAIRS)} topical + {len(UNRELATED_PAIRS)} unrelated pairs")
    print("=================================================================")
    print()
    print("  Cosine similarity distributions (L2-normalized vectors):")
    report_dist("paraphrase",  sims_para)
    report_dist("topical",     sims_topo)
    report_dist("unrelated",   sims_unrel)
    report_dist("combined",    sims_comb)
    print()
    print("  d' (positive category vs unrelated negative class):")
    print(f"      paraphrase  vs unrelated:  d' = {dp_para:+.4f}")
    print(f"      topical     vs unrelated:  d' = {dp_topo:+.4f}")
    print(f"      combined    vs unrelated:  d' = {dp_comb:+.4f}")
    print()

    # Gate assessment
    GATE = {
        "paraphrase": (dp_para, 1.5),
        "topical":    (dp_topo, 1.5),
        "combined":   (dp_comb, 1.3),
    }
    print("  -- MiniLM acceptance gate --")
    all_pass = True
    for cat, (dp, thr) in GATE.items():
        verdict = "PASS" if dp >= thr else "FAIL"
        if verdict == "FAIL":
            all_pass = False
        print(f"  {verdict}  d'_{cat:<12} = {dp:+.4f}  (threshold >= {thr})")
    print()
    if all_pass:
        print("  MiniLM PASSES all acceptance gates.")
        print("  Approved for deployment as the default embedder.")
    else:
        print("  MiniLM FAILS one or more gates.")

    # Comparison table
    print()
    print("  -- Comparison: hash-128d baseline vs MiniLM --")
    print(f"  {'Category':<12}  {'hash-128d':>10}  {'MiniLM':>10}  {'Gap':>8}  Gate")
    baselines = {"paraphrase": 0.1931, "topical": 3.3996, "combined": 1.1966}
    for cat, (dp, thr) in GATE.items():
        base = baselines[cat]
        gap  = dp - base
        print(f"  {cat:<12}  {base:>+10.4f}  {dp:>+10.4f}  {gap:>+8.4f}  >= {thr}")
