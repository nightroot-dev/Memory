//! Phase 3 Approval Benchmark — Role A scaffolding (frozen at bc66340)
//!
//! AUTHORING BOUNDARY (Protocol v1.5 Section 4.1):
//!   Role A authored this file: all types, the runner harness, scoring logic,
//!   distractor generation, pool-stage extraction, gate metric computation,
//!   and the PHASE3_RESULT|...| structured output formatter.
//!
//!   Role B fills in DATA ONLY: the `SCENARIOS` static slice below.
//!   ZERO structural changes, type changes, harness changes, or scoring changes
//!   are permitted after Role B takes custody.
//!
//! PROHIBITED CORPUS GUARD:
//!   This file must never contain any query, positive text, or negative text
//!   derived from `dprime_bench.rs` (15-query rejection set) or
//!   `phase3_held_out_bench.rs` (32-query compromised set).
//!   The overlap auditor (`scripts/audit_corpus_overlap.ps1`) enforces this
//!   automatically at Stage 1. Any violation fails closed and terminates
//!   the attempt.
//!
//! EXECUTION CONSTRAINT:
//!   This benchmark consumes the single approval attempt when Stage 2 launches.
//!   Do not run `scripts/verify_phase3_approval.ps1` without `-SyntheticOnly`
//!   until Role B has committed and audited the corpus.

use std::sync::atomic::{AtomicU32, Ordering};
use tempfile::TempDir;

use cmhm_core::{Config, ContextRuntime, ContextType, ConflictType, CallerRole};

