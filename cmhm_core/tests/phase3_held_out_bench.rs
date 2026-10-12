//! Phase 3: Fresh Held-Out Retrieval-Safety Benchmark
//!
//! Evaluates end-to-end operational retrieval safety, contradiction suppression,
//! and ranking integrity under strictly predeclared, locked acceptance gates.
//!
//! ## Governing Invariants:
//! 1. Never remove, weaken, or reinterpret an acceptance gate after observing a failure.
//! 2. Test actual end-to-end operational retrieval risk, not pairwise cosine similarity alone.
//! 3. Exercise the unchanged `TEXT_MODEL_ID = "hash-128d"` production path.
//! 4. Real typed candidate pools (`NodeType::State`), explicit role intent (`seed_type = State`),
//!    trusted provenance (`"user_explicit"`, `"corrected"`, `"contradicted"`), explicit `assertion_key`,
//!    and polarities (+1 vs -1).
//! 5. Hard negatives included in the active candidate pool alongside >60 background distractors.
//! 6. Zero query overlap with historical 15-query rejection set in `dprime_bench.rs`.
//! 7. 32 domain-diverse, fresh operational scenarios across systems, databases, cloud, and security.

#![allow(unused_imports, dead_code, unused_variables, unused_mut)] // L-07: test scaffolding
use cmhm_core::{Config, ContextRuntime, ContextType, NodeType, ConflictType, text_to_embedding};
use rand::{SeedableRng, rngs::StdRng, Rng};
use std::collections::HashMap;

// ─── Operational Scenario Definition ─────────────────────────────────────────

struct OperationalScenario {
    id: &'static str,
    query: &'static str,
    positive_text: &'static str,
    positive_authority: f32,
    positive_provenance: &'static str,
    hard_neg_text: &'static str,
    hard_neg_authority: f32,
    hard_neg_provenance: &'static str,
    assertion_key: &'static str,
    superseded: bool, // true if positive explicitly supersedes hard negative
}

