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
//! ## Corpus
//!
//! Three pair categories x 20 pairs = 60 total:
//!
//!   PARAPHRASE: same meaning, NO shared content words.
//!               hash-128d gets zero lexical signal; MiniLM must use semantics.
//!   TOPICAL:    same domain, 2-5 shared keywords.
//!               hash-128d scores via hash collisions; both embedders should excel.
//!   UNRELATED:  completely different domains -- the negative class.
//!
//! ## Acceptance Criteria (MiniLM Upgrade Gate)
//!
//!   hash-128d / paraphrase  d' <  0.5   (semantically blind by design)
//!   hash-128d / topical     d' >= 0.8   (keyword hashing must register)
//!   MiniLM    / paraphrase  d' >= 1.5   (must capture semantic meaning)
//!   MiniLM    / topical     d' >= 1.5   (must capture keyword overlap)
//!   MiniLM    / combined    d' >= 1.3   (overall discriminability gate)
//!
//! ## Run
//!
//!   # hash-128d baseline (always available):
//!   cargo test --test dprime_bench -- --ignored --nocapture
//!
//!   # MiniLM gate (MSVC toolchain required, downloads ~14 MB on first run):
//!   cargo test --test dprime_bench --features onnx -- --ignored --nocapture

use cmhm_core::{Embedder, HashEmbedder};

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

/// Semantic paraphrases with NO shared content words.
///
/// hash-128d expected: cosine ~0.00-0.15 (pure hash-collision noise, same as
/// unrelated pairs).  MiniLM expected: cosine ~0.70-0.95.
const PARAPHRASE_PAIRS: &[(&str, &str)] = &[
    // General language
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
    // Software / systems domain
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
];

/// Same-domain pairs sharing 2-5 keywords.
///
/// hash-128d expected: cosine ~0.30-0.75 (keyword hash collisions).
/// MiniLM expected: cosine ~0.80-0.98 (semantic + lexical overlap).
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
];

/// Completely unrelated domain pairs -- the negative class.
///
/// Both embedders expected: cosine ~-0.10 to +0.15.
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
];

// ---------------------------------------------------------------------------
// Metric computation
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
        / (xs.len() as f32 - 1.0); // Bessel-corrected (sample std-dev)
    var.sqrt()
}

/// Signal Detection Theory d' = (mu_pos - mu_neg) / sigma_pooled.
///
/// sigma_pooled = sqrt((sigma_p^2 + sigma_n^2) / 2).
/// Returns 0.0 for degenerate case (all similarities identical).
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
    pairs
        .iter()
        .map(|(a, b)| {
            let va = embedder.embed(a).expect("embed a");
            let vb = embedder.embed(b).expect("embed b");
            cosine(&va, &vb)
        })
        .collect()
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
        "      {label:<14}  n={:2}  mu={:+.4}  sigma={:.4}  min={:+.4}  max={:+.4}",
        sims.len(), m, s, min, max
    );
}

