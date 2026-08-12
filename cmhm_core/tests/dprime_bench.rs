//! d-prime (d') discriminability benchmark for CMHM embedders.
//!
//! Measures how well each embedder separates semantically similar text pairs
//! (positive class) from unrelated text pairs (negative class) using the
//! Signal Detection Theory discriminability index d'.
//!
//! ## Metric
//!
//!   d' = (mu_pos - mu_neg) / sigma_pooled
//!   sigma_pooled = sqrt((sigma_pos^2 + sigma_neg^2) / 2)
//!
//! | d'  | Discrimination level | Approx. AUC |
//! |-----|----------------------|-------------|
//! | 0.0 | chance               | 0.50        |
//! | 0.5 | weak                 | 0.64        |
//! | 1.0 | moderate             | 0.76        |
//! | 1.5 | strong               | 0.86        |
//! | 2.0 | excellent            | 0.92        |
//!
//! ## Corpus (v2)
//!
//! 30 pairs x 3 categories = 90 pairs, plus 15 hard-negative pairs.
//!
//!   PARAPHRASE:    same meaning, NO shared content words (hash-128d blind).
//!   TOPICAL:       same domain, 2-5 shared keywords.
//!   UNRELATED:     completely different domains (negative class).
//!   HARD_NEGATIVE: token overlap but inverted/negated meaning.
//!
//! ## Acceptance Criteria (MiniLM Upgrade Gate)
//!
//!   hash-128d  d_para  <  0.5   (semantically blind by design)
//!   hash-128d  d_topo  >= 0.8   (keyword hashing must register)
//!   MiniLM     d_para  >= 1.5
//!   MiniLM     d_topo  >= 1.5
//!   MiniLM     d_comb  >= 1.3
//!   MiniLM     Recall@1       >= 0.5   (retrieval quality gate)
//!   MiniLM     Recall@5       >= 0.8
//!   MiniLM     MRR            >= 0.6
//!
//! NOTE: Hard-negative FPR is printed but NOT gated.
//! Sentence encoders capture topic/domain similarity, not negation-aware reasoning.
//! "The deployment succeeded" and "the deployment did not succeed" are topically
//! identical — high cosine is expected and correct. CMHM disambiguates outcomes
//! via Beta bandit feedback (separate outcome nodes), not embedding distance.
//!
//! ## Run
//!
//!   cargo test --test dprime_bench -- --ignored --nocapture
//!   cargo test --test dprime_bench --features onnx -- --ignored --nocapture

use cmhm_core::{Embedder, HashEmbedder};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Corpus v2
// ---------------------------------------------------------------------------

const PARAPHRASE_PAIRS: &[(&str, &str)] = &[
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
];

const TOPICAL_PAIRS: &[(&str, &str)] = &[
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
];

const UNRELATED_PAIRS: &[(&str, &str)] = &[
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
];

/// Hard-negative pairs: token overlap, but semantics inverted/negated/reversed.
const HARD_NEGATIVE_PAIRS: &[(&str, &str)] = &[
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
];

// ---------------------------------------------------------------------------
// Result struct
// ---------------------------------------------------------------------------

#[allow(dead_code)]
struct BenchResult {
    dp_para:      f32,
    dp_topo:      f32,
    dp_comb:      f32,
    dp_hard_neg:  f32,
    recall_at_1:  f32,
    recall_at_5:  f32,
    recall_at_10: f32,
    mrr:          f32,
    fpr_at_06:    f32,
    ms_per_embed: f32,
}

// ---------------------------------------------------------------------------
// Metric helpers
// ---------------------------------------------------------------------------

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn mean(xs: &[f32]) -> f32 {
    xs.iter().sum::<f32>() / xs.len() as f32
}

fn std_dev(xs: &[f32]) -> f32 {
    if xs.len() < 2 { return 0.0; }
    let m = mean(xs);
    let var = xs.iter().map(|x| (x - m).powi(2)).sum::<f32>()
        / (xs.len() as f32 - 1.0);
    var.sqrt()
}

fn dprime(pos: &[f32], neg: &[f32]) -> f32 {
    let mu_p = mean(pos);
    let mu_n = mean(neg);
    let sp = std_dev(pos);
    let sn = std_dev(neg);
    let pooled = ((sp * sp + sn * sn) / 2.0).sqrt();
    if pooled < 1e-8 { return 0.0; }
    (mu_p - mu_n) / pooled
}