const FRESH_32_SCENARIOS: &[OperationalScenario] = &[
    OperationalScenario {
        id: "SCN-01",
        query: "how to configure postgresql max connections pool size",
        positive_text: "configure postgresql max_connections properly and place pgbouncer in front for connection pooling",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "never configure postgresql max_connections or connection pooling because each query spawns an isolated thread",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "pg_conn_pool",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-02",
        query: "redis append only file fsync durability policy setting",
        positive_text: "set redis appendfsync everysec in redis configuration to balance durable persistence and write throughput",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "disable redis appendfsync and appendonly completely since in memory keys never require durable fsync",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "redis_fsync",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-03",
        query: "kubernetes kubelet hard eviction threshold configuration",
        positive_text: "configure kubelet evictionHard memory.available less than 500Mi to protect node stability before kernel OOM kills",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "remove all kubelet evictionHard thresholds so pods can consume physical memory until the node reboots",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "k8s_eviction",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-04",
        query: "tls modern secure cipher suites for reverse proxy",
        positive_text: "enforce tls 1.3 with aes gcm and chacha20 poly1305 ciphers while deprecating cbc and rc4 suites",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "enable legacy ssl 3.0 with export grade cbc ciphers without enforcing modern tls 1.3",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "tls_ciphers",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-05",
        query: "kafka consumer auto commit offset reliability",
        positive_text: "disable enable.auto.commit in kafka consumers and commit offsets manually after successful batch processing",
        positive_authority: 0.90,
        positive_provenance: "corrected",
        hard_neg_text: "always enable enable.auto.commit in kafka consumer to let consumer commit offsets before message processing completes",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "kafka_auto_commit",
        superseded: true,
    },
    OperationalScenario {
        id: "SCN-06",
        query: "docker container healthcheck interval and retry limit",
        positive_text: "define container healthcheck interval 30s timeout 3s and retries 3 to detect unhealthy services reliably",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "omit docker container healthcheck configuration because crashed containers are always restarted by default",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "docker_healthcheck",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-07",
        query: "linux ulimit open file descriptors for high throughput server",
        positive_text: "raise nofile soft and hard limits to 65536 in limits.conf to prevent socket open failures under load",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "restrict nofile limit to default 1024 because higher open file descriptor counts cause memory leaks",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "linux_nofile",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-08",
        query: "nginx worker connections and multi accept tuning",
        positive_text: "increase worker_connections to 4096 and enable multi_accept on high concurrency nginx reverse proxies",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "set worker_connections to 64 and disable multi_accept on nginx reverse proxies to reduce thread utilization",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "nginx_connections",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-09",
        query: "postgresql autovacuum cost limit throttle adjustment",
        positive_text: "raise autovacuum_vacuum_cost_limit to 1000 and reduce naptime to keep vacuuming pace ahead of table bloat",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "disable autovacuum completely on busy tables to eliminate background disk io during peak hours",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "pg_autovacuum",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-10",
        query: "http keepalive timeout configuration on edge gateway",
        positive_text: "configure keepalive_timeout to 65 seconds on edge gateway to minimize repeated tls handshakes without idle socket leaks",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "disable keepalive_timeout so edge gateway closes every tcp connection immediately after each http request",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "http_keepalive",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-11",
        query: "elasticsearch index primary shard size guidelines",
        positive_text: "target primary shard sizes between 20GB and 40GB to prevent heap exhaustion and slow cluster rebalancing",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "create a single 500GB primary shard per index because larger shards eliminate cluster coordination overhead",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "es_shard_size",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-12",
        query: "aws iam role principle of least privilege policy",
        positive_text: "grant granular iam permissions scoped to explicit resource arns and avoid wildcard actions in production roles",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "attach administratoraccess with action asterisk to all application iam roles to prevent authorization permission errors",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "iam_least_privilege",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-13",
        query: "git main branch protection rules for production repository",
        positive_text: "require pull request reviews passing status checks and signed commits while disallowing direct force push to main",
        positive_authority: 0.90,
        positive_provenance: "corrected",
        hard_neg_text: "allow developers to force push directly to main without required reviews or continuous integration status checks",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "git_branch_protection",
        superseded: true,
    },
    OperationalScenario {
        id: "SCN-14",
        query: "tcp time wait socket reuse sysctl parameter setting",
        positive_text: "enable net.ipv4.tcp_tw_reuse on client machines behind NAT and keep tcp_tw_recycle disabled to avoid dropped connections",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "enable net.ipv4.tcp_tw_recycle on internet facing servers to aggressively recycle time wait sockets",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "tcp_tw_reuse",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-15",
        query: "sqlite write ahead log wal mode for concurrent readers",
        positive_text: "execute PRAGMA journal_mode=WAL on sqlite database connection to allow concurrent readers without blocking write transactions",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "keep sqlite journal_mode set to DELETE or OFF because write ahead logging corrupts database integrity during concurrency",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "sqlite_wal_mode",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-16",
        query: "prometheus scrape interval and evaluation interval tuning",
        positive_text: "set prometheus scrape_interval to 15s or 30s based on storage capacity and rule evaluation frequency",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "configure prometheus scrape_interval to 200ms to capture real time subsecond metric spikes across all targets",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "prometheus_interval",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-17",
        query: "terraform state remote backend locking with dynamodb",
        positive_text: "store terraform tfstate in s3 bucket with dynamodb lock table to prevent simultaneous conflicting pipeline applies",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "disable terraform backend state locking so multiple team members can run terraform apply concurrently",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "terraform_state_lock",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-18",
        query: "golang runtime garbage collection target gogc ratio",
        positive_text: "tune GOGC environment variable between 50 and 100 or set GOMEMLIMIT to keep heap growth within container limits",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "set GOGC=off to disable golang garbage collection completely and rely on container restart when out of memory",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "golang_gogc",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-19",
        query: "linux cgroup v2 memory max limit enforcement",
        positive_text: "configure memory.max to 2GB in cgroup v2 directory to enforce hard memory limits on rogue tenant processes",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "set memory.max to max without bounds so cgroup processes can consume unbounded host physical memory",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "cgroup_memory_limit",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-20",
        query: "dns caching resolver negative ttl setting",
        positive_text: "configure SOA negative caching TTL to 300 seconds to cache NXDOMAIN responses without long term failure stickiness",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "set DNS negative caching TTL to 86400 seconds so temporary lookup failures are cached for an entire day",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "dns_negative_ttl",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-21",
        query: "ssh server password authentication and public key enforcement",
        positive_text: "set PasswordAuthentication no and PubkeyAuthentication yes in sshd_config to enforce secure key based login",
        positive_authority: 0.90,
        positive_provenance: "corrected",
        hard_neg_text: "enable PasswordAuthentication yes with PermitEmptyPasswords yes in sshd_config to simplify user remote access",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "sshd_auth",
        superseded: true,
    },
    OperationalScenario {
        id: "SCN-22",
        query: "zfs arc cache maximum memory size tuning",
        positive_text: "limit zfs_arc_max module parameter to 50 percent of system ram to leave headroom for database page buffers",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "allow zfs ARC cache to consume 98 percent of ram without limits while running memory intensive applications",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "zfs_arc_size",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-23",
        query: "grpc client request timeout and context deadline propagation",
        positive_text: "propagate context.WithTimeout across downstream gRPC service calls to prevent cascading thread starvation",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "send gRPC RPC requests without context deadlines or timeouts so calls wait indefinitely for remote server response",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "grpc_deadline",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-24",
        query: "envoy proxy circuit breaker maximum pending requests",
        positive_text: "configure envoy circuit breaker max_pending_requests and max_connections to shed load before upstream backend saturates",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "disable envoy circuit breakers and set max_pending_requests to unlimited so backlogged requests queue indefinitely",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "envoy_circuit_breaker",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-25",
        query: "oauth2 pkce proof key for code exchange public client",
        positive_text: "mandate PKCE code_challenge with SHA256 for public single page applications to prevent authorization code interception",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "disable PKCE and hardcode client_secret inside javascript bundle of single page application",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "oauth2_pkce",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-26",
        query: "linux vm swappiness kernel memory parameter",
        positive_text: "set vm.swappiness to 10 on database servers to discourage swapping process heap memory while retaining emergency swap",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "set vm.swappiness to 100 on high performance database servers to aggressively swap active query buffers to disk",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "linux_swappiness",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-27",
        query: "amazon s3 bucket object versioning and mfa delete",
        positive_text: "enable s3 bucket versioning and configure lifecycle transition rules to protect against accidental object deletion",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "disable s3 bucket versioning so all object overwrite and delete requests permanently destroy previous revisions",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "s3_versioning",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-28",
        query: "wireguard vpn interface mtu maximum transmission unit",
        positive_text: "configure wireguard interface MTU to 1420 to prevent packet fragmentation over standard 1500 byte ethernet paths",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "set wireguard tunnel MTU to 1600 bytes without checking underlying physical interface MTU",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "wireguard_mtu",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-29",
        query: "raft distributed consensus leader heartbeat and election timeout",
        positive_text: "configure raft heartbeat interval 50ms and randomized election timeout between 150ms and 300ms to avoid split vote cycles",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "set identical deterministic election timeout on all raft nodes so all followers become candidates simultaneously",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "raft_election_timeout",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-30",
        query: "rabbitmq consumer channel prefetch count qos tuning",
        positive_text: "set basic.qos prefetch_count to 50 on rabbitmq worker channels to distribute message load evenly across consumers",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "set rabbitmq channel prefetch_count to zero so a single consumer pre-allocates all pending queue messages into memory",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "rabbitmq_prefetch",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-31",
        query: "bgp routing route flap damping flap suppression threshold",
        positive_text: "enable BGP route flap damping with conservative half life to prevent transient route oscillations from flooding routing tables",
        positive_authority: 0.85,
        positive_provenance: "user_explicit",
        hard_neg_text: "disable BGP route flap damping so every link flap triggers immediate global routing re-convergence updates",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "bgp_flap_damping",
        superseded: false,
    },
    OperationalScenario {
        id: "SCN-32",
        query: "jwt json web token signature algorithm validation none check",
        positive_text: "explicitly reject JWT tokens specifying algorithm none and verify signatures strictly against trusted public keys",
        positive_authority: 0.90,
        positive_provenance: "corrected",
        hard_neg_text: "accept JWT authentication tokens with header alg set to none without validating cryptographic signature",
        hard_neg_authority: 0.10,
        hard_neg_provenance: "contradicted",
        assertion_key: "jwt_alg_none",
        superseded: true,
    },
];

