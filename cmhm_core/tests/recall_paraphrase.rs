// cmhm_core/tests/recall_paraphrase.rs
// Run (hash-128d): cargo test --package cmhm_core --test recall_paraphrase -- --nocapture
// Run (MiniLM):    cargo +stable-x86_64-pc-windows-msvc test --package cmhm_core --test recall_paraphrase --features onnx -- --nocapture
//
// 200-pair corpus. Noise floor: ±0.5pp per pair flip (vs ±8.3pp on n=12).
// Categories: 20 domains × ~10 pairs each.
// Exact-match pairs (cos≈1.0) serve as sanity check — must be 100%.
//
// Uses default_embedder() — automatically picks OnnxEmbedder (MiniLM) when
// compiled with --features onnx, else falls back to HashEmbedder (FNV hash-128d).

use cmhm_core::embed::{default_embedder, Embedder};
use std::sync::OnceLock;

static EMBEDDER: OnceLock<Box<dyn Embedder>> = OnceLock::new();

fn get_embedder() -> &'static dyn Embedder {
    EMBEDDER.get_or_init(|| default_embedder().expect("embedder init failed")).as_ref()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>().clamp(-1.0, 1.0)
}

fn sim(a: &str, b: &str) -> f32 {
    let emb = get_embedder();
    let ea = emb.embed(a).expect("embed failed");
    let eb = emb.embed(b).expect("embed failed");
    cosine(&ea, &eb)
}

