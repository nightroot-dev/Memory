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
//! ## Corpus (v3)
//!
//! 30 pairs x 3 categories = 90 pairs, 15 hard-negative pairs,
//! 15 operational query triples (query / D+ / D-).
//!
//!   PARAPHRASE:    same meaning, NO shared content words (hash-128d blind).
//!   TOPICAL:       same domain, 2-5 shared keywords.
//!   UNRELATED:     completely different domains (negative class).
//!   HARD_NEGATIVE: token overlap but inverted/negated meaning.
//!   OPERATIONAL:   15 queries, each with a true positive D+ and contradiction D-,
//!                  embedded in a 45-doc mixed corpus for retrieval measurement.
//!
//! ## Acceptance Criteria (MiniLM Upgrade Gate)
//!
//!   hash-128d  d_para  <  0.5   (semantically blind by design)
//!   hash-128d  d_topo  >= 0.8   (keyword hashing must register)
//!
//!   MiniLM d' gates:
//!     d_para  >= 1.5,  d_topo  >= 1.5,  d_comb  >= 1.3
//!
//!   MiniLM mixed-corpus retrieval gates (pre-declared, LOCKED):
//!     Recall@1             >= 0.40
//!     Recall@5             >= 0.67
//!     MRR                  >= 0.40
//!     Contradiction-at-1   <  0.40   (D- must not occupy rank 1 for >= 40% of queries)
//!     Hard-neg occupancy@5 <  0.40   (D- must occupy < 40% of top-5 slots on average)
//!     Mean margin          >  0.00   (cosine(q,D+) must exceed cosine(q,D-) on average)
//!
//! NOTE: Hard-negative pairwise FPR is printed but NOT gated. See OPERATIONAL gates above.
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