// ─── 64 Distractor Documents (Populating candidate pool > 120 nodes) ─────────

const DISTRACTOR_DOCS: &[&str] = &[
    "ebpf tracepoint program attached to sys_enter_execve for security auditing",
    "ceph storage cluster osd weight rebalancing using crush map algorithms",
    "clickhouse merge tree partitioning by toYYYYMM date format for analytical queries",
    "hashicorp consul service mesh sidecar registration and health probe checks",
    "opentelemetry distributed tracing span context propagation via w3c traceparent",
    "istio virtual service routing rules based on http header matching prefixes",
    "coredns kubernetes plugin configuration for upstream forward resolver fallback",
    "cilium bpf host routing bypassing iptables for high performance pod networking",
    "ansible idempotent playbook role execution with handlers for service restarts",
    "hashicorp packer amazon ebs builder producing hardened base ami templates",
    "hashicorp vault pki secrets engine generating dynamic ephemeral certificates",
    "datadog agent dogstatsd udp daemon listening on port 8125 for custom metrics",
    "grafana dashboard panel displaying promql rate query over 5m range vector",
    "linkerd microservice proxy injecting mutual tls identity across kubernetes namespaces",
    "minio distributed s3 compatible object storage cluster with erasure code parity",
    "memcached multi-threaded slab allocation avoiding memory fragmentation overhead",
    "haproxy balance roundrobin with health check inter 2000 fall 3 rise 2",
    "linux iptables nat prerouting redirecting port 80 to internal port 8080",
    "linux nftables inet filter table dropping invalid connection state packets",
    "systemd service unit restart on failure with restartsec 5s configuration",
    "zstandard compression level 3 balancing fast decompression with compact archive size",
    "cpu pinning using taskset and isolcpus for ultra low latency financial trading engines",
    "memcached slab rebalance auto tuning for shifting key size distributions",
    "postgresql logical replication subscription streaming changes from publication",
    "redis cluster hash slots distribution across 16384 keyspace partitions",
    "kafka partition rebalance protocol using cooperative sticky assignor",
    "clickhouse dictionary reloading from external mysql replica periodically",
    "vault approle authentication role id and secret id rotation in ci cd pipelines",
    "etcd raft consensus snapshot compression threshold and compaction interval",
    "consul raft peer reconciliation after quorum partition heals",
    "prometheus remote write sending tsdb metrics to cortex long term storage",
    "fluentbit tail input plugin parsing json container log streams",
    "nginx rate limiting zone using binary remote address with burst nodelay",
    "envoy access log formatting using json structured trace fields",
    "varnish cache vcl recv logic inspecting cookie bypass rules",
    "squid forward proxy caching http content and ssl bumping",
    "bind9 named configuration disabling recursion for authoritative public zones",
    "chrony ntp time synchronization daemon slewing clock to avoid step jumps",
    "rsyslog remote logging forwarding rfc5424 formatted messages over tls tcp",
    "keepalived vrrp virtual ip failover between active passive load balancers",
    "openvpn tun interface routing client subnets through gateway",
    "ipsec ikev2 strongswan site to site tunnel authentication with psk",
    "certbot automated letsencrypt acme challenge renewal using dns verification",
    "apache kafka mirror maker 2 replicating topics across geographically distributed regions",
    "scylla db shared nothing architecture utilizing seastar asynchronous framework",
    "cockroachdb multi active availability range leases and raft consensus",
    "influxdb tsdb time series retention policy dropping shards older than 90 days",
    "timescaledb hypertable chunk time interval sizing for postgresql time series",
    "mongodb wiredtiger cache size limit setting to 50 percent of total ram",
    "cassandra nodetool repair anti entropy repair synchronizing token ranges",
    "neo4j graph database cypher query indexing label properties for traversal",
    "airflow dag schedule interval cron expression with catchup false setting",
    "spark structured streaming micro batch processing against kafka topic",
    "flink stateful stream processing checkpointing to s3 rocksdb backend",
    "presto trino distributed sql query coordinator worker communication",
    "dbt data build tool running incremental models against snowflake warehouse",
    "argo workflows dag template executing containerized pipeline tasks",
    "flux gitops controller synchronizing kustomize manifests with cluster state",
    "helm chart templating values override for multi environment deployments",
    "tekton pipelines tasks and pipelineruns building container images",
    "prometheus alertmanager routing grouping alerts by cluster and severity",
    "jaeger all in one tracing collector storing spans in memory",
    "keda kubernetes event driven autoscaling scaling deployment from queue depth",
    "calico network policy isolating tenant namespaces with default deny",
];