#[test]
fn paraphrase_recall_report() {
    // Each entry: (stored_text, query_text, category)
    // Pairs are true paraphrases — same meaning, different words.
    let pairs: &[(&str, &str, &str)] = &[

        // ── Build / compile errors (10 pairs) ──────────────────────────────
        ("cargo build failed linker error",        "compilation error missing symbol",          "build"),
        ("cargo build failed linker error",        "link failed cannot find library",           "build"),
        ("cargo build failed",                     "compilation failed with errors",            "build"),
        ("build step failed",                      "compile error occurred",                    "build"),
        ("undefined reference to main",            "linker cannot find entry point",            "build"),
        ("cannot find crate for std",              "missing standard library dependency",       "build"),
        ("error type mismatch expected i32",       "wrong type passed expected integer",        "build"),
        ("unused variable warning in rust",        "variable declared but never read",          "build"),
        ("dependency version conflict cargo",      "incompatible crate versions in lockfile",   "build"),
        ("stack overflow during compilation",      "recursive macro expansion limit exceeded",  "build"),

        // ── Auth / API errors (10 pairs) ───────────────────────────────────
        ("api returned 401 unauthorized",          "authentication failed bad credentials",     "auth"),
        ("http request failed connection refused", "network error server unreachable",          "auth"),
        ("jwt token expired signature invalid",    "bearer token no longer valid",              "auth"),
        ("oauth2 refresh token rejected",          "access token renewal failed",               "auth"),
        ("api key rate limit exceeded",            "too many requests quota exhausted",         "auth"),
        ("ssl certificate verification failed",    "tls handshake rejected invalid cert",       "auth"),
        ("forbidden 403 insufficient permission",  "access denied missing privilege",           "auth"),
        ("api gateway timeout upstream",           "backend service did not respond",           "auth"),
        ("cors policy blocked request",            "cross origin request denied by server",     "auth"),
        ("session cookie expired login again",     "user session timed out reauthenticate",     "auth"),

        // ── Database errors (10 pairs) ─────────────────────────────────────
        ("database connection pool exhausted",     "no available db connections",               "database"),
        ("postgres deadlock detected rolled back", "transaction deadlock two queries blocked",  "database"),
        ("query timeout exceeded 30 seconds",      "sql statement took too long cancelled",     "database"),
        ("unique constraint violation duplicate",  "duplicate key value violates unique index", "database"),
        ("foreign key constraint failed",          "referenced row does not exist",             "database"),
        ("disk full cannot write to database",     "database storage capacity reached",         "database"),
        ("migration failed column already exists", "schema migration duplicate column error",   "database"),
        ("null value in column violates not null", "required field missing cannot be null",     "database"),
        ("too many connections server refused",    "max client connections reached postgres",   "database"),
        ("replication lag too high standby",       "replica falling behind primary database",   "database"),

        // ── Networking (10 pairs) ──────────────────────────────────────────
        ("dns resolution failed unknown host",     "hostname not found cannot resolve",         "network"),
        ("tcp connection timed out port 443",      "socket connect timeout no response",        "network"),
        ("packet loss detected high latency",      "network unstable dropping packets",         "network"),
        ("http 502 bad gateway upstream error",    "proxy received invalid response backend",   "network"),
        ("ssh connection reset by peer",           "remote closed the connection unexpectedly", "network"),
        ("firewall rule blocking outbound traffic","egress blocked by security group policy",   "network"),
        ("network interface down no carrier",      "ethernet link disconnected",                "network"),
        ("grpc deadline exceeded rpc failed",      "remote call timed out exceeded deadline",   "network"),
        ("vpn tunnel dropped reconnecting",        "virtual private network disconnected",      "network"),
        ("load balancer health check failing",     "backend not responding to probes",          "network"),

        // ── Test failures (10 pairs) ───────────────────────────────────────
        ("test failed assertion error",            "unit test panicked unexpected result",      "test"),
        ("cargo test failed",                      "test suite failure all tests failed",       "test"),
        ("integration test failed timeout",        "end to end test timed out",                "test"),
        ("flaky test intermittent failure",        "test passes sometimes fails randomly",      "test"),
        ("test fixture setup failed database",     "beforeAll hook threw exception",            "test"),
        ("snapshot mismatch test failed",          "rendered output differs from expected",     "test"),
        ("coverage below threshold 80 percent",    "code coverage insufficient failing gate",   "test"),
        ("race condition test nondeterministic",   "concurrent test outcome unpredictable",     "test"),
        ("mock not called expected one invocation","stub was never invoked assertion failed",   "test"),
        ("test binary panicked segfault",          "test process crashed signal 11",            "test"),

        // ── Config / provider (10 pairs) ──────────────────────────────────
        ("nvidia provider model config",           "ai model provider configuration",           "config"),
        ("opencode config provider setup",         "model provider setup configuration",        "config"),
        ("provider not responding timeout",        "model api request timed out",               "config"),
        ("missing environment variable DATABASE_URL","required env var not set",               "config"),
        ("config file not found yaml missing",     "configuration file does not exist",         "config"),
        ("invalid json in config file parse error","malformed configuration cannot parse",      "config"),
        ("feature flag disabled in production",    "toggle turned off prod environment",        "config"),
        ("wrong port configured 8080 expected",    "service binding wrong address",             "config"),
        ("api endpoint url misconfigured",         "base url pointing to wrong host",           "config"),
        ("secret manager key not found vault",     "credentials not available in keystore",     "config"),

        // ── Memory / heap (10 pairs) ───────────────────────────────────────
        ("out of memory killed by oom killer",     "process terminated insufficient memory",    "memory"),
        ("heap allocation failed enomem",          "cannot allocate memory on heap",            "memory"),
        ("memory leak detected valgrind",          "unreleased heap blocks growing over time",  "memory"),
        ("stack overflow recursive function",      "call stack depth exceeded",                 "memory"),
        ("buffer overflow detected asan",          "write past end of allocated buffer",        "memory"),
        ("use after free undefined behavior",      "dangling pointer accessed freed memory",    "memory"),
        ("garbage collector pause too long",       "gc stop the world latency spike",           "memory"),
        ("shared memory segment full",             "ipc memory region capacity exceeded",       "memory"),
        ("swap usage critical system slow",        "paging to disk memory pressure high",       "memory"),
        ("null pointer dereference crash",         "nil pointer access caused panic",           "memory"),

        // ── CPU / performance (10 pairs) ──────────────────────────────────
        ("cpu usage 100 percent process stuck",    "process spinning consuming all cores",      "cpu"),
        ("thread pool exhausted blocking",         "all worker threads busy cannot enqueue",    "cpu"),
        ("high load average system sluggish",      "system overloaded too many processes",      "cpu"),
        ("hot loop tight spin wasting cycles",     "busy wait burning cpu unnecessarily",       "cpu"),
        ("context switch overhead too high",       "too many threads causing scheduler churn",  "cpu"),
        ("simd instruction not supported cpu",     "avx512 feature missing on this processor",  "cpu"),
        ("process priority too low starving",      "task not getting scheduled nicely value",   "cpu"),
        ("cache miss rate elevated performance",   "l3 cache thrashing slowing execution",      "cpu"),
        ("profiler shows function hot path",       "most cpu time spent in this function",      "cpu"),
        ("floating point exception nan inf",       "arithmetic overflow invalid float result",  "cpu"),

        // ── Disk / storage (10 pairs) ──────────────────────────────────────
        ("disk full no space left on device",      "filesystem at 100 percent capacity",        "disk"),
        ("inode limit reached cannot create file", "no inodes available filesystem full",       "disk"),
        ("file permission denied cannot write",    "access denied insufficient permissions",    "disk"),
        ("file not found no such file directory",  "path does not exist cannot open",           "disk"),
        ("disk io timeout read failed",            "storage device not responding",             "disk"),
        ("raid array degraded drive failed",       "disk array member offline rebuild needed",  "disk"),
        ("backup failed insufficient space",       "snapshot creation aborted disk full",       "disk"),
        ("file lock contention cannot acquire",    "another process holds exclusive lock",      "disk"),
        ("corrupt file checksum mismatch",         "data integrity error invalid hash",         "disk"),
        ("nfs mount stale file handle",            "remote filesystem handle expired",          "disk"),

        // ── Security (10 pairs) ────────────────────────────────────────────
        ("sql injection detected blocked",         "malicious query input sanitization failed", "security"),
        ("xss cross site scripting vulnerability", "reflected script injection in response",    "security"),
        ("privilege escalation attempt blocked",   "unauthorized elevation to root prevented",  "security"),
        ("cve critical vulnerability in library",  "known exploit in dependency needs patch",   "security"),
        ("brute force login attempt detected",     "repeated failed password attempts blocked", "security"),
        ("secrets exposed in git commit",          "api key accidentally committed to repo",    "security"),
        ("container running as root user",         "pod security policy violation not rootless","security"),
        ("insecure tls version 1.0 detected",      "outdated ssl protocol should upgrade",      "security"),
        ("csrf token missing invalid request",     "cross site request forgery protection failed","security"),
        ("dependency audit failed known cve",      "vulnerable package version in use",         "security"),

        // ── Deployment / CI-CD (10 pairs) ──────────────────────────────────
        ("deployment failed rollback initiated",   "release failed reverting to previous",      "deploy"),
        ("pipeline failed at build stage",         "ci job failed cannot proceed",              "deploy"),
        ("docker image pull rate limit",           "container registry throttling requests",    "deploy"),
        ("helm upgrade failed chart error",        "kubernetes deployment chart rejected",      "deploy"),
        ("pod crashloopbackoff restarting",        "container keeps restarting crash loop",     "deploy"),
        ("rolling update stuck unavailable pods",  "deployment rollout blocked unhealthy",      "deploy"),
        ("artifact upload failed registry",        "build artifact push to repo failed",        "deploy"),
        ("canary deployment error high error rate","new version producing too many errors",      "deploy"),
        ("blue green switch failed traffic",       "production traffic routing failed",          "deploy"),
        ("terraform apply failed state lock",      "infrastructure deploy blocked state issue", "deploy"),

        // ── Containers / orchestration (10 pairs) ──────────────────────────
        ("kubernetes node not ready taint",        "worker node unhealthy removed from pool",   "k8s"),
        ("pod evicted low memory node pressure",   "container killed due to resource pressure", "k8s"),
        ("image pull backoff imagepullsecret",     "cannot pull docker image auth failed",      "k8s"),
        ("configmap not found pod startup",        "missing kubernetes configuration volume",   "k8s"),
        ("service mesh sidecar injection failed",  "envoy proxy container not injected",        "k8s"),
        ("horizontal pod autoscaler not scaling",  "hpa cannot scale cpu metric missing",       "k8s"),
        ("pvc pending no storage class",           "persistent volume claim not bound",         "k8s"),
        ("liveness probe failed container killed", "health check timeout pod restarted",        "k8s"),
        ("resource quota exceeded namespace",      "namespace limits reached cannot schedule",  "k8s"),
        ("etcd cluster unavailable apiserver",     "control plane storage not accessible",      "k8s"),

        // ── Observability / monitoring (10 pairs) ──────────────────────────
        ("alert firing high error rate",           "error percentage threshold breached",       "observability"),
        ("metric scrape failed prometheus",        "monitoring endpoint not reachable",         "observability"),
        ("log volume too high dropping",           "logging pipeline overwhelmed losing data",  "observability"),
        ("trace sampling rate too low",            "distributed tracing missing spans",         "observability"),
        ("dashboard no data grafana",              "metrics not appearing in visualization",    "observability"),
        ("span context propagation failed",        "trace correlation header not forwarded",    "observability"),
        ("alert silenced notification missed",     "pagerduty mute suppressed alert",           "observability"),
        ("histogram bucket overflow",              "metric cardinality too high",               "observability"),
        ("slo breach error budget exhausted",      "reliability target violated",               "observability"),
        ("log parsing failed regex mismatch",      "structured log format unexpected",          "observability"),

        // ── Git / version control (10 pairs) ───────────────────────────────
        ("merge conflict cannot auto merge",       "git conflict requires manual resolution",   "git"),
        ("push rejected non fast forward",         "remote has diverged force push needed",     "git"),
        ("branch protection rule violated",        "direct push to main blocked policy",        "git"),
        ("rebase failed conflicting changes",      "cherry pick resulted in conflict",          "git"),
        ("submodule not initialized missing",      "git submodule not checked out",             "git"),
        ("git lfs object not found",               "large file storage pointer broken",         "git"),
        ("stash pop conflict working tree",        "unstash failed uncommitted changes",        "git"),
        ("tag already exists cannot create",       "duplicate git tag in repository",           "git"),
        ("detached head state warning",            "not on any branch current head detached",  "git"),
        ("gitignore not working file tracked",     "file should be ignored still committed",    "git"),

        // ── Language / runtime errors (10 pairs) ───────────────────────────
        ("python importerror module not found",    "cannot import package missing library",     "runtime"),
        ("node module not found require failed",   "npm package missing install first",         "runtime"),
        ("java class not found classloader",       "jvm cannot load class missing jar",         "runtime"),
        ("rust borrow checker lifetime error",     "reference outlives borrowed value",         "runtime"),
        ("python type error wrong argument",       "function received incorrect type",          "runtime"),
        ("segmentation fault core dumped",         "memory access violation process crashed",   "runtime"),
        ("ruby gem conflict incompatible",         "bundler cannot resolve gem versions",       "runtime"),
        ("go goroutine leak not terminated",       "goroutine never exits accumulating",        "runtime"),
        ("scala implicits not found compile",      "implicit value missing in scope",           "runtime"),
        ("kotlin npe null pointer exception",      "null value accessed kotlin runtime",        "runtime"),

        // ── Performance / latency (10 pairs) ───────────────────────────────
        ("p99 latency spiking 2000ms",             "tail latency increased two seconds",        "perf"),
        ("throughput dropped requests per second", "rps degraded lower than baseline",          "perf"),
        ("slow query detected full table scan",    "query not using index performance issue",   "perf"),
        ("connection pool wait time high",         "clients waiting for available connection",  "perf"),
        ("jit compilation slow warmup",            "application slow to start cold cache",      "perf"),
        ("response time degraded after deploy",    "latency regression introduced in release",  "perf"),
        ("batch job taking too long hours",        "data processing exceeding time budget",     "perf"),
        ("cdn cache miss rate 80 percent",         "cache not serving requests origin hit",     "perf"),
        ("event loop blocked nodejs",              "synchronous operation stalling io loop",    "perf"),
        ("lock contention high mutex wait",        "threads blocked waiting for lock",          "perf"),

        // ── Cloud / infrastructure (10 pairs) ──────────────────────────────
        ("aws s3 access denied bucket policy",     "s3 object not accessible iam permission",  "cloud"),
        ("gcp quota exceeded compute instances",   "google cloud resource limit reached",       "cloud"),
        ("azure subscription suspended billing",   "cloud account billing issue service stopped","cloud"),
        ("ec2 instance terminated unexpected",     "virtual machine stopped without warning",   "cloud"),
        ("cloudformation stack rollback failed",   "infrastructure stack deployment failed",    "cloud"),
        ("lambda timeout exceeded 15 minutes",     "serverless function execution too long",    "cloud"),
        ("rds failover in progress unavailable",   "managed database switching to replica",     "cloud"),
        ("vpc peering route misconfigured",        "network path between accounts broken",      "cloud"),
        ("iam role missing policy permission",     "service account lacks required access",     "cloud"),
        ("cloud nat gateway unhealthy",            "outbound internet routing failing",         "cloud"),

        // ── ML / AI / model (10 pairs) ─────────────────────────────────────
        ("model inference timeout gpu",            "neural network prediction took too long",   "ml"),
        ("cuda out of memory gpu batch",           "gpu ran out of vram reduce batch size",     "ml"),
        ("model weights file not found",           "checkpoint missing cannot load model",      "ml"),
        ("nan loss during training exploded",      "gradient explosion training unstable",      "ml"),
        ("embedding dimension mismatch",           "vector size incompatible with index",       "ml"),
        ("tokenizer vocab file missing",           "cannot tokenize text model not loaded",     "ml"),
        ("onnx runtime session failed",            "model inference engine error",              "ml"),
        ("model returned empty response",          "llm output was blank no text generated",    "ml"),
        ("hallucination detected factual error",   "model produced incorrect information",      "ml"),
        ("context window exceeded token limit",    "prompt too long for model max tokens",      "ml"),

        // ── OS / system (10 pairs) ─────────────────────────────────────────
        ("kernel panic system halted",             "operating system crash fatal error",        "os"),
        ("system call failed eperm operation",     "syscall blocked by kernel permission",      "os"),
        ("open file descriptor limit reached",     "too many open files ulimit exceeded",       "os"),
        ("cgroup memory limit exceeded killed",    "process killed resource cgroup limit",      "os"),
        ("zombie process cannot be reaped",        "child process lingering not cleaned up",    "os"),
        ("shared library soname mismatch",         "dynamic linker cannot load library version","os"),
        ("signal sigterm received graceful stop",  "process asked to terminate cleanly",        "os"),
        ("hugepages not available performance",    "transparent hugepages disabled os",         "os"),
        ("entropy pool depleted urandom slow",     "system randomness source exhausted",        "os"),
        ("ntp clock skew too large drift",         "system time drifted out of sync",           "os"),

        // ── Distributed systems (10 pairs) ─────────────────────────────────
        ("split brain scenario two leaders",       "dual leader election conflict cluster",     "distributed"),
        ("quorum lost cannot commit",              "majority nodes unavailable write blocked",  "distributed"),
        ("raft election timeout no leader",        "cluster leadership election failed",        "distributed"),
        ("message queue full dropping events",     "kafka partition at capacity backpressure",  "distributed"),
        ("circuit breaker open service down",      "downstream service failing requests rejected","distributed"),
        ("eventual consistency lag data stale",    "read returning outdated data replica lag",   "distributed"),
        ("distributed lock expired timeout",       "lease expired lost distributed mutex",      "distributed"),
        ("gossip protocol node not converging",    "cluster membership not propagating",        "distributed"),
        ("idempotency key duplicate request",      "same operation submitted twice detected",   "distributed"),
        ("partition tolerance network split",      "nodes cannot communicate cluster divided",  "distributed"),

        // ── Exact sanity (must be ~1.0) ────────────────────────────────────
        ("cargo build failed linker error",        "cargo build failed linker error",           "exact"),
        ("the model returned empty response",      "the model returned empty response",         "exact"),
        ("database connection pool exhausted",     "database connection pool exhausted",        "exact"),
        ("kubernetes node not ready taint",        "kubernetes node not ready taint",           "exact"),
        ("jwt token expired signature invalid",    "jwt token expired signature invalid",       "exact"),
    ];

    let threshold_strict = 0.50_f32;
    let threshold_loose  = 0.20_f32;

    let mut total       = 0usize;
    let mut strict_pass = 0usize;
    let mut loose_pass  = 0usize;
    let mut exact_pass  = 0usize;
    let mut exact_total = 0usize;
    let mut low_scores: Vec<(&str, &str, f32)> = Vec::new();

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║     CMHM Paraphrase Recall Audit — hash-128d embedder — n=200      ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    for (a, b, cat) in pairs {
        let s = sim(a, b);

        if *cat == "exact" {
            exact_total += 1;
            if s > 0.99 { exact_pass += 1; }
        } else {
            total += 1;
            if s >= threshold_strict { strict_pass += 1; }
            if s >= threshold_loose  { loose_pass  += 1; }
            if s < threshold_loose   { low_scores.push((a, b, s)); }
        }
    }

    // Print worst-scoring pairs (diagnostic)
    low_scores.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
    println!("── Lowest-scoring pairs (below loose threshold {threshold_loose:.2}) ──");
    for (a, b, s) in low_scores.iter().take(10) {
        println!("  {s:.3}  \"{a}\"");
        println!("         vs \"{b}\"");
    }

    let model = get_embedder().model_id();
    let strict_pct = strict_pass as f32 / total as f32 * 100.0;
    let loose_pct  = loose_pass  as f32 / total as f32 * 100.0;

    println!("\n══════════════════════════════════════════════════════════════════════");
    println!("  Embedder                  : {model}");
    println!("  Corpus size               : {total} paraphrase pairs + {exact_total} exact");
    println!("  Exact match sanity        : {exact_pass}/{exact_total} (must be 100%)");
    println!("  Paraphrase recall@strict  (cos≥{threshold_strict:.2}): {strict_pass}/{total} = {strict_pct:.0}%");
    println!("  Paraphrase recall@loose   (cos≥{threshold_loose:.2}): {loose_pass}/{total} = {loose_pct:.0}%");
    println!("  Noise floor               : ±{:.1}pp per pair flip (1/{} corpus)",
        100.0 / total as f32, total);
    println!("══════════════════════════════════════════════════════════════════════");
    println!();
    if model.contains("hash") {
        println!("  🔴 hash-128d: FNV projection has zero semantic awareness.");
        println!("     Run with --features onnx for MiniLM (target ~40% strict recall)");
    } else {
        println!("  Embedder: {model}");
        if strict_pct >= 35.0 {
            println!("  ✅ Strict recall meets target (≥35%)");
        } else {
            println!("  🟡 Strict recall below 35% target — consider BGE or reranker");
        }
    }
    println!();

    // Strict assertions
    assert_eq!(exact_pass, exact_total,
        "Exact-match pairs must score >0.99 — embedder is broken if this fails");
    assert!(total >= 190,
        "Corpus size check — expected ~200 paraphrase pairs, got {total}");
    // Note: we do NOT assert a minimum recall% here — the number is diagnostic,
    // not a pass/fail gate. The test always passes to allow CI to run.
}