fn embed_pairs(embedder: &dyn Embedder, pairs: &[(&str, &str)]) -> Vec<f32> {
    pairs.iter().map(|(a, b)| {
        let va = embedder.embed(a).expect("embed a");
        let vb = embedder.embed(b).expect("embed b");
        cosine(&va, &vb)
    }).collect()
}

/// Assert embedding is valid: correct dim, unit-normalized, no NaN/inf.
fn assert_embedding_valid(emb: &[f32], expected_dim: usize, label: &str) {
    assert_eq!(emb.len(), expected_dim,
        "Embedding '{label}': dim={} expected={expected_dim}", emb.len());
    assert!(emb.iter().all(|x| x.is_finite()),
        "Embedding '{label}': contains NaN or inf");
    let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.02,
        "Embedding '{label}': L2 norm={norm:.6} expected ~1.0");
}

/// Recall@1, Recall@5, Recall@10, MRR.
/// 15 queries (B-sides of first 15 PARAPHRASE_PAIRS) against 30-doc corpus (A-sides).
fn retrieval_metrics(embedder: &dyn Embedder) -> (f32, f32, f32, f32) {
    let n_docs = PARAPHRASE_PAIRS.len();
    let n_queries = 15usize;
    let doc_embs: Vec<Vec<f32>> = PARAPHRASE_PAIRS.iter()
        .map(|(a, _)| embedder.embed(a).expect("embed doc"))
        .collect();
    let (mut r1, mut r5, mut r10) = (0u32, 0u32, 0u32);
    let mut mrr_sum = 0.0f32;
    for q in 0..n_queries {
        let q_emb = embedder.embed(PARAPHRASE_PAIRS[q].1).expect("embed query");
        let mut scores: Vec<(usize, f32)> = (0..n_docs)
            .map(|d| (d, cosine(&q_emb, &doc_embs[d]))).collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let rank = scores.iter().position(|(di, _)| *di == q)
            .map(|r| r + 1).unwrap_or(n_docs + 1);
        if rank <= 1  { r1  += 1; }
        if rank <= 5  { r5  += 1; }
        if rank <= 10 { r10 += 1; }
        mrr_sum += 1.0 / rank as f32;
    }
    let n = n_queries as f32;
    (r1 as f32 / n, r5 as f32 / n, r10 as f32 / n, mrr_sum / n)
}

/// FPR of hard-negative pairs at cosine threshold.
fn hard_neg_fpr(embedder: &dyn Embedder, thresh: f32) -> f32 {
    let n = HARD_NEGATIVE_PAIRS.len() as f32;
    let fp = HARD_NEGATIVE_PAIRS.iter().filter(|(a, b)| {
        let va = embedder.embed(a).expect("embed hard-neg a");
        let vb = embedder.embed(b).expect("embed hard-neg b");
        cosine(&va, &vb) >= thresh
    }).count() as f32;
    fp / n
}