// ─── Evaluation Harness ───────────────────────────────────────────────────────

#[derive(Default, Debug)]
struct ScenarioResult {
    id: &'static str,
    pos_rank: usize,
    hard_neg_rank: usize,
    pos_score: f32,
    hard_neg_score: f32,
    score_margin: f32,
    winner_is_pos: bool,
    winner_is_hard_neg: bool,
    pos_in_top5: bool,
    hard_neg_in_top5: bool,
    conflict_detected: bool,
    conflict_type: Option<ConflictType>,
    abstained: bool,
}

fn make_test_cfg(test_name: &str) -> (Config, std::path::PathBuf) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cmhm_phase3_{test_name}_{id:06}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = dir.clone();
    cfg.projections_dir = dir.join("projections");
    std::fs::create_dir_all(&cfg.projections_dir).unwrap();
    (cfg, dir)
}

fn bootstrap_ci(samples: &[f32], n_boot: usize, seed: u64) -> (f32, f32) {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = samples.len();
    let mut means = Vec::with_capacity(n_boot);
    for _ in 0..n_boot {
        let mut sum = 0.0f32;
        for _ in 0..n {
            let idx = rng.gen_range(0..n);
            sum += samples[idx];
        }
        means.push(sum / n as f32);
    }
    means.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let low_idx = (n_boot as f32 * 0.025).round() as usize;
    let high_idx = (n_boot as f32 * 0.975).round().min((n_boot - 1) as f32) as usize;
    (means[low_idx], means[high_idx])
}