/// Embed all three categories, print the full benchmark report, and return
/// `(d'_paraphrase, d'_topical, d'_combined)`.
fn run_benchmark(embedder: &dyn Embedder) -> (f32, f32, f32) {
    let id = embedder.model_id();
    println!();
    println!("=================================================================");
    println!("  Embedder : {}  (dim = {})", id, embedder.dim());
    println!("  Corpus   : {} paraphrase + {} topical + {} unrelated pairs",
        PARAPHRASE_PAIRS.len(), TOPICAL_PAIRS.len(), UNRELATED_PAIRS.len());
    println!("=================================================================");

    let sims_para  = embed_pairs(embedder, PARAPHRASE_PAIRS);
    let sims_topo  = embed_pairs(embedder, TOPICAL_PAIRS);
    let sims_unrel = embed_pairs(embedder, UNRELATED_PAIRS);

    let mut sims_combined = sims_para.clone();
    sims_combined.extend_from_slice(&sims_topo);

    let dp_para = dprime(&sims_para,     &sims_unrel);
    let dp_topo = dprime(&sims_topo,     &sims_unrel);
    let dp_comb = dprime(&sims_combined, &sims_unrel);

    println!();
    println!("  Cosine similarity distributions (L2-normalized vectors):");
    print_distribution("paraphrase",  &sims_para);
    print_distribution("topical",     &sims_topo);
    print_distribution("unrelated",   &sims_unrel);
    print_distribution("combined",    &sims_combined);

    println!();
    println!("  d' (positive category vs unrelated negative class):");
    println!("      paraphrase  vs unrelated:  d' = {:+.4}", dp_para);
    println!("      topical     vs unrelated:  d' = {:+.4}", dp_topo);
    println!("      combined    vs unrelated:  d' = {:+.4}", dp_comb);
    println!();

    (dp_para, dp_topo, dp_comb)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Establishes the hash-128d discriminability baseline.
///
/// hash-128d is semantically blind by design: the embedding is a FNV-1a hash
/// projection, so two texts with the same meaning but different words map to
/// arbitrary positions in 128-dim space with cosine similarity indistinguishable
/// from unrelated texts.
///
/// Assertions:
///   1. d'_paraphrase < 0.5 -- semantically blind (expected ~0.0 +/- 0.3)
///   2. d'_topical   >= 0.8 -- keyword hash collisions must register
///
/// Run:
///   cargo test --test dprime_bench -- --ignored bench_hash128d_dprime --nocapture
#[test]
#[ignore]
fn bench_hash128d_dprime() {
    let embedder = HashEmbedder;
    let (dp_para, dp_topo, dp_comb) = run_benchmark(&embedder);

    println!("  -- Baseline assertions for hash-128d --");

    assert!(
        dp_para < 0.5,
        "hash-128d d'_paraphrase = {dp_para:.4} -- expected < 0.5 (semantically blind). \
         If this fails, the paraphrase corpus has unintended keyword overlap."
    );
    println!("  PASS  d'_paraphrase = {:+.4} < 0.5  (semantically blind, as expected)", dp_para);

    assert!(
        dp_topo >= 0.8,
        "hash-128d d'_topical = {dp_topo:.4} -- expected >= 0.8 (keyword hash collisions). \
         If this fails, the topical corpus lacks sufficient keyword overlap."
    );
    println!("  PASS  d'_topical    = {:+.4} >= 0.8  (keyword hashing works)", dp_topo);

    println!();
    println!("  -- MiniLM acceptance gate (run with --features onnx) --");
    println!("      d'_paraphrase must be >= 1.5   [baseline: {:+.4}]", dp_para);
    println!("      d'_topical    must be >= 1.5   [baseline: {:+.4}]", dp_topo);
    println!("      d'_combined   must be >= 1.3   [baseline: {:+.4}]", dp_comb);
    println!();
    println!("  MiniLM must show a gap of >= 1.0 d' on paraphrase pairs to justify");
    println!("  the 14 MB model download and MSVC toolchain requirement.");
}

/// MiniLM discriminability gate.
///
/// Requires --features onnx and MSVC toolchain. Downloads ~14 MB on first run.
/// Fails if MiniLM does not clear all three acceptance thresholds.
///
/// Run:
///   cargo test --test dprime_bench --features onnx -- --ignored bench_minilm_dprime --nocapture
#[cfg(feature = "onnx")]
#[test]
#[ignore]
fn bench_minilm_dprime() {
    let embedder = cmhm_core::embed::OnnxEmbedder::new()
        .expect("MiniLM init failed -- check MSVC toolchain and network access");

    let (dp_para, dp_topo, dp_comb) = run_benchmark(&embedder);

    println!("  -- MiniLM acceptance gate --");

    assert!(
        dp_para >= 1.5,
        "MiniLM d'_paraphrase = {dp_para:.4} -- must be >= 1.5 to justify replacing hash-128d. \
         Model fails to capture semantic meaning on pure paraphrase pairs."
    );
    println!("  PASS  d'_paraphrase = {:+.4} >= 1.5  (semantic meaning captured)", dp_para);

    assert!(
        dp_topo >= 1.5,
        "MiniLM d'_topical = {dp_topo:.4} -- must be >= 1.5. \
         Model underperforms on topical keyword-overlap pairs."
    );
    println!("  PASS  d'_topical    = {:+.4} >= 1.5  (keyword overlap captured)", dp_topo);

    assert!(
        dp_comb >= 1.3,
        "MiniLM d'_combined = {dp_comb:.4} -- overall gate requires >= 1.3."
    );
    println!("  PASS  d'_combined   = {:+.4} >= 1.3  (combined gate passed)", dp_comb);

    println!();
    println!("  MiniLM PASSES all acceptance gates.");
    println!("  Approved for deployment as the default embedder.");
}