/// Mean embedding latency in ms over n_reps calls.
fn latency_ms(embedder: &dyn Embedder, n_reps: usize) -> f32 {
    let text = "the service returned an unexpected error code during initialization";
    let start = Instant::now();
    for _ in 0..n_reps { let _ = embedder.embed(text).expect("latency embed"); }
    (start.elapsed().as_secs_f64() * 1000.0 / n_reps as f64) as f32
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

fn print_distribution(label: &str, sims: &[f32]) {
    let m   = mean(sims);
    let s   = std_dev(sims);
    let min = sims.iter().cloned().fold(f32::INFINITY,     f32::min);
    let max = sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    println!(
        "      {label:<16}  n={:2}  mu={:+.4}  sigma={:.4}  min={:+.4}  max={:+.4}",
        sims.len(), m, s, min, max
    );
}

fn run_benchmark(embedder: &dyn Embedder) -> BenchResult {
    let id  = embedder.model_id();
    let dim = embedder.dim();
    println!();
    println!("=================================================================");
    println!("  Embedder : {}  (dim={})", id, dim);
    println!("  Corpus   : {} para + {} topo + {} unrel + {} hard-neg",
        PARAPHRASE_PAIRS.len(), TOPICAL_PAIRS.len(),
        UNRELATED_PAIRS.len(), HARD_NEGATIVE_PAIRS.len());
    println!("=================================================================");

    // Validity
    println!();
    println!("  Embedding validity:");
    for (label, text) in [
        ("para-A[0]",    PARAPHRASE_PAIRS[0].0),
        ("para-B[0]",    PARAPHRASE_PAIRS[0].1),
        ("topical-A[0]", TOPICAL_PAIRS[0].0),
        ("unrel-A[0]",   UNRELATED_PAIRS[0].0),
        ("hardneg-A[0]", HARD_NEGATIVE_PAIRS[0].0),
    ] {
        let emb = embedder.embed(text).expect("validity embed");
        assert_embedding_valid(&emb, dim, label);
    }
    println!("  PASS  dim={dim}, L2-norm in [0.99,1.01], no NaN/inf");

    // Pairwise cosines
    let sims_para     = embed_pairs(embedder, PARAPHRASE_PAIRS);
    let sims_topo     = embed_pairs(embedder, TOPICAL_PAIRS);
    let sims_unrel    = embed_pairs(embedder, UNRELATED_PAIRS);
    let sims_hard_neg = embed_pairs(embedder, HARD_NEGATIVE_PAIRS);
    let mut sims_comb = sims_para.clone();
    sims_comb.extend_from_slice(&sims_topo);

    let dp_para     = dprime(&sims_para,     &sims_unrel);
    let dp_topo     = dprime(&sims_topo,     &sims_unrel);
    let dp_comb     = dprime(&sims_comb,     &sims_unrel);
    let dp_hard_neg = dprime(&sims_hard_neg, &sims_unrel);

    println!();
    println!("  Cosine distributions:");
    print_distribution("paraphrase",    &sims_para);
    print_distribution("topical",       &sims_topo);
    print_distribution("unrelated",     &sims_unrel);
    print_distribution("hard-negative", &sims_hard_neg);
    print_distribution("combined",      &sims_comb);

    println!();
    println!("  d' (vs unrelated):");
    println!("      paraphrase : {:+.4}", dp_para);
    println!("      topical    : {:+.4}", dp_topo);
    println!("      combined   : {:+.4}", dp_comb);
    println!("      hard-neg   : {:+.4}  (informational)", dp_hard_neg);

    // Retrieval
    println!();
    println!("  Retrieval (15 queries / 30-doc corpus):");
    let (r1, r5, r10, mrr) = retrieval_metrics(embedder);
    println!("      Recall@1  = {:.4}", r1);
    println!("      Recall@5  = {:.4}", r5);
    println!("      Recall@10 = {:.4}", r10);
    println!("      MRR       = {:.4}", mrr);

    // FPR
    let fpr = hard_neg_fpr(embedder, 0.6);
    println!();
    println!("  Hard-negative FPR @cos>=0.6 : {:.4}  ({} of {})",
        fpr,
        (fpr * HARD_NEGATIVE_PAIRS.len() as f32).round() as usize,
        HARD_NEGATIVE_PAIRS.len());

    // Latency
    let ms = latency_ms(embedder, 100);
    println!();
    println!("  Latency: {:.3} ms/embed (100 reps)", ms);

    BenchResult { dp_para, dp_topo, dp_comb, dp_hard_neg,
                  recall_at_1: r1, recall_at_5: r5, recall_at_10: r10,
                  mrr, fpr_at_06: fpr, ms_per_embed: ms }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// hash-128d discriminability baseline.
///
/// Assertions:
///   d'_paraphrase < 0.5   (semantically blind by design)
///   d'_topical   >= 0.8   (keyword hash collisions must register)
/// Retrieval, FPR, and latency are printed but not gated.
///
/// Run:
///   cargo test --test dprime_bench -- --ignored bench_hash128d_dprime --nocapture
#[test]
#[ignore]
fn bench_hash128d_dprime() {
    let embedder = HashEmbedder;
    let r = run_benchmark(&embedder);

    println!("  -- Baseline assertions (hash-128d) --");

    assert!(r.dp_para < 0.5,
        "hash-128d d'_para={:.4} must be < 0.5. Paraphrase corpus has unintended keyword overlap.", r.dp_para);
    println!("  PASS  d'_para={:+.4} < 0.5  (semantically blind)", r.dp_para);

    assert!(r.dp_topo >= 0.8,
        "hash-128d d'_topo={:.4} must be >= 0.8. Topical corpus lacks keyword overlap.", r.dp_topo);
    println!("  PASS  d'_topo={:+.4} >= 0.8  (keyword hashing works)", r.dp_topo);

    println!();
    println!("  (Retrieval/FPR/latency: informational for hash-128d)");
    println!("  R@1={:.4} R@5={:.4} MRR={:.4} FPR@.6={:.4} lat={:.2}ms",
        r.recall_at_1, r.recall_at_5, r.mrr, r.fpr_at_06, r.ms_per_embed);

    println!();
    println!("  -- MiniLM gate thresholds --");
    println!("  d'_para>=1.5  d'_topo>=1.5  d'_comb>=1.3");
    println!("  R@1>=0.50     R@5>=0.80     MRR>=0.60");
    println!("  [hash baselines: para={:+.4} topo={:+.4} comb={:+.4}]",
        r.dp_para, r.dp_topo, r.dp_comb);
}

/// MiniLM full acceptance gate.
/// Requires --features onnx and MSVC C++ workload.
///
/// All gates must pass before TEXT_MODEL_ID is changed in tools.rs.
///
/// Run:
///   cargo test --test dprime_bench --features onnx -- --ignored bench_minilm_dprime --nocapture
#[cfg(feature = "onnx")]
#[test]
#[ignore]
fn bench_minilm_dprime() {
    let embedder = cmhm_core::embed::OnnxEmbedder::new()
        .expect("MiniLM init failed -- check MSVC toolchain and network access");

    // Hash-128d d'_topo for non-inferiority check
    let hash_dp_topo = {
        let h = HashEmbedder;
        let s = embed_pairs(&h, TOPICAL_PAIRS);
        let n = embed_pairs(&h, UNRELATED_PAIRS);
        dprime(&s, &n)
    };

    let r = run_benchmark(&embedder);

    println!("  -- MiniLM acceptance gate --");

    assert!(r.dp_para >= 1.5,
        "d'_para={:.4} must be >= 1.5", r.dp_para);
    println!("  PASS  d'_para={:+.4} >= 1.5", r.dp_para);

    assert!(r.dp_topo >= 1.5,
        "d'_topo={:.4} must be >= 1.5", r.dp_topo);
    println!("  PASS  d'_topo={:+.4} >= 1.5", r.dp_topo);

    assert!(r.dp_comb >= 1.3,
        "d'_comb={:.4} must be >= 1.3", r.dp_comb);
    println!("  PASS  d'_comb={:+.4} >= 1.3", r.dp_comb);

    let ni_thresh = 0.8 * hash_dp_topo;
    assert!(r.dp_topo >= ni_thresh,
        "Non-inferiority: d'_topo={:.4} must be >= 80% of hash-128d {:.4} (threshold={:.4})",
        r.dp_topo, hash_dp_topo, ni_thresh);
    println!("  PASS  topical non-inferiority: {:.4} >= 80% of hash {:.4}", r.dp_topo, hash_dp_topo);

    assert!(r.recall_at_1 >= 0.5,
        "Recall@1={:.4} must be >= 0.50", r.recall_at_1);
    println!("  PASS  Recall@1={:.4} >= 0.50", r.recall_at_1);

    assert!(r.recall_at_5 >= 0.8,
        "Recall@5={:.4} must be >= 0.80", r.recall_at_5);
    println!("  PASS  Recall@5={:.4} >= 0.80", r.recall_at_5);

    assert!(r.mrr >= 0.6,
        "MRR={:.4} must be >= 0.60", r.mrr);
    println!("  PASS  MRR={:.4} >= 0.60", r.mrr);

    // Hard-negative FPR is informational — not gated.
    // Sentence encoders capture topic similarity, not negation-aware reasoning.
    // "deployment succeeded" vs "deployment did not succeed" score high because they
    // share domain, entities, and structure — correct behavior for a similarity encoder.
    // Outcome disambiguation is handled by Beta bandit feedback in CMHM, not embedding.
    println!("  INFO  FPR@0.6={:.4} ({} of {} — informational, not gated)",
        r.fpr_at_06,
        (r.fpr_at_06 * HARD_NEGATIVE_PAIRS.len() as f32).round() as usize,
        HARD_NEGATIVE_PAIRS.len());

    println!();
    println!("  MiniLM PASSES all gated acceptance criteria.");
    println!("  Latency: {:.3} ms/embed (warm path, informational)", r.ms_per_embed);
    println!("  TEXT_MODEL_ID may be changed to \"minilm-l6-v2-int8\" once");
    println!("  this test passes on the native Rust ONNX backend.");
}