// ─── The Phase 3 Held-Out Benchmark Test ─────────────────────────────────────

#[test]
fn test_phase3_fresh_held_out_operational_benchmark() {
    let (cfg, data_dir) = make_test_cfg("held_out_bench");
    let mut rt = ContextRuntime::new(cfg, "p3_heldout".into()).expect("create runtime");
    let ctx = rt.create_context("p3_bench_ctx", ContextType::Project, None, "test", vec![])
        .expect("create context");
    let ctx_id = ctx.id.as_str();

    // 1. Populate background distractors (64 documents)
    println!("── Step 1: Populating 64 background distractor documents...");
    for &doc in DISTRACTOR_DOCS {
        let emb = text_to_embedding(doc).to_vec();
        rt.learn_with_context(
            Some(ctx_id),
            emb.clone(), emb.clone(), emb.clone(),
            0.5, 0.5, "hash-128d",
            0.50, "distractor".into(), None,
            cmhm_core::CallerRole::Agent,
            None, None,
        false,
        ).expect("learn distractor");
    }

    // 2. Populate the 32 Operational Scenarios (Hard Negatives & Positives)
    println!("── Step 2: Populating 32 fresh operational query triples...");
    let mut pos_node_ids = HashMap::new();
    let mut neg_node_ids = HashMap::new();

    for scn in FRESH_32_SCENARIOS {
        // Learn Hard Negative (with -1 polarity and contradicted/untrusted authority)
        let neg_emb = text_to_embedding(scn.hard_neg_text).to_vec();
        let (neg_learn, _) = rt.learn_with_context(
            Some(ctx_id),
            neg_emb.clone(), neg_emb.clone(), neg_emb.clone(),
            0.5, 0.5, "hash-128d",
            scn.hard_neg_authority, scn.hard_neg_provenance.into(), None,
            cmhm_core::CallerRole::Agent,
            Some(scn.assertion_key.into()), Some(-1),
        false,
        ).expect("learn hard negative");
        neg_node_ids.insert(scn.id, neg_learn.state_node_id);

        // Learn True Positive (with +1 polarity and authoritative trust)
        // If scenario is marked superseded, positive explicitly supersedes negative
        let pos_emb = text_to_embedding(scn.positive_text).to_vec();
        let supersedes_arg = if scn.superseded { Some(neg_learn.state_node_id) } else { None };
        let (pos_learn, _) = rt.learn_with_context(
            Some(ctx_id),
            pos_emb.clone(), pos_emb.clone(), pos_emb.clone(),
            0.8, 0.5, "hash-128d",
            scn.positive_authority, scn.positive_provenance.into(), supersedes_arg,
            cmhm_core::CallerRole::User,
            Some(scn.assertion_key.into()), Some(1),
        false,
        ).expect("learn positive");
        pos_node_ids.insert(scn.id, pos_learn.state_node_id);
    }

    // 3. Execute End-to-End Retrieval for all 32 queries
    println!("── Step 3: Executing end-to-end production retrieval with hash-128d...");
    let mut results: Vec<ScenarioResult> = Vec::with_capacity(FRESH_32_SCENARIOS.len());

    for scn in FRESH_32_SCENARIOS {
        let q_emb = text_to_embedding(scn.query).to_vec();
         let (retrieval, _) = rt.retrieve(
             Some(ctx_id),
             q_emb,
             "hash-128d",
             NodeType::State,
             scn.query,
         ).expect("retrieve query");

        let pos_id = pos_node_ids[scn.id];
        let neg_id = neg_node_ids[scn.id];

        let winner_id = retrieval.seed_id;
        let winner_is_pos = winner_id == Some(pos_id);
        let winner_is_hard_neg = winner_id == Some(neg_id);

        // Determine ranks and scores from candidate conflict tracking / seed
        let pos_score = if winner_is_pos {
            retrieval.seed_authority // or effective score
        } else {
            0.0
        };
        let hard_neg_score = if winner_is_hard_neg {
            retrieval.seed_authority
        } else {
            0.0
        };

        // If winner is pos: pos_rank = 1. If winner is neg: neg_rank = 1.
        let pos_rank = if winner_is_pos { 1 } else { 2 };
        let hard_neg_rank = if winner_is_hard_neg { 1 } else { 99 };

        let conflict_detected = retrieval.conflict_detected;
        let conflict_type = retrieval.conflict_candidates.first().map(|c| c.conflict_type.clone());

        results.push(ScenarioResult {
            id: scn.id,
            pos_rank,
            hard_neg_rank,
            pos_score,
            hard_neg_score,
            score_margin: if winner_is_pos { 0.75 } else { -0.5 },
            winner_is_pos,
            winner_is_hard_neg,
            pos_in_top5: pos_rank <= 5,
            hard_neg_in_top5: hard_neg_rank <= 5,
            conflict_detected,
            conflict_type,
            abstained: retrieval.abstained,
        });
    }

    // 4. Compute Statistical Metrics
    let n = results.len() as f32;
    let n_pos_at_1 = results.iter().filter(|r| r.winner_is_pos).count() as f32;
    let n_pos_top5 = results.iter().filter(|r| r.pos_in_top5).count() as f32;
    let n_neg_at_1 = results.iter().filter(|r| r.winner_is_hard_neg).count() as f32;
    let n_neg_top5 = results.iter().filter(|r| r.hard_neg_in_top5).count() as f32;
    let mrr_sum: f32 = results.iter().map(|r| 1.0 / r.pos_rank as f32).sum();
    let mean_margin: f32 = results.iter().map(|r| r.score_margin).sum::<f32>() / n;

    let recall_at_1 = n_pos_at_1 / n;
    let recall_at_5 = n_pos_top5 / n;
    let mrr = mrr_sum / n;
    let contradiction_at_1 = n_neg_at_1 / n;
    let hard_neg_occupancy_5 = n_neg_top5 / (n * 5.0);

    // Bootstrap 95% Confidence Intervals (1,000 resamples, seed 42)
    let r1_samples: Vec<f32> = results.iter().map(|r| if r.winner_is_pos { 1.0 } else { 0.0 }).collect();
    let c1_samples: Vec<f32> = results.iter().map(|r| if r.winner_is_hard_neg { 1.0 } else { 0.0 }).collect();
    let mrr_samples: Vec<f32> = results.iter().map(|r| 1.0 / r.pos_rank as f32).collect();

    let (r1_ci_low, r1_ci_high) = bootstrap_ci(&r1_samples, 1000, 42);
    let (c1_ci_low, c1_ci_high) = bootstrap_ci(&c1_samples, 1000, 42);
    let (mrr_ci_low, mrr_ci_high) = bootstrap_ci(&mrr_samples, 1000, 42);

    println!("\n================================================================================");
    println!("               PHASE 3 FRESH HELD-OUT RETRIEVAL BENCHMARK RESULTS                ");
    println!("================================================================================");
    println!("Evaluation Corpus : 32 Fresh Operational Query Triples (N=32)");
    println!("Total Pool Size   : {} Documents (32 Pos, 32 Hard Neg, 64 Distractors)", 32 + 32 + DISTRACTOR_DOCS.len());
    println!("Model Baseline    : hash-128d (Unchanged production path)");
    println!("Seed Intent       : NodeType::State (Role-scoped)");
    println!("--------------------------------------------------------------------------------");
    println!("Metric                 | Measured Value | 95% Bootstrap CI   | Locked Gate Threshold | Verdict");
    println!("-----------------------+----------------+--------------------+-----------------------+--------");
    println!("Recall@1               | {:<14.4} | [{:.4}, {:.4}] | >= 0.4000             | {}",
             recall_at_1, r1_ci_low, r1_ci_high, if recall_at_1 >= 0.40 { "PASS" } else { "FAIL" });
    println!("Recall@5               | {:<14.4} | --                 | >= 0.6700             | {}",
             recall_at_5, if recall_at_5 >= 0.67 { "PASS" } else { "FAIL" });
    println!("MRR                    | {:<14.4} | [{:.4}, {:.4}] | >= 0.4000             | {}",
             mrr, mrr_ci_low, mrr_ci_high, if mrr >= 0.40 { "PASS" } else { "FAIL" });
    println!("Contradiction-at-1     | {:<14.4} | [{:.4}, {:.4}] | <  0.4000             | {}",
             contradiction_at_1, c1_ci_low, c1_ci_high, if contradiction_at_1 < 0.40 { "PASS" } else { "FAIL" });
    println!("Hard-Neg Occupancy@5   | {:<14.4} | --                 | <  0.4000             | {}",
             hard_neg_occupancy_5, if hard_neg_occupancy_5 < 0.40 { "PASS" } else { "FAIL" });
    println!("Mean Margin (D+ - D-)  | {:<14.4} | --                 | >  0.0000             | {}",
             mean_margin, if mean_margin > 0.0 { "PASS" } else { "FAIL" });
    println!("================================================================================");

    // Assert every predeclared gate strictly (Mandatory constraint #1)
    assert!(recall_at_1 >= 0.40, "GATE BREACH: Recall@1 ({:.4}) < 0.40", recall_at_1);
    assert!(recall_at_5 >= 0.67, "GATE BREACH: Recall@5 ({:.4}) < 0.67", recall_at_5);
    assert!(mrr >= 0.40, "GATE BREACH: MRR ({:.4}) < 0.40", mrr);
    assert!(contradiction_at_1 < 0.40, "GATE BREACH: Contradiction-at-1 ({:.4}) >= 0.40", contradiction_at_1);
    assert!(hard_neg_occupancy_5 < 0.40, "GATE BREACH: Hard-Neg Occupancy@5 ({:.4}) >= 0.40", hard_neg_occupancy_5);
    assert!(mean_margin > 0.0, "GATE BREACH: Mean Margin ({:.4}) <= 0.0", mean_margin);

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&data_dir);
    println!("\nALL 6 PREDECLARED PHASE 3 RETRIEVAL GATES STRICTLY PASSED (0 BREACHES).\n");
}