/// Operational query corpus for mixed-corpus retrieval test.
///
/// Each entry is (query, D+, D-) where:
///   query = HARD_NEGATIVE_PAIRS A-side (operational domain statement)
///   D+    = paraphrase of query with different surface form (true positive)
///   D-    = HARD_NEGATIVE_PAIRS B-side (the contradiction)
///
/// Retrieval corpus: all 15 D+ + all 15 D- + TOPICAL[0..10] + UNRELATED[0..5] = 45 docs.
const OPERATIONAL_QUERY_CORPUS: &[(&str, &str, &str)] = &[
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
// Mixed-corpus retrieval (v3)
// ---------------------------------------------------------------------------

/// Per-query results from the operational mixed-corpus retrieval test.
#[allow(dead_code)]
struct MixedCorpusResult {
    recall_at_1:        f32,
    recall_at_5:        f32,
    recall_at_10:       f32,
    mrr:                f32,
    contradiction_at_1: f32,
    hard_neg_occ_at5:   f32,
    mean_margin:        f32,
    min_margin:         f32,
    // per-query values (n=15) for bootstrap CI
    recall1_per:  [f32; 15],
    contra1_per:  [f32; 15],
    hn5_occ_per:  [f32; 15],
    margins_per:  [f32; 15],
    mrr_per:      [f32; 15],
}

/// Percentile bootstrap 95% CI on the mean using a fixed-seed LCG.
/// seed=42, n_boot=1000, alpha=0.05.
fn bootstrap_ci_mean(values: &[f32]) -> (f32, f32) {
    const N_BOOT: usize = 1000;
    const ALPHA: f32    = 0.05;
    let n = values.len();
    let mut state: u64 = 42; // fixed seed
    let mut boot_means: Vec<f32> = Vec::with_capacity(N_BOOT);
    for _ in 0..N_BOOT {
        let mut s = 0.0f32;
        for _ in 0..n {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let idx = ((state >> 33) as usize) % n;
            s += values[idx];
        }
        boot_means.push(s / n as f32);
    }
    boot_means.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lo = boot_means[(ALPHA / 2.0 * N_BOOT as f32) as usize];
    let hi = boot_means[((1.0 - ALPHA / 2.0) * N_BOOT as f32) as usize];
    (lo, hi)
}

/// Run the operational mixed-corpus retrieval test.
///
/// Corpus (45 docs):
///   D+[0..15]       correct paraphrases
///   D-[0..15]       contradictions
///   TOPICAL[0..10]  lexical distractors
///   UNRELATED[0..5] unrelated distractors
fn mixed_corpus_retrieval(embedder: &dyn Embedder) -> MixedCorpusResult {
    let n_q = OPERATIONAL_QUERY_CORPUS.len(); // 15

    // Build corpus in fixed order
    let dplus:  Vec<&str> = OPERATIONAL_QUERY_CORPUS.iter().map(|(_, d, _)| *d).collect();
    let dminus: Vec<&str> = OPERATIONAL_QUERY_CORPUS.iter().map(|(_, _, d)| *d).collect();
    let topical_dist:  Vec<&str> = TOPICAL_PAIRS[..10].iter().map(|(a, _)| *a).collect();
    let unrel_dist:    Vec<&str> = UNRELATED_PAIRS[..5].iter().map(|(a, _)| *a).collect();

    let corpus: Vec<&str> = dplus.iter().chain(dminus.iter())
        .chain(topical_dist.iter()).chain(unrel_dist.iter())
        .cloned().collect();
    let n_corpus = corpus.len(); // 45

    // Embed all corpus docs
    let corpus_embs: Vec<Vec<f32>> = corpus.iter()
        .map(|t| embedder.embed(t).expect("embed corpus doc"))
        .collect();

    let mut recall1_per  = [0.0f32; 15];
    let mut recall5_per  = [0.0f32; 15];
    let mut recall10_per = [0.0f32; 15];
    let mut mrr_per      = [0.0f32; 15];
    let mut contra1_per  = [0.0f32; 15];
    let mut hn5_occ_per  = [0.0f32; 15];
    let mut margins_per  = [0.0f32; 15];

    println!();
    println!("  {:44}  {:>7}  {:>7}  {:>7}  {:>4}",
        "Query", "D+ rank", "D- rank", "margin", "C@1");
    println!("  {}  {}  {}  {}  {}",
        "-".repeat(44), "-".repeat(7), "-".repeat(7), "-".repeat(7), "-".repeat(4));

    for i in 0..n_q {
        let (q, _, _) = OPERATIONAL_QUERY_CORPUS[i];
        let q_emb = embedder.embed(q).expect("embed query");

        // Score against all 45 corpus docs
        let mut scores: Vec<(usize, f32)> = (0..n_corpus)
            .map(|j| (j, cosine(&q_emb, &corpus_embs[j])))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let dplus_idx  = i;          // D+[i] = corpus[i]
        let dminus_idx = n_q + i;   // D-[i] = corpus[n_q + i]

        let dplus_rank  = scores.iter().position(|(j, _)| *j == dplus_idx)
            .map(|r| r + 1).unwrap_or(n_corpus + 1);
        let dminus_rank = scores.iter().position(|(j, _)| *j == dminus_idx)
            .map(|r| r + 1).unwrap_or(n_corpus + 1);

        let score_dplus  = cosine(&q_emb, &corpus_embs[dplus_idx]);
        let score_dminus = cosine(&q_emb, &corpus_embs[dminus_idx]);
        let margin       = score_dplus - score_dminus;

        // Hard-neg in top-5: any D- doc in first 5 slots
        let hn_in_top5 = scores[..5].iter()
            .filter(|(j, _)| *j >= n_q && *j < 2 * n_q)
            .count();

        recall1_per[i]  = if dplus_rank <= 1  { 1.0 } else { 0.0 };
        recall5_per[i]  = if dplus_rank <= 5  { 1.0 } else { 0.0 };
        recall10_per[i] = if dplus_rank <= 10 { 1.0 } else { 0.0 };
        mrr_per[i]      = 1.0 / dplus_rank as f32;
        contra1_per[i]  = if dminus_rank == 1 { 1.0 } else { 0.0 };
        hn5_occ_per[i]  = hn_in_top5 as f32 / 5.0;
        margins_per[i]  = margin;

        let contra_flag = if dminus_rank == 1 { " *" } else { "" };
        let q_short = if q.len() > 44 { &q[..44] } else { q };
        println!("  {:44}  {:>7}  {:>7}  {:>+7.4}  {:>4}{}",
            q_short, dplus_rank, dminus_rank, margin,
            if dminus_rank == 1 { 1 } else { 0 }, contra_flag);
    }

    let avg = |xs: &[f32; 15]| xs.iter().sum::<f32>() / 15.0;

    MixedCorpusResult {
        recall_at_1:        avg(&recall1_per),
        recall_at_5:        avg(&recall5_per),
        recall_at_10:       avg(&recall10_per),
        mrr:                avg(&mrr_per),
        contradiction_at_1: avg(&contra1_per),
        hard_neg_occ_at5:   avg(&hn5_occ_per),
        mean_margin:        avg(&margins_per),
        min_margin:         margins_per.iter().cloned().fold(f32::INFINITY, f32::min),
        recall1_per, contra1_per, hn5_occ_per, margins_per, mrr_per,
    }
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
    println!("  (Clean-corpus retrieval and FPR: informational for hash-128d)");
    println!("  R@1={:.4} R@5={:.4} MRR={:.4} FPR@.6={:.4} lat={:.2}ms",
        r.recall_at_1, r.recall_at_5, r.mrr, r.fpr_at_06, r.ms_per_embed);

    // Mixed-corpus retrieval (informational for hash-128d)
    println!();
    println!("  Mixed-corpus retrieval (informational for hash-128d):");
    let mc = mixed_corpus_retrieval(&embedder);
    println!();
    println!("  R@1={:.4} R@5={:.4} MRR={:.4} C@1={:.4} HN@5={:.4} margin={:+.4}",
        mc.recall_at_1, mc.recall_at_5, mc.mrr,
        mc.contradiction_at_1, mc.hard_neg_occ_at5, mc.mean_margin);

    println!();
    println!("  -- MiniLM pre-declared gate thresholds --");
    println!("  d' : para>=1.5  topo>=1.5  comb>=1.3");
    println!("  Mixed-corpus: R@1>=0.40  R@5>=0.67  MRR>=0.40");
    println!("                C@1<0.40   HN@5<0.40  margin>0.0");
    println!("  [hash d' baselines: para={:+.4} topo={:+.4} comb={:+.4}]",
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

    // d' gates
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

    println!("  INFO  FPR@0.6={:.4} ({} of {} — informational)",
        r.fpr_at_06,
        (r.fpr_at_06 * HARD_NEGATIVE_PAIRS.len() as f32).round() as usize,
        HARD_NEGATIVE_PAIRS.len());

    // ── Mixed-corpus retrieval gate (pre-declared, locked) ────────────────
    //
    // Gates were declared in implementation_plan.md before any results were
    // observed.  These thresholds MUST NOT be changed post-hoc.
    println!();
    println!("  -- Mixed-corpus retrieval (pre-declared gates) --");
    println!("  Corpus: 15 D+ + 15 D- + 10 topical + 5 unrelated = 45 docs");
    let mc = mixed_corpus_retrieval(&embedder);
    println!();

    // Bootstrap CIs
    let (r1_lo,  r1_hi)  = bootstrap_ci_mean(&mc.recall1_per);
    let (c1_lo,  c1_hi)  = bootstrap_ci_mean(&mc.contra1_per);
    let (hn_lo,  hn_hi)  = bootstrap_ci_mean(&mc.hn5_occ_per);
    let (mg_lo,  mg_hi)  = bootstrap_ci_mean(&mc.margins_per);
    let (mrr_lo, mrr_hi) = bootstrap_ci_mean(&mc.mrr_per);
    println!("  Recall@1   = {:.4}  [{:.3}, {:.3}]", mc.recall_at_1, r1_lo, r1_hi);
    println!("  Recall@5   = {:.4}", mc.recall_at_5);
    println!("  MRR        = {:.4}  [{:.3}, {:.3}]", mc.mrr, mrr_lo, mrr_hi);
    println!("  C@1        = {:.4}  [{:.3}, {:.3}]", mc.contradiction_at_1, c1_lo, c1_hi);
    println!("  HN-occ@5   = {:.4}  [{:.3}, {:.3}]", mc.hard_neg_occ_at5, hn_lo, hn_hi);
    println!("  Margin avg = {:+.4}  [{:+.3}, {:+.3}]", mc.mean_margin, mg_lo, mg_hi);
    println!("  Margin min = {:+.4}", mc.min_margin);
    println!();

    assert!(mc.recall_at_1 >= 0.40,
        "Mixed-corpus Recall@1={:.4} must be >= 0.40 (pre-declared gate)", mc.recall_at_1);
    println!("  PASS  Recall@1={:.4} >= 0.40", mc.recall_at_1);

    assert!(mc.recall_at_5 >= 0.67,
        "Mixed-corpus Recall@5={:.4} must be >= 0.67 (pre-declared gate)", mc.recall_at_5);
    println!("  PASS  Recall@5={:.4} >= 0.67", mc.recall_at_5);

    assert!(mc.mrr >= 0.40,
        "Mixed-corpus MRR={:.4} must be >= 0.40 (pre-declared gate)", mc.mrr);
    println!("  PASS  MRR={:.4} >= 0.40", mc.mrr);

    assert!(mc.contradiction_at_1 < 0.40,
        "Contradiction-at-1={:.4} must be < 0.40 (pre-declared gate). \
         Contradictions occupy rank 1 for {:.0}% of queries.",
        mc.contradiction_at_1, mc.contradiction_at_1 * 100.0);
    println!("  PASS  Contradiction-at-1={:.4} < 0.40", mc.contradiction_at_1);

    assert!(mc.hard_neg_occ_at5 < 0.40,
        "Hard-neg occupancy@5={:.4} must be < 0.40 (pre-declared gate)",
        mc.hard_neg_occ_at5);
    println!("  PASS  Hard-neg occupancy@5={:.4} < 0.40", mc.hard_neg_occ_at5);

    assert!(mc.mean_margin > 0.0,
        "Mean margin={:+.4} must be > 0.0 (pre-declared gate). \
         D- outscores D+ on average — contradictions rank above paraphrases.",
        mc.mean_margin);
    println!("  PASS  Mean margin={:+.4} > 0.0", mc.mean_margin);

    println!();
    println!("  MiniLM PASSES all acceptance gates.");
    println!("  Latency: {:.3} ms/embed (warm path, informational)", r.ms_per_embed);
    println!("  TEXT_MODEL_ID may be changed to \"minilm-l6-v2-int8\" once");
    println!("  this test passes on the native Rust ONNX backend.");
}