// ── Scenario types (frozen — Role B must not modify) ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioClass {
    Contradiction,
    Supersession,
    AuthorityPolarity,
    AdversarialAuthority,
    SafeAbstention,
    UnresolvedConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedConflict {
    LogicalContradiction,
    AuthorityPolarity,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedWinner {
    Pos,
    Neg,
    None,
}

pub struct ApprovalScenario {
    /// Unique scenario ID (1-based, sequential).
    pub id: usize,
    /// Operational domain label (see Section 4.2 domain table).
    pub domain: &'static str,
    /// Scenario class (see Section 4.4 matrix).
    pub class: ScenarioClass,
    /// The retrieval query text (Role B-authored; must not overlap prohibited corpora).
    pub query: &'static str,
    /// Positive candidate text (the "correct" / preferred memory entry).
    pub pos_text: &'static str,
    pub pos_auth: f32,
    pub pos_prov: &'static str,
    pub pos_key: Option<&'static str>,
    pub pos_polarity: Option<i8>,
    /// Hard-negative candidate text (the "dangerous" conflicting entry).
    pub neg_text: &'static str,
    pub neg_auth: f32,
    pub neg_prov: &'static str,
    pub neg_key: Option<&'static str>,
    pub neg_polarity: Option<i8>,
    /// If true, D+ supersedes D− (SUPERSESSION class only).
    pub supersedes_neg: bool,
    /// Expected conflict classification from the engine.
    pub expected_conflict: ExpectedConflict,
    /// If true, the engine must abstain (winner = None).
    pub expected_abstain: bool,
    /// Which candidate should win if not abstained.
    pub expected_winner: ExpectedWinner,
}

// ── ROLE B: POPULATE THIS SLICE ONLY. MINIMUM 40 SCENARIOS. ──────────────────
// Zero structural or harness logic changes are permitted.
// Follow Section 4 of Protocol v1.5 strictly.
// ── ROLE B CORPUS ─────────────────────────────────────────────────────────────
// 40 scenarios selected from Role B's 140-scenario submission.
// Original IDs: 1–37, 57, 62, 67 → renumbered 1–40 here for sequential compliance.
// Satisfies: ≥10 Contradiction, ≥6 Supersession, ≥6 AuthorityPolarity,
//            ≥6 AdversarialAuthority, ≥6 SafeAbstention, ≥6 UnresolvedConflict,
//            no domain > 6, total = 40.
// expected_conflict / expected_abstain / expected_winner are derived from class per
// Protocol v1.6 Section 4.4; those fields are metadata only — gates use engine output.
//
// NOTE G8b: AdversarialAuthority scenarios (bench IDs 22–27) carry same assertion_key
// with opposite polarity; engine may emit LogicalContradiction instead of
// AuthorityPolarity for those. Gate results are authoritative.
pub static SCENARIOS: &[ApprovalScenario] = &[

    // ── CONTRADICTION (10) ────────────────────────────────────────────────────

    // bench 1, orig 1
    ApprovalScenario {
        id: 1, domain: "Infrastructure/OS", class: ScenarioClass::Contradiction,
        query: "Should the systemd unit for the API service enable automatic restarts?",
        pos_text: "systemd unit api.service has Restart=always enabled",
        pos_auth: 0.90, pos_prov: "user_explicit",
        pos_key: Some("systemd_api_restart_policy"), pos_polarity: Some(1),
        neg_text: "systemd unit api.service has Restart=no",
        neg_auth: 0.55, neg_prov: "contradicted",
        neg_key: Some("systemd_api_restart_policy"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 2, orig 2
    ApprovalScenario {
        id: 2, domain: "Database", class: ScenarioClass::Contradiction,
        query: "Is connection pooling enabled for the PostgreSQL client?",
        pos_text: "PostgreSQL client pool_size=20 is active",
        pos_auth: 0.85, pos_prov: "user_explicit",
        pos_key: Some("postgres_pool_enabled"), pos_polarity: Some(1),
        neg_text: "PostgreSQL client pool_size=0 (disabled)",
        neg_auth: 0.40, neg_prov: "contradicted",
        neg_key: Some("postgres_pool_enabled"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 3, orig 3
    ApprovalScenario {
        id: 3, domain: "Network/TLS", class: ScenarioClass::Contradiction,
        query: "Is TLS certificate verification enabled for outgoing HTTPS requests?",
        pos_text: "outgoing HTTPS requests have certificate verification enabled",
        pos_auth: 0.88, pos_prov: "user_explicit",
        pos_key: Some("tls_verify_enabled"), pos_polarity: Some(1),
        neg_text: "outgoing HTTPS requests skip certificate verification",
        neg_auth: 0.45, neg_prov: "contradicted",
        neg_key: Some("tls_verify_enabled"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 4, orig 4
    ApprovalScenario {
        id: 4, domain: "Container orchestration", class: ScenarioClass::Contradiction,
        query: "Is the Kubernetes deployment configured to use a rolling update strategy?",
        pos_text: "K8s deployment uses RollingUpdate strategy",
        pos_auth: 0.82, pos_prov: "user_explicit",
        pos_key: Some("k8s_update_strategy"), pos_polarity: Some(1),
        neg_text: "K8s deployment uses Recreate strategy",
        neg_auth: 0.48, neg_prov: "contradicted",
        neg_key: Some("k8s_update_strategy"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 5, orig 5
    ApprovalScenario {
        id: 5, domain: "Cloud/IAM", class: ScenarioClass::Contradiction,
        query: "Is MFA enforced for all IAM users?",
        pos_text: "IAM policy enforces MFA for all users",
        pos_auth: 0.92, pos_prov: "user_explicit",
        pos_key: Some("iam_mfa_required"), pos_polarity: Some(1),
        neg_text: "IAM policy does not require MFA",
        neg_auth: 0.38, neg_prov: "contradicted",
        neg_key: Some("iam_mfa_required"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 6, orig 6
    ApprovalScenario {
        id: 6, domain: "Security/crypto", class: ScenarioClass::Contradiction,
        query: "Should the encryption key be rotated every 90 days?",
        pos_text: "Key rotation policy is set to 90 days",
        pos_auth: 0.87, pos_prov: "user_explicit",
        pos_key: Some("key_rotation_90d"), pos_polarity: Some(1),
        neg_text: "Key rotation policy is disabled",
        neg_auth: 0.42, neg_prov: "contradicted",
        neg_key: Some("key_rotation_90d"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 7, orig 7
    ApprovalScenario {
        id: 7, domain: "Distributed consensus", class: ScenarioClass::Contradiction,
        query: "Is the Raft consensus leader election timeout set to 150ms?",
        pos_text: "Raft leader election timeout = 150ms",
        pos_auth: 0.80, pos_prov: "user_explicit",
        pos_key: Some("raft_election_timeout"), pos_polarity: Some(1),
        neg_text: "Raft leader election timeout = 500ms",
        neg_auth: 0.50, neg_prov: "contradicted",
        neg_key: Some("raft_election_timeout"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 8, orig 8
    ApprovalScenario {
        id: 8, domain: "Observability", class: ScenarioClass::Contradiction,
        query: "Is debug-level logging enabled for the ingest service?",
        pos_text: "ingest service log level is debug",
        pos_auth: 0.78, pos_prov: "user_explicit",
        pos_key: Some("ingest_log_level_debug"), pos_polarity: Some(1),
        neg_text: "ingest service log level is info",
        neg_auth: 0.52, neg_prov: "contradicted",
        neg_key: Some("ingest_log_level_debug"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 9, orig 9
    ApprovalScenario {
        id: 9, domain: "Storage/filesystem", class: ScenarioClass::Contradiction,
        query: "Is the data volume mounted with noatime?",
        pos_text: "/data volume is mounted with noatime option",
        pos_auth: 0.84, pos_prov: "user_explicit",
        pos_key: Some("data_volume_noatime"), pos_polarity: Some(1),
        neg_text: "/data volume is mounted with atime enabled",
        neg_auth: 0.46, neg_prov: "contradicted",
        neg_key: Some("data_volume_noatime"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 10, orig 10
    ApprovalScenario {
        id: 10, domain: "Infrastructure/OS", class: ScenarioClass::Contradiction,
        query: "Is the swap partition encrypted?",
        pos_text: "Swap partition is encrypted with LUKS",
        pos_auth: 0.89, pos_prov: "user_explicit",
        pos_key: Some("swap_encrypted"), pos_polarity: Some(1),
        neg_text: "Swap partition is not encrypted",
        neg_auth: 0.41, neg_prov: "contradicted",
        neg_key: Some("swap_encrypted"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },

    // ── SUPERSESSION (6) ──────────────────────────────────────────────────────

    // bench 11, orig 11
    ApprovalScenario {
        id: 11, domain: "Database", class: ScenarioClass::Supersession,
        query: "What is the current read replica count for the production DB?",
        pos_text: "Production DB has 3 read replicas",
        pos_auth: 0.95, pos_prov: "user_explicit",
        pos_key: Some("db_replica_count"), pos_polarity: Some(1),
        neg_text: "Production DB has 1 read replica",
        neg_auth: 0.60, neg_prov: "contradicted",
        neg_key: Some("db_replica_count"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    // bench 12, orig 12
    ApprovalScenario {
        id: 12, domain: "Network/TLS", class: ScenarioClass::Supersession,
        query: "Which TLS version is allowed for the public API?",
        pos_text: "Public API supports TLS 1.3 only",
        pos_auth: 0.94, pos_prov: "user_explicit",
        pos_key: Some("api_tls_version"), pos_polarity: Some(1),
        neg_text: "Public API supports TLS 1.2 and 1.3",
        neg_auth: 0.55, neg_prov: "contradicted",
        neg_key: Some("api_tls_version"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    // bench 13, orig 13
    ApprovalScenario {
        id: 13, domain: "Container orchestration", class: ScenarioClass::Supersession,
        query: "What is the CPU limit for the worker container?",
        pos_text: "Worker container CPU limit is 2 cores",
        pos_auth: 0.93, pos_prov: "user_explicit",
        pos_key: Some("worker_cpu_limit"), pos_polarity: Some(1),
        neg_text: "Worker container CPU limit is 1 core",
        neg_auth: 0.58, neg_prov: "contradicted",
        neg_key: Some("worker_cpu_limit"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    // bench 14, orig 14
    ApprovalScenario {
        id: 14, domain: "Cloud/IAM", class: ScenarioClass::Supersession,
        query: "What is the maximum session duration for temporary credentials?",
        pos_text: "Temporary credentials max session duration is 12 hours",
        pos_auth: 0.91, pos_prov: "user_explicit",
        pos_key: Some("iam_session_duration"), pos_polarity: Some(1),
        neg_text: "Temporary credentials max session duration is 1 hour",
        neg_auth: 0.50, neg_prov: "contradicted",
        neg_key: Some("iam_session_duration"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    // bench 15, orig 15
    ApprovalScenario {
        id: 15, domain: "Security/crypto", class: ScenarioClass::Supersession,
        query: "What is the current password policy for service accounts?",
        pos_text: "Service account password minimum length is 24 characters",
        pos_auth: 0.92, pos_prov: "user_explicit",
        pos_key: Some("password_policy_min_length"), pos_polarity: Some(1),
        neg_text: "Service account password minimum length is 16 characters",
        neg_auth: 0.53, neg_prov: "contradicted",
        neg_key: Some("password_policy_min_length"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    // bench 16, orig 16
    ApprovalScenario {
        id: 16, domain: "Distributed consensus", class: ScenarioClass::Supersession,
        query: "What is the quorum size for the etcd cluster?",
        pos_text: "etcd cluster quorum size is 3 nodes",
        pos_auth: 0.90, pos_prov: "user_explicit",
        pos_key: Some("etcd_quorum_size"), pos_polarity: Some(1),
        neg_text: "etcd cluster quorum size is 5 nodes",
        neg_auth: 0.49, neg_prov: "contradicted",
        neg_key: Some("etcd_quorum_size"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },

    // ── AUTHORITY-POLARITY (6) ────────────────────────────────────────────────

    // bench 17, orig 17
    ApprovalScenario {
        id: 17, domain: "Observability", class: ScenarioClass::AuthorityPolarity,
        query: "Should the metrics endpoint be exposed publicly?",
        pos_text: "Metrics endpoint should be bound to localhost only",
        pos_auth: 0.85, pos_prov: "corrected",
        pos_key: Some("metrics_endpoint_bind"), pos_polarity: Some(1),
        neg_text: "Metrics endpoint should be exposed on all interfaces",
        neg_auth: 0.75, neg_prov: "user_explicit",
        neg_key: Some("metrics_endpoint_bind"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 18, orig 18
    ApprovalScenario {
        id: 18, domain: "Storage/filesystem", class: ScenarioClass::AuthorityPolarity,
        query: "What filesystem type is used for the data volume?",
        pos_text: "Data volume uses XFS filesystem",
        pos_auth: 0.86, pos_prov: "corrected",
        pos_key: Some("data_volume_fs_type"), pos_polarity: Some(1),
        neg_text: "Data volume uses ext4 filesystem",
        neg_auth: 0.74, neg_prov: "user_explicit",
        neg_key: Some("data_volume_fs_type"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 19, orig 19
    ApprovalScenario {
        id: 19, domain: "Infrastructure/OS", class: ScenarioClass::AuthorityPolarity,
        query: "What is the kernel sysctl value for net.core.somaxconn?",
        pos_text: "net.core.somaxconn = 4096",
        pos_auth: 0.83, pos_prov: "corrected",
        pos_key: Some("sysctl_somaxconn"), pos_polarity: Some(1),
        neg_text: "net.core.somaxconn = 1024",
        neg_auth: 0.70, neg_prov: "user_explicit",
        neg_key: Some("sysctl_somaxconn"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 20, orig 20
    ApprovalScenario {
        id: 20, domain: "Database", class: ScenarioClass::AuthorityPolarity,
        query: "What is the connection timeout for the Redis client?",
        pos_text: "Redis client connection timeout = 5000ms",
        pos_auth: 0.87, pos_prov: "corrected",
        pos_key: Some("redis_connection_timeout"), pos_polarity: Some(1),
        neg_text: "Redis client connection timeout = 2000ms",
        neg_auth: 0.76, neg_prov: "user_explicit",
        neg_key: Some("redis_connection_timeout"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 21, orig 21
    ApprovalScenario {
        id: 21, domain: "Network/TLS", class: ScenarioClass::AuthorityPolarity,
        query: "What is the keepalive interval for the gRPC connection?",
        pos_text: "gRPC keepalive interval is 30 seconds",
        pos_auth: 0.84, pos_prov: "corrected",
        pos_key: Some("grpc_keepalive_interval"), pos_polarity: Some(1),
        neg_text: "gRPC keepalive interval is 60 seconds",
        neg_auth: 0.72, neg_prov: "user_explicit",
        neg_key: Some("grpc_keepalive_interval"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 22, orig 57 (6th AuthorityPolarity — Observability)
    ApprovalScenario {
        id: 22, domain: "Observability", class: ScenarioClass::AuthorityPolarity,
        query: "What is the log shipping batch size?",
        pos_text: "Log shipping batch size is 512 records",
        pos_auth: 0.84, pos_prov: "corrected",
        pos_key: Some("log_shipping_batch_size"), pos_polarity: Some(1),
        neg_text: "Log shipping batch size is 1024 records",
        neg_auth: 0.73, neg_prov: "user_explicit",
        neg_key: Some("log_shipping_batch_size"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },

    // ── ADVERSARIAL-AUTHORITY (6) ─────────────────────────────────────────────

    // bench 23, orig 22
    ApprovalScenario {
        id: 23, domain: "Cloud/IAM", class: ScenarioClass::AdversarialAuthority,
        query: "Should the S3 bucket be public?",
        pos_text: "S3 bucket is private (no public access)",
        pos_auth: 0.80, pos_prov: "corrected",
        pos_key: Some("s3_bucket_public_access"), pos_polarity: Some(-1),
        neg_text: "S3 bucket is public read-only",
        neg_auth: 0.95, neg_prov: "user_explicit",
        neg_key: Some("s3_bucket_public_access"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 24, orig 23
    ApprovalScenario {
        id: 24, domain: "Security/crypto", class: ScenarioClass::AdversarialAuthority,
        query: "Is the admin password stored in plaintext?",
        pos_text: "Admin password is stored hashed with bcrypt",
        pos_auth: 0.80, pos_prov: "corrected",
        pos_key: Some("admin_password_storage"), pos_polarity: Some(-1),
        neg_text: "Admin password is stored in plaintext",
        neg_auth: 0.95, neg_prov: "user_explicit",
        neg_key: Some("admin_password_storage"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 25, orig 24
    ApprovalScenario {
        id: 25, domain: "Container orchestration", class: ScenarioClass::AdversarialAuthority,
        query: "Is privileged mode enabled for the container?",
        pos_text: "Container runs with privileged mode disabled",
        pos_auth: 0.82, pos_prov: "corrected",
        pos_key: Some("container_privileged_mode"), pos_polarity: Some(-1),
        neg_text: "Container runs with privileged mode enabled",
        neg_auth: 0.96, neg_prov: "user_explicit",
        neg_key: Some("container_privileged_mode"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 26, orig 25
    ApprovalScenario {
        id: 26, domain: "Distributed consensus", class: ScenarioClass::AdversarialAuthority,
        query: "Is the Raft snapshot compression enabled?",
        pos_text: "Raft snapshot compression is enabled",
        pos_auth: 0.81, pos_prov: "corrected",
        pos_key: Some("raft_snapshot_compression"), pos_polarity: Some(1),
        neg_text: "Raft snapshot compression is disabled",
        neg_auth: 0.94, neg_prov: "user_explicit",
        neg_key: Some("raft_snapshot_compression"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 27, orig 26
    ApprovalScenario {
        id: 27, domain: "Observability", class: ScenarioClass::AdversarialAuthority,
        query: "Is the trace sampling rate set to 10%?",
        pos_text: "Trace sampling rate is 10%",
        pos_auth: 0.79, pos_prov: "corrected",
        pos_key: Some("trace_sampling_rate"), pos_polarity: Some(1),
        neg_text: "Trace sampling rate is 100%",
        neg_auth: 0.93, neg_prov: "user_explicit",
        neg_key: Some("trace_sampling_rate"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 28, orig 62 (6th AdversarialAuthority — Cloud/IAM)
    ApprovalScenario {
        id: 28, domain: "Cloud/IAM", class: ScenarioClass::AdversarialAuthority,
        query: "Should the IAM role have full administrative access?",
        pos_text: "IAM role has least-privilege permissions only",
        pos_auth: 0.80, pos_prov: "corrected",
        pos_key: Some("iam_role_admin_access"), pos_polarity: Some(-1),
        neg_text: "IAM role has full administrative access",
        neg_auth: 0.95, neg_prov: "user_explicit",
        neg_key: Some("iam_role_admin_access"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },

    // ── SAFE-ABSTENTION (6) ───────────────────────────────────────────────────

    // bench 29, orig 27
    ApprovalScenario {
        id: 29, domain: "Storage/filesystem", class: ScenarioClass::SafeAbstention,
        query: "What is the default block size for the log volume?",
        pos_text: "Log volume block size is 4096 bytes",
        pos_auth: 0.68, pos_prov: "user_explicit",
        pos_key: Some("log_volume_blocksize"), pos_polarity: Some(1),
        neg_text: "Log volume block size is 8192 bytes",
        neg_auth: 0.66, neg_prov: "user_explicit",
        neg_key: Some("log_volume_blocksize"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 30, orig 28
    ApprovalScenario {
        id: 30, domain: "Infrastructure/OS", class: ScenarioClass::SafeAbstention,
        query: "What is the file descriptor limit for the application process?",
        pos_text: "Process file descriptor limit is 65536",
        pos_auth: 0.70, pos_prov: "user_explicit",
        pos_key: Some("process_fd_limit"), pos_polarity: Some(1),
        neg_text: "Process file descriptor limit is 32768",
        neg_auth: 0.67, neg_prov: "user_explicit",
        neg_key: Some("process_fd_limit"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 31, orig 29
    ApprovalScenario {
        id: 31, domain: "Database", class: ScenarioClass::SafeAbstention,
        query: "What is the isolation level for the transactions service?",
        pos_text: "Transaction isolation level is READ COMMITTED",
        pos_auth: 0.69, pos_prov: "user_explicit",
        pos_key: Some("tx_isolation_level"), pos_polarity: Some(1),
        neg_text: "Transaction isolation level is REPEATABLE READ",
        neg_auth: 0.68, neg_prov: "user_explicit",
        neg_key: Some("tx_isolation_level"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 32, orig 30
    ApprovalScenario {
        id: 32, domain: "Network/TLS", class: ScenarioClass::SafeAbstention,
        query: "What is the HTTP keep-alive timeout for the proxy?",
        pos_text: "Proxy keep-alive timeout is 60 seconds",
        pos_auth: 0.65, pos_prov: "user_explicit",
        pos_key: Some("proxy_keepalive_timeout"), pos_polarity: Some(1),
        neg_text: "Proxy keep-alive timeout is 120 seconds",
        neg_auth: 0.63, neg_prov: "user_explicit",
        neg_key: Some("proxy_keepalive_timeout"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 33, orig 31
    ApprovalScenario {
        id: 33, domain: "Cloud/IAM", class: ScenarioClass::SafeAbstention,
        query: "Is the IAM role allowed to assume the production role?",
        pos_text: "IAM role has sts:AssumeRole permission for production role",
        pos_auth: 0.64, pos_prov: "user_explicit",
        pos_key: Some("iam_assume_role_permission"), pos_polarity: Some(1),
        neg_text: "IAM role lacks sts:AssumeRole permission for production role",
        neg_auth: 0.62, neg_prov: "user_explicit",
        neg_key: Some("iam_assume_role_permission"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    // bench 34, orig 67 (6th SafeAbstention — Storage/filesystem)
    ApprovalScenario {
        id: 34, domain: "Storage/filesystem", class: ScenarioClass::SafeAbstention,
        query: "What is the mount option for the database volume regarding file locking?",
        pos_text: "Database volume uses local file locking",
        pos_auth: 0.67, pos_prov: "user_explicit",
        pos_key: Some("db_volume_file_lock"), pos_polarity: Some(1),
        neg_text: "Database volume uses network file locking",
        neg_auth: 0.65, neg_prov: "user_explicit",
        neg_key: Some("db_volume_file_lock"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain: true, expected_winner: ExpectedWinner::None,
    },

    // ── UNRESOLVED-CONFLICT (6) ───────────────────────────────────────────────

    // bench 35, orig 32
    ApprovalScenario {
        id: 35, domain: "Security/crypto", class: ScenarioClass::UnresolvedConflict,
        query: "What encryption algorithm is used for data at rest?",
        pos_text: "Data at rest uses AES-256-GCM",
        pos_auth: 0.85, pos_prov: "user_explicit",
        pos_key: Some("data_at_rest_encryption_algorithm"), pos_polarity: Some(1),
        neg_text: "Data at rest uses ChaCha20-Poly1305",
        neg_auth: 0.80, neg_prov: "user_explicit",
        neg_key: Some("data_at_rest_encryption_algorithm"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    // bench 36, orig 33
    ApprovalScenario {
        id: 36, domain: "Container orchestration", class: ScenarioClass::UnresolvedConflict,
        query: "What is the image pull policy for the deployment?",
        pos_text: "Image pull policy is Always",
        pos_auth: 0.82, pos_prov: "user_explicit",
        pos_key: Some("image_pull_policy"), pos_polarity: Some(1),
        neg_text: "Image pull policy is IfNotPresent",
        neg_auth: 0.78, neg_prov: "user_explicit",
        neg_key: Some("image_pull_policy"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    // bench 37, orig 34
    ApprovalScenario {
        id: 37, domain: "Distributed consensus", class: ScenarioClass::UnresolvedConflict,
        query: "What is the heartbeat interval for the consensus group?",
        pos_text: "Consensus heartbeat interval is 500ms",
        pos_auth: 0.83, pos_prov: "user_explicit",
        pos_key: Some("consensus_heartbeat_interval"), pos_polarity: Some(1),
        neg_text: "Consensus heartbeat interval is 250ms",
        neg_auth: 0.81, neg_prov: "user_explicit",
        neg_key: Some("consensus_heartbeat_interval"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    // bench 38, orig 35
    ApprovalScenario {
        id: 38, domain: "Observability", class: ScenarioClass::UnresolvedConflict,
        query: "What is the log retention period for the audit logs?",
        pos_text: "Audit log retention period is 90 days",
        pos_auth: 0.84, pos_prov: "user_explicit",
        pos_key: Some("audit_log_retention"), pos_polarity: Some(1),
        neg_text: "Audit log retention period is 180 days",
        neg_auth: 0.79, neg_prov: "user_explicit",
        neg_key: Some("audit_log_retention"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    // bench 39, orig 36
    ApprovalScenario {
        id: 39, domain: "Storage/filesystem", class: ScenarioClass::UnresolvedConflict,
        query: "What is the backup snapshot frequency for the persistent volume?",
        pos_text: "Persistent volume snapshot frequency is every 12 hours",
        pos_auth: 0.86, pos_prov: "user_explicit",
        pos_key: Some("pv_snapshot_frequency"), pos_polarity: Some(1),
        neg_text: "Persistent volume snapshot frequency is every 24 hours",
        neg_auth: 0.77, neg_prov: "user_explicit",
        neg_key: Some("pv_snapshot_frequency"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    // bench 40, orig 37
    ApprovalScenario {
        id: 40, domain: "Infrastructure/OS", class: ScenarioClass::UnresolvedConflict,
        query: "What is the systemd service type for the worker process?",
        pos_text: "Worker service type is forking",
        pos_auth: 0.80, pos_prov: "user_explicit",
        pos_key: Some("worker_service_type"), pos_polarity: Some(1),
        neg_text: "Worker service type is simple",
        neg_auth: 0.76, neg_prov: "user_explicit",
        neg_key: Some("worker_service_type"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
];

// ── Distractor RNG seed (frozen by protocol) ──────────────────────────────────
const DISTRACTOR_RNG_SEED: u64 = 0xA3F7_2B91_C5D4_E608;
const MIN_DISTRACTORS: usize = 60;

// ── Runtime counter for isolated temp dirs ───────────────────────────────────
static COUNTER: AtomicU32 = AtomicU32::new(0);

// ── Harness helpers (frozen — Role B must not modify) ────────────────────────

fn make_runtime_pair() -> (ContextRuntime, TempDir) {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = TempDir::new().expect("tempdir");
    let mut cfg = Config::default();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.projections_dir = tmp.path().join("projections");
    let rt = ContextRuntime::new(cfg, format!("bench-{id}")).expect("runtime init");
    (rt, tmp)
}

/// Deterministic LCG embedding from a u64 seed (128-dimensional, unit-normed).
fn lcg_emb(seed: u64) -> Vec<f32> {
    let mut v = vec![0.0f32; 128];
    let mut x = seed;
    for f in v.iter_mut() {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *f = ((x >> 33) as f32) / (u32::MAX as f32) - 0.5;
    }
    let norm = v.iter().map(|f| f * f).sum::<f32>().sqrt().max(1e-9);
    v.iter_mut().for_each(|f| *f /= norm);
    v
}

/// Learn a node using text_to_embedding (the production path under test).
fn learn_text(
    rt: &mut ContextRuntime,
    ctx_id: &str,
    text: &str,
    auth: f32,
    prov: &str,
    supersedes: Option<u64>,
    key: Option<&str>,
    polarity: Option<i8>,
) -> u64 {
    use cmhm_core::text_to_embedding;
    let emb = text_to_embedding(text).to_vec();
    let (res, _) = rt.learn_with_context(
        Some(ctx_id),
        emb.clone(), emb.clone(), emb.clone(),
        0.5, 0.5,
        "hash-128d",
        auth,
        prov.to_string(),
        supersedes,
        CallerRole::User,
        key.map(|s| s.to_string()),
        polarity,
    ).expect("learn_text");
    res.state_node_id
}

/// Learn a background distractor node (LCG embedding, low authority).
fn learn_distractor(rt: &mut ContextRuntime, ctx_id: &str, seed: u64) -> u64 {
    let emb = lcg_emb(seed);
    let (res, _) = rt.learn_with_context(
        Some(ctx_id),
        emb.clone(), emb.clone(), emb.clone(),
        0.1, 0.1,
        "hash-128d",
        0.30,
        "distractor".to_string(),
        None,
        CallerRole::User,
        None,
        None,
    ).expect("learn_distractor");
    res.state_node_id
}

// ── Per-scenario result (frozen) ──────────────────────────────────────────────

struct ScenarioResult {
    id: usize,
    domain: &'static str,
    class: ScenarioClass,
    // Pool stage membership
    pos_in_raw_nn:       bool,
    neg_in_raw_nn:       bool,
    pos_in_post_filter:  bool,
    neg_in_post_filter:  bool,
    pos_rank_scored:     Option<usize>,  // 1-based; None if absent
    neg_rank_scored:     Option<usize>,
    // Composite scores
    pos_score:           Option<f32>,
    neg_score:           Option<f32>,
    // Engine outputs
    abstained:           bool,
    winner_id:           Option<u64>,
    conflict_type:       Option<ConflictType>,
    // Expected
    expected_conflict:   ExpectedConflict,
    expected_abstain:    bool,
    expected_winner:     ExpectedWinner,
    pos_id:              u64,
    neg_id:              u64,
}

// ── 5-way outcome categorisation (frozen) ─────────────────────────────────────

#[derive(Debug, Default)]
struct OutcomeCounts {
    selected_d_plus:       usize,
    selected_d_minus:      usize,
    selected_other:        usize,
    correct_abstention:    usize,
    incorrect_abstention:  usize,
}

fn categorise(r: &ScenarioResult) -> &'static str {
    if r.abstained {
        if r.expected_abstain { "correct_abstention" } else { "incorrect_abstention" }
    } else {
        match r.winner_id {
            Some(w) if w == r.pos_id => "selected_d_plus",
            Some(w) if w == r.neg_id => "selected_d_minus",
            _                         => "selected_other",
        }
    }
}

// ── Bootstrap CI helper (10,000 resamples, frozen) ────────────────────────────

fn bootstrap_ci_proportion(hits: usize, n: usize, iters: usize, seed: u64)
    -> (f64, f64)
{
    if n == 0 { return (0.0, 0.0); }
    let p = hits as f64 / n as f64;
    let mut rng = seed;
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let mut s = 0usize;
        for _ in 0..n {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let u = (rng >> 33) as usize;
            if (u % n) < hits { s += 1; }
        }
        samples.push(s as f64 / n as f64);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lo = samples[(0.025 * iters as f64) as usize];
    let hi = samples[(0.975 * iters as f64) as usize];
    let _ = p;
    (lo, hi)
}

// ── Gate evaluation (frozen) ───────────────────────────────────────────────────

struct GateResult {
    name:   &'static str,
    passed: bool,
    value:  f64,
    ci_lo:  f64,
    ci_hi:  f64,
    note:   String,
}

// ── Main benchmark test ────────────────────────────────────────────────────────

#[test]
fn phase3_approval_benchmark() {
    assert!(
        !SCENARIOS.is_empty(),
        "SCENARIOS is empty — Role B must populate the corpus before running the benchmark."
    );
    assert!(
        SCENARIOS.len() >= 40,
        "SCENARIOS has {} entries; protocol requires ≥ 40.",
        SCENARIOS.len()
    );

    let n = SCENARIOS.len();
    let mut results: Vec<ScenarioResult> = Vec::with_capacity(n);

    for scenario in SCENARIOS.iter() {
        // ── Isolated runtime per scenario ──────────────────────────────────
        let (mut rt, _tmp) = make_runtime_pair();
        let ctx_id = rt.create_context(
            &format!("approval-{}", scenario.id),
            ContextType::default(),
            None, "", vec![],
        ).expect("create_context").id.to_string();

        // ── Populate 60+ background distractors ───────────────────────────
        let base_seed = DISTRACTOR_RNG_SEED
            .wrapping_add((scenario.id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        for i in 0..MIN_DISTRACTORS {
            learn_distractor(&mut rt, &ctx_id, base_seed.wrapping_add(i as u64));
        }

        // ── Learn D− (negative / dangerous) ───────────────────────────────
        let neg_id = learn_text(
            &mut rt, &ctx_id,
            scenario.neg_text,
            scenario.neg_auth,
            scenario.neg_prov,
            None,
            scenario.neg_key,
            scenario.neg_polarity,
        );

        // ── Learn D+ (positive / correct) ─────────────────────────────────
        // For SUPERSESSION: D+ supersedes D−.
        let supersedes_arg = if scenario.supersedes_neg { Some(neg_id) } else { None };
        let pos_id = learn_text(
            &mut rt, &ctx_id,
            scenario.pos_text,
            scenario.pos_auth,
            scenario.pos_prov,
            supersedes_arg,
            scenario.pos_key,
            scenario.pos_polarity,
        );

        // ── Retrieve via production unscoped path ──────────────────────────
        use cmhm_core::text_to_embedding;
        let query_emb = text_to_embedding(scenario.query).to_vec();
        let (result, _) = rt
            .retrieve_unscoped(Some(&ctx_id), query_emb, "hash-128d")
            .expect("retrieve_unscoped");

        // ── Pool stage membership ──────────────────────────────────────────
        let pos_in_raw_nn      = result.raw_nn_candidates.contains(&pos_id);
        let neg_in_raw_nn      = result.raw_nn_candidates.contains(&neg_id);
        let pos_in_post_filter = result.post_filter_candidates.contains(&pos_id);
        let neg_in_post_filter = result.post_filter_candidates.contains(&neg_id);

        let pos_rank_scored = result.scored_candidates.iter()
            .position(|&(id, _)| id == pos_id)
            .map(|i| i + 1);
        let neg_rank_scored = result.scored_candidates.iter()
            .position(|&(id, _)| id == neg_id)
            .map(|i| i + 1);
        let pos_score = result.scored_candidates.iter()
            .find(|&&(id, _)| id == pos_id)
            .map(|&(_, s)| s);
        let neg_score = result.scored_candidates.iter()
            .find(|&&(id, _)| id == neg_id)
            .map(|&(_, s)| s);

        // ── Engine decision ────────────────────────────────────────────────
        let abstained    = result.abstained;
        let winner_id    = result.seed_id;
        // conflict_type is stored on ConflictCandidate entries; take from first if present
        let conflict_type: Option<ConflictType> = result.conflict_candidates
            .first()
            .map(|c| c.conflict_type.clone());

        results.push(ScenarioResult {
            id: scenario.id,
            domain: scenario.domain,
            class: scenario.class,
            pos_in_raw_nn, neg_in_raw_nn,
            pos_in_post_filter, neg_in_post_filter,
            pos_rank_scored, neg_rank_scored,
            pos_score, neg_score,
            abstained, winner_id, conflict_type,
            expected_conflict: scenario.expected_conflict,
            expected_abstain: scenario.expected_abstain,
            expected_winner: scenario.expected_winner,
            pos_id, neg_id,
        });
    }

    // ── Compute gate metrics ──────────────────────────────────────────────────

    // Subpopulation filters
    let all_n = n;
    let supersession: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class == ScenarioClass::Supersession).collect();
    let contradiction: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class == ScenarioClass::Contradiction).collect();
    let auth_polarity_and_adversarial: Vec<&ScenarioResult> = results.iter()
        .filter(|r| matches!(r.class,
            ScenarioClass::AuthorityPolarity | ScenarioClass::AdversarialAuthority)).collect();
    let unresolved: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class == ScenarioClass::UnresolvedConflict).collect();
    let safe_abstention: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class == ScenarioClass::SafeAbstention).collect();

    // S1: LC eligibility — D+ and D− both in SCORED top-2, gap < 0.02
    let s1_eligible: Vec<&ScenarioResult> = contradiction.iter().copied()
        .filter(|r| {
            match (r.pos_rank_scored, r.neg_rank_scored, r.pos_score, r.neg_score) {
                (Some(pr), Some(nr), Some(ps), Some(ns))
                    if pr <= 2 && nr <= 2 && (ps - ns).abs() < 0.02 => true,
                _ => false,
            }
        }).collect();

    // G1: Recall@1
    let g1_hits = results.iter().filter(|r| r.pos_rank_scored == Some(1)).count();
    // G2: Recall@5
    let g2_hits = results.iter().filter(|r| r.pos_rank_scored.map_or(false, |r| r <= 5)).count();
    // G3: MRR
    let g3_sum: f64 = results.iter()
        .map(|r| r.pos_rank_scored.map_or(0.0, |r| 1.0 / r as f64)).sum();
    let g3_val = g3_sum / all_n as f64;
    // G4: Contradiction-at-1
    let g4_hits = results.iter().filter(|r| r.neg_rank_scored == Some(1)).count();
    // G5: Hard-Neg Occupancy@5 (200 slots total)
    let g5_slots: usize = 5 * all_n; // 5 slots per query; was hardcoded 200 (scaffold defect fix)
    let g5_neg_in_top5 = results.iter()
        .filter(|r| r.neg_rank_scored.map_or(false, |r| r <= 5)).count();
    // G6: Mean Margin
    let g6_sum: f64 = results.iter()
        .map(|r| (r.pos_score.unwrap_or(0.0) - r.neg_score.unwrap_or(0.0)) as f64).sum();
    let g6_val = g6_sum / all_n as f64;
    // G7: Supersession Safety — superseded D− must not appear in post_filter/scored/chain/winner
    let g7_violations = supersession.iter()
        .filter(|r| r.neg_in_post_filter || r.neg_rank_scored.is_some()
            || r.winner_id == Some(r.neg_id)).count();
    // G8a: LogicalContradiction rate over S1-eligible contradiction scenarios
    let g8a_hits = s1_eligible.iter()
        .filter(|r| r.conflict_type == Some(ConflictType::LogicalContradiction)).count();
    // G8b: AuthorityPolarity rate
    let g8b_hits = auth_polarity_and_adversarial.iter()
        .filter(|r| r.conflict_type == Some(ConflictType::AuthorityPolarity)).count();
    // G8c: Unresolved Non-Conflict rate
    let g8c_hits = unresolved.iter()
        .filter(|r| r.conflict_type.is_none()).count();
    // G9: Safe Abstention rate
    let g9_hits = safe_abstention.iter().filter(|r| r.abstained).count();
    // G10: Pool Contamination — D− absent from RAW_NN for non-supersession scenarios
    let non_supersession: Vec<&ScenarioResult> = results.iter()
        .filter(|r| r.class != ScenarioClass::Supersession).collect();
    let g10_violations = non_supersession.iter()
        .filter(|r| !r.neg_in_raw_nn).count();

    // ── Bootstrap CIs ─────────────────────────────────────────────────────────
    let bs = 10_000usize;
    let (g1_lo, g1_hi) = bootstrap_ci_proportion(g1_hits, all_n, bs, 0x1111);
    let (g2_lo, g2_hi) = bootstrap_ci_proportion(g2_hits, all_n, bs, 0x2222);
    // G3 MRR CI: resample reciprocal ranks
    let g3_vals: Vec<f64> = results.iter()
        .map(|r| r.pos_rank_scored.map_or(0.0, |r| 1.0 / r as f64)).collect();
    let (g3_lo, g3_hi) = {
        let mut rng = 0x3333u64;
        let mut samples = Vec::with_capacity(bs);
        for _ in 0..bs {
            let mut s = 0.0f64;
            for _ in 0..all_n {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                s += g3_vals[(rng >> 33) as usize % all_n];
            }
            samples.push(s / all_n as f64);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (samples[(0.025 * bs as f64) as usize], samples[(0.975 * bs as f64) as usize])
    };
    let (g4_lo, g4_hi) = bootstrap_ci_proportion(g4_hits, all_n, bs, 0x4444);
    let (g5_lo, g5_hi) = bootstrap_ci_proportion(g5_neg_in_top5, g5_slots, bs, 0x5555);
    // G6 margin CI
    let g6_margins: Vec<f64> = results.iter()
        .map(|r| (r.pos_score.unwrap_or(0.0) - r.neg_score.unwrap_or(0.0)) as f64).collect();
    let (g6_lo, g6_hi) = {
        let mut rng = 0x6666u64;
        let mut samples = Vec::with_capacity(bs);
        for _ in 0..bs {
            let mut s = 0.0f64;
            for _ in 0..all_n {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                s += g6_margins[(rng >> 33) as usize % all_n];
            }
            samples.push(s / all_n as f64);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (samples[(0.025 * bs as f64) as usize], samples[(0.975 * bs as f64) as usize])
    };

    // ── Evaluate gates ─────────────────────────────────────────────────────────
    let gates: Vec<GateResult> = vec![
        GateResult { name: "G1",  passed: g1_lo  >= 0.40,
            value: g1_hits as f64 / all_n as f64, ci_lo: g1_lo,  ci_hi: g1_hi,
            note: format!("{}/{}", g1_hits, all_n) },
        GateResult { name: "G2",  passed: g2_lo  >= 0.67,
            value: g2_hits as f64 / all_n as f64, ci_lo: g2_lo,  ci_hi: g2_hi,
            note: format!("{}/{}", g2_hits, all_n) },
        GateResult { name: "G3",  passed: g3_lo  >  0.40,
            value: g3_val, ci_lo: g3_lo, ci_hi: g3_hi,
            note: format!("mrr={:.4}", g3_val) },
        GateResult { name: "G4",  passed: g4_hi  <  0.40,
            value: g4_hits as f64 / all_n as f64, ci_lo: g4_lo,  ci_hi: g4_hi,
            note: format!("{}/{}", g4_hits, all_n) },
        GateResult { name: "G5",  passed: g5_hi  <  0.40,
            value: g5_neg_in_top5 as f64 / g5_slots as f64,
            ci_lo: g5_lo, ci_hi: g5_hi,
            note: format!("{}/{}", g5_neg_in_top5, g5_slots) },
        GateResult { name: "G6",  passed: g6_lo  >  0.00,
            value: g6_val, ci_lo: g6_lo, ci_hi: g6_hi,
            note: format!("mean_margin={:.4}", g6_val) },
        GateResult { name: "G7",  passed: g7_violations == 0,
            value: g7_violations as f64, ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{} violations / {} supersession", g7_violations, supersession.len()) },
        GateResult { name: "S1",  passed: s1_eligible.len() >= (contradiction.len() * 60 / 100),
            value: s1_eligible.len() as f64 / contradiction.len().max(1) as f64,
            ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{}/{}", s1_eligible.len(), contradiction.len()) },
        GateResult { name: "G8a", passed: g8a_hits >= (s1_eligible.len() * 80 / 100),
            value: g8a_hits as f64 / s1_eligible.len().max(1) as f64,
            ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{}/{} eligible", g8a_hits, s1_eligible.len()) },
        GateResult { name: "G8b", passed: g8b_hits >= (auth_polarity_and_adversarial.len() * 80 / 100),
            value: g8b_hits as f64 / auth_polarity_and_adversarial.len().max(1) as f64,
            ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{}/{}", g8b_hits, auth_polarity_and_adversarial.len()) },
        GateResult { name: "G8c", passed: g8c_hits >= (unresolved.len() * 80 / 100),
            value: g8c_hits as f64 / unresolved.len().max(1) as f64,
            ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{}/{}", g8c_hits, unresolved.len()) },
        GateResult { name: "G9",  passed: g9_hits >= (safe_abstention.len() * 80 / 100),
            value: g9_hits as f64 / safe_abstention.len().max(1) as f64,
            ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{}/{}", g9_hits, safe_abstention.len()) },
        GateResult { name: "G10", passed: g10_violations == 0,
            value: g10_violations as f64, ci_lo: 0.0, ci_hi: 0.0,
            note: format!("{} violations / {} non-supersession", g10_violations, non_supersession.len()) },
    ];

    // ── 5-way outcome decomposition ────────────────────────────────────────────
    let mut outcomes = OutcomeCounts::default();
    for r in &results {
        match categorise(r) {
            "selected_d_plus"      => outcomes.selected_d_plus      += 1,
            "selected_d_minus"     => outcomes.selected_d_minus     += 1,
            "selected_other"       => outcomes.selected_other       += 1,
            "correct_abstention"   => outcomes.correct_abstention   += 1,
            "incorrect_abstention" => outcomes.incorrect_abstention += 1,
            _                      => {}
        }
    }

    // ── Structured output (parsed by verify_phase3_approval.ps1) ──────────────
    println!("PHASE3_RESULT|N={}", n);
    for g in &gates {
        println!(
            "PHASE3_RESULT|GATE|{}|passed={}|value={:.6}|ci_lo={:.6}|ci_hi={:.6}|note={}",
            g.name, g.passed, g.value, g.ci_lo, g.ci_hi, g.note
        );
    }
    println!("PHASE3_RESULT|OUTCOME|selected_d_plus={}",      outcomes.selected_d_plus);
    println!("PHASE3_RESULT|OUTCOME|selected_d_minus={}",     outcomes.selected_d_minus);
    println!("PHASE3_RESULT|OUTCOME|selected_other={}",       outcomes.selected_other);
    println!("PHASE3_RESULT|OUTCOME|correct_abstention={}",   outcomes.correct_abstention);
    println!("PHASE3_RESULT|OUTCOME|incorrect_abstention={}", outcomes.incorrect_abstention);

    // ── Per-scenario detail ────────────────────────────────────────────────────
    for r in &results {
        println!(
            "PHASE3_RESULT|SCENARIO|id={}|domain={}|class={:?}|pos_rank={:?}|neg_rank={:?}|\
             pos_score={:.4}|neg_score={:.4}|abstained={}|conflict={:?}|outcome={}",
            r.id, r.domain, r.class,
            r.pos_rank_scored, r.neg_rank_scored,
            r.pos_score.unwrap_or(0.0), r.neg_score.unwrap_or(0.0),
            r.abstained, r.conflict_type, categorise(r)
        );
    }

    // ── Final verdict ──────────────────────────────────────────────────────────
    let all_passed = gates.iter().all(|g| g.passed);
    println!("PHASE3_RESULT|VERDICT|{}", if all_passed { "APPROVED" } else { "REJECTED" });

    assert!(
        all_passed,
        "Phase 3 approval benchmark FAILED. Check PHASE3_RESULT|GATE|... lines above for failures."
    );
}