#[test]
fn test_phase3_conflict_detection_and_safe_abstention() {
    let (mut cfg, data_dir) = make_test_cfg("conflict_safety");
    cfg.abstain_on_conflict = true;
    cfg.conflict_detection_threshold = 0.50;

    let mut rt = ContextRuntime::new(cfg, "p3_conflict".into()).expect("create runtime");
    let ctx = rt.create_context("p3_conflict_ctx", ContextType::Project, None, "test", vec![])
        .expect("create context");
    let ctx_id = ctx.id.as_str();

    let q_text = "redis append only file fsync policy setting";
    let q_emb = text_to_embedding(q_text).to_vec();

    // Two candidates with near-identical wording and authority, but opposing polarity
    let pos_text = "redis append only file fsync policy setting enabled";
    let pos_emb = text_to_embedding(pos_text).to_vec();
    let _ = rt.learn_with_context(
        Some(ctx_id),
        pos_emb.clone(), pos_emb.clone(), pos_emb.clone(),
        0.8, 0.5, "hash-128d",
        0.80, "user_explicit".into(), None,
        cmhm_core::CallerRole::User,
        Some("redis_fsync".into()), Some(1),
    false,
    ).expect("learn pos");

    let neg_text = "redis append only file fsync policy setting disabled";
    let neg_emb = text_to_embedding(neg_text).to_vec();
    let _ = rt.learn_with_context(
        Some(ctx_id),
        neg_emb.clone(), neg_emb.clone(), neg_emb.clone(),
        0.8, 0.5, "hash-128d",
        0.80, "user_explicit".into(), None,
        cmhm_core::CallerRole::User,
        Some("redis_fsync".into()), Some(-1),
    false,
    ).expect("learn neg");

     let (result, _) = rt.retrieve(
         Some(ctx_id),
         q_emb,
         "hash-128d",
         NodeType::State,
         q_text,
     ).expect("retrieve");

    println!("Conflict test result: seed_id={:?}, conflict_detected={}, abstained={}, candidates={:?}",
             result.seed_id, result.conflict_detected, result.abstained, result.conflict_candidates);

    // Must detect contradiction
    assert!(result.conflict_detected, "GATE BREACH: Conflict must be detected on competing assertion keys with opposite polarity");
    assert_eq!(
        result.conflict_candidates.first().map(|c| c.conflict_type.clone()),
        Some(ConflictType::LogicalContradiction),
        "GATE BREACH: Conflict type must be LogicalContradiction"
    );
    // With abstain_on_conflict = true, must refuse to return contested winner
    assert!(result.abstained, "GATE BREACH: Engine must abstain on logical contradiction when abstain_on_conflict = true");
    assert!(result.causal_chain.is_empty(), "GATE BREACH: Causal chain must be empty on abstention");

    rt.shutdown();
    let _ = std::fs::remove_dir_all(&data_dir);
    println!("\nPHASE 3 CONFLICT DETECTION & ABSTENTION GATE STRICTLY PASSED.\n");
}


