//! Phase 3 Approval Benchmark — Role A scaffolding (frozen at bc66340)
//!
//! AUTHORING BOUNDARY (Protocol v1.8 Section 4):
//!   Role A authored this file: all types, the runner harness, scoring logic,
//!   distractor generation, pool-stage extraction, gate metric computation,
//!   the PHASE3_RESULT|...| structured output formatter, and the
//!   phase3_conformance_check test (Stage 1.5 validator).
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
//! STAGE 1.5 CONFORMANCE CHECK:
//!   `phase3_conformance_check` must pass before Stage 2 may execute.
//!   It validates every scenario in SCENARIOS against the Section 4.4 matrix.
//!   Failures terminate the attempt before the single run is consumed.
//!
//! EXECUTION CONSTRAINT:
//!   This benchmark consumes the single approval attempt when Stage 2 launches.
//!   Do not run `scripts/verify_phase3_approval.ps1` without `-SyntheticOnly`
//!   until the conformance check and overlap audit have both passed.

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
// Follow Section 4 of Protocol v1.8 strictly.
//
// CORPUS v2 PLACEHOLDER — attempt-1 corpus removed (commit e989a91, invalid on its face).
// Role B authors fresh scenarios per Protocol v1.8 Section 4.4 and Section 13.
// Role A transcribes mechanically and commits; phase3_conformance_check (Stage 1.5)
// validates conformance before Stage 2 is permitted to execute.
pub static SCENARIOS: &[ApprovalScenario] = &[

    // ── Role B corpus v2 — 43 scenarios, IDs 201–243 ─────────────────────────

    // ── CONTRADICTION (13) ─────────────────────────────────

    ApprovalScenario {
        id: 201, domain: "Infrastructure/OS", class: ScenarioClass::Contradiction,
        query:    "Should the systemd service for the metric exporter be enabled on boot?",
        pos_text: "metric-exporter.service is enabled on boot",
        pos_auth: 0.88, pos_prov: "user_explicit", pos_key: Some("metric_exporter_enabled"), pos_polarity: Some(1),
        neg_text: "metric-exporter.service is disabled on boot",
        neg_auth: 0.75, neg_prov: "contradicted", neg_key: Some("metric_exporter_enabled"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 202, domain: "Database", class: ScenarioClass::Contradiction,
        query:    "Are prepared statements enabled for the PostgreSQL driver?",
        pos_text: "PostgreSQL driver uses prepared statements",
        pos_auth: 0.86, pos_prov: "user_explicit", pos_key: Some("postgres_prepared_statements"), pos_polarity: Some(1),
        neg_text: "PostgreSQL driver does not use prepared statements",
        neg_auth: 0.78, neg_prov: "contradicted", neg_key: Some("postgres_prepared_statements"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 203, domain: "Network/TLS", class: ScenarioClass::Contradiction,
        query:    "Is TLS session resumption enabled on the API gateway?",
        pos_text: "API gateway has TLS session resumption enabled",
        pos_auth: 0.90, pos_prov: "user_explicit", pos_key: Some("tls_session_resumption"), pos_polarity: Some(1),
        neg_text: "API gateway disables TLS session resumption",
        neg_auth: 0.72, neg_prov: "contradicted", neg_key: Some("tls_session_resumption"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 204, domain: "Container orchestration", class: ScenarioClass::Contradiction,
        query:    "Is the K8s deployment configured to run with read-only root filesystem?",
        pos_text: "K8s deployment uses read-only root filesystem",
        pos_auth: 0.85, pos_prov: "user_explicit", pos_key: Some("k8s_readonly_rootfs"), pos_polarity: Some(1),
        neg_text: "K8s deployment uses writable root filesystem",
        neg_auth: 0.74, neg_prov: "contradicted", neg_key: Some("k8s_readonly_rootfs"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 205, domain: "Cloud/IAM", class: ScenarioClass::Contradiction,
        query:    "Is public access blocked on the S3 bucket?",
        pos_text: "S3 bucket blocks all public access",
        pos_auth: 0.91, pos_prov: "user_explicit", pos_key: Some("s3_block_public_access"), pos_polarity: Some(1),
        neg_text: "S3 bucket allows public read access",
        neg_auth: 0.73, neg_prov: "contradicted", neg_key: Some("s3_block_public_access"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 206, domain: "Security/crypto", class: ScenarioClass::Contradiction,
        query:    "Is the database connection encrypted using TLS?",
        pos_text: "Database connections use TLS encryption",
        pos_auth: 0.87, pos_prov: "user_explicit", pos_key: Some("db_tls_enabled"), pos_polarity: Some(1),
        neg_text: "Database connections are unencrypted",
        neg_auth: 0.76, neg_prov: "contradicted", neg_key: Some("db_tls_enabled"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 207, domain: "Distributed consensus", class: ScenarioClass::Contradiction,
        query:    "Is the Raft group using encrypted inter-node communication?",
        pos_text: "Raft inter-node communication is encrypted",
        pos_auth: 0.84, pos_prov: "user_explicit", pos_key: Some("raft_encrypted_communication"), pos_polarity: Some(1),
        neg_text: "Raft inter-node communication is plaintext",
        neg_auth: 0.77, neg_prov: "contradicted", neg_key: Some("raft_encrypted_communication"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 208, domain: "Observability", class: ScenarioClass::Contradiction,
        query:    "Is log shipping to the central platform enabled?",
        pos_text: "Log shipping to central platform is enabled",
        pos_auth: 0.82, pos_prov: "user_explicit", pos_key: Some("log_shipping_enabled"), pos_polarity: Some(1),
        neg_text: "Log shipping to central platform is disabled",
        neg_auth: 0.71, neg_prov: "contradicted", neg_key: Some("log_shipping_enabled"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 209, domain: "Storage/filesystem", class: ScenarioClass::Contradiction,
        query:    "Is the database volume encrypted at rest?",
        pos_text: "Database volume is encrypted at rest",
        pos_auth: 0.89, pos_prov: "user_explicit", pos_key: Some("db_volume_encryption"), pos_polarity: Some(1),
        neg_text: "Database volume is not encrypted at rest",
        neg_auth: 0.79, neg_prov: "contradicted", neg_key: Some("db_volume_encryption"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 210, domain: "Infrastructure/OS", class: ScenarioClass::Contradiction,
        query:    "Is the application service hardened with ProtectHome=read-only?",
        pos_text: "Application service has ProtectHome=read-only",
        pos_auth: 0.83, pos_prov: "user_explicit", pos_key: Some("protect_home_readonly"), pos_polarity: Some(1),
        neg_text: "Application service has ProtectHome=off",
        neg_auth: 0.70, neg_prov: "contradicted", neg_key: Some("protect_home_readonly"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 238, domain: "Database", class: ScenarioClass::Contradiction,
        query:    "Is the query cache enabled for the database?",
        pos_text: "Database query cache is enabled",
        pos_auth: 0.85, pos_prov: "user_explicit", pos_key: Some("db_query_cache"), pos_polarity: Some(1),
        neg_text: "Database query cache is disabled",
        neg_auth: 0.72, neg_prov: "contradicted", neg_key: Some("db_query_cache"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 239, domain: "Network/TLS", class: ScenarioClass::Contradiction,
        query:    "Is OCSP stapling enabled for the HTTPS server?",
        pos_text: "OCSP stapling is enabled",
        pos_auth: 0.84, pos_prov: "user_explicit", pos_key: Some("ocsp_stapling"), pos_polarity: Some(1),
        neg_text: "OCSP stapling is disabled",
        neg_auth: 0.73, neg_prov: "contradicted", neg_key: Some("ocsp_stapling"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 240, domain: "Cloud/IAM", class: ScenarioClass::Contradiction,
        query:    "Is the AWS CLI configured to use the company SSO endpoint?",
        pos_text: "AWS CLI profile uses company SSO endpoint",
        pos_auth: 0.86, pos_prov: "user_explicit", pos_key: Some("aws_cli_sso_configured"), pos_polarity: Some(1),
        neg_text: "AWS CLI profile uses standard IAM credentials",
        neg_auth: 0.74, neg_prov: "contradicted", neg_key: Some("aws_cli_sso_configured"), neg_polarity: Some(-1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },

    // ── SUPERSESSION (6) ───────────────────────────────────

    ApprovalScenario {
        id: 211, domain: "Database", class: ScenarioClass::Supersession,
        query:    "What is the current max_connections for the PostgreSQL server?",
        pos_text: "PostgreSQL max_connections is 300",
        pos_auth: 0.95, pos_prov: "user_explicit", pos_key: Some("postgres_max_connections"), pos_polarity: Some(1),
        neg_text: "PostgreSQL max_connections is 150",
        neg_auth: 0.60, neg_prov: "contradicted", neg_key: Some("postgres_max_connections"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 212, domain: "Network/TLS", class: ScenarioClass::Supersession,
        query:    "What is the minimum TLS version allowed?",
        pos_text: "Minimum TLS version allowed is 1.2",
        pos_auth: 0.94, pos_prov: "user_explicit", pos_key: Some("min_tls_version"), pos_polarity: Some(1),
        neg_text: "Minimum TLS version allowed is 1.0",
        neg_auth: 0.55, neg_prov: "contradicted", neg_key: Some("min_tls_version"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 213, domain: "Container orchestration", class: ScenarioClass::Supersession,
        query:    "How much memory is allocated to the worker container?",
        pos_text: "Worker container memory limit is 512 MiB",
        pos_auth: 0.93, pos_prov: "user_explicit", pos_key: Some("worker_memory_limit"), pos_polarity: Some(1),
        neg_text: "Worker container memory limit is 256 MiB",
        neg_auth: 0.58, neg_prov: "contradicted", neg_key: Some("worker_memory_limit"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 214, domain: "Cloud/IAM", class: ScenarioClass::Supersession,
        query:    "What is the session duration for temporary security credentials?",
        pos_text: "Temporary credentials session duration is 6 hours",
        pos_auth: 0.92, pos_prov: "user_explicit", pos_key: Some("temp_session_duration"), pos_polarity: Some(1),
        neg_text: "Temporary credentials session duration is 1 hour",
        neg_auth: 0.50, neg_prov: "contradicted", neg_key: Some("temp_session_duration"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 215, domain: "Security/crypto", class: ScenarioClass::Supersession,
        query:    "What is the key rotation interval for TLS certificates?",
        pos_text: "TLS certificate key rotation interval is 30 days",
        pos_auth: 0.96, pos_prov: "user_explicit", pos_key: Some("tls_key_rotation_interval"), pos_polarity: Some(1),
        neg_text: "TLS certificate key rotation interval is 90 days",
        neg_auth: 0.53, neg_prov: "contradicted", neg_key: Some("tls_key_rotation_interval"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 216, domain: "Distributed consensus", class: ScenarioClass::Supersession,
        query:    "What is the snapshot frequency for the Raft state machine?",
        pos_text: "Raft snapshot frequency is every 2000 log entries",
        pos_auth: 0.91, pos_prov: "user_explicit", pos_key: Some("raft_snapshot_frequency"), pos_polarity: Some(1),
        neg_text: "Raft snapshot frequency is every 500 entries",
        neg_auth: 0.49, neg_prov: "contradicted", neg_key: Some("raft_snapshot_frequency"), neg_polarity: Some(1),
        supersedes_neg: true,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },

    // ── AUTHORITY-POLARITY (6) ─────────────────────────────

    ApprovalScenario {
        id: 217, domain: "Observability", class: ScenarioClass::AuthorityPolarity,
        query:    "Should the tracing exporter use a sidecar or a daemon?",
        pos_text: "Tracing exporter runs as a sidecar container",
        pos_auth: 0.85, pos_prov: "corrected", pos_key: Some("tracing_exporter_deployment"), pos_polarity: Some(1),
        neg_text: "Tracing exporter runs as a host daemon",
        neg_auth: 0.75, neg_prov: "contradicted", neg_key: None, neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 218, domain: "Storage/filesystem", class: ScenarioClass::AuthorityPolarity,
        query:    "Which RAID level should the backup array use?",
        pos_text: "Backup array uses RAID 6",
        pos_auth: 0.86, pos_prov: "corrected", pos_key: Some("backup_raid_level"), pos_polarity: Some(1),
        neg_text: "Backup array uses RAID 5",
        neg_auth: 0.74, neg_prov: "contradicted", neg_key: None, neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 219, domain: "Infrastructure/OS", class: ScenarioClass::AuthorityPolarity,
        query:    "What should the kernel I/O scheduler be for the database volume?",
        pos_text: "Database volume uses none I/O scheduler",
        pos_auth: 0.83, pos_prov: "corrected", pos_key: Some("db_io_scheduler"), pos_polarity: Some(1),
        neg_text: "Database volume uses mq-deadline I/O scheduler",
        neg_auth: 0.73, neg_prov: "contradicted", neg_key: None, neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 220, domain: "Database", class: ScenarioClass::AuthorityPolarity,
        query:    "What connection timeout should the MongoDB driver use?",
        pos_text: "MongoDB connection timeout is 10 seconds",
        pos_auth: 0.87, pos_prov: "corrected", pos_key: Some("mongodb_timeout"), pos_polarity: Some(1),
        neg_text: "MongoDB connection timeout is 30 seconds",
        neg_auth: 0.76, neg_prov: "contradicted", neg_key: None, neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 221, domain: "Network/TLS", class: ScenarioClass::AuthorityPolarity,
        query:    "How often should the internal proxy send TCP keepalive probes?",
        pos_text: "Internal proxy TCP keepalive interval is 300 seconds",
        pos_auth: 0.84, pos_prov: "corrected", pos_key: Some("proxy_tcp_keepalive"), pos_polarity: Some(1),
        neg_text: "Internal proxy TCP keepalive interval is 600 seconds",
        neg_auth: 0.72, neg_prov: "contradicted", neg_key: None, neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 241, domain: "Database", class: ScenarioClass::AuthorityPolarity,
        query:    "What should the connection pool maximum be for the analytics database?",
        pos_text: "Analytics DB connection pool maximum is 50",
        pos_auth: 0.86, pos_prov: "corrected", pos_key: Some("analytics_db_pool_max"), pos_polarity: Some(1),
        neg_text: "Analytics DB connection pool maximum is 100",
        neg_auth: 0.74, neg_prov: "contradicted", neg_key: None, neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },

    // ── ADVERSARIAL-AUTHORITY (6) ──────────────────────────

    ApprovalScenario {
        id: 222, domain: "Cloud/IAM", class: ScenarioClass::AdversarialAuthority,
        query:    "Should the IAM role be granted full S3 access?",
        pos_text: "IAM role has least-privilege S3 access (read-only to one bucket)",
        pos_auth: 0.80, pos_prov: "corrected", pos_key: Some("iam_role_s3_access"), pos_polarity: Some(-1),
        neg_text: "IAM role has full S3 access",
        neg_auth: 0.95, neg_prov: "user_explicit", neg_key: Some("iam_role_s3_access"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 223, domain: "Security/crypto", class: ScenarioClass::AdversarialAuthority,
        query:    "Should the private key be stored in a software keystore?",
        pos_text: "Private key is stored in an HSM",
        pos_auth: 0.80, pos_prov: "corrected", pos_key: Some("private_key_storage_method"), pos_polarity: Some(-1),
        neg_text: "Private key is stored in a software keystore",
        neg_auth: 0.95, neg_prov: "user_explicit", neg_key: Some("private_key_storage_method"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 224, domain: "Container orchestration", class: ScenarioClass::AdversarialAuthority,
        query:    "Is the container allowed to mount the host Docker socket?",
        pos_text: "Container cannot mount host Docker socket",
        pos_auth: 0.82, pos_prov: "corrected", pos_key: Some("container_mount_docker_socket"), pos_polarity: Some(-1),
        neg_text: "Container mounts host Docker socket",
        neg_auth: 0.96, neg_prov: "user_explicit", neg_key: Some("container_mount_docker_socket"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 225, domain: "Distributed consensus", class: ScenarioClass::AdversarialAuthority,
        query:    "Should the Raft implementation use static authentication keys?",
        pos_text: "Raft authentication uses dynamically rotated keys",
        pos_auth: 0.81, pos_prov: "corrected", pos_key: Some("raft_auth_key_rotation"), pos_polarity: Some(-1),
        neg_text: "Raft authentication uses static keys",
        neg_auth: 0.94, neg_prov: "user_explicit", neg_key: Some("raft_auth_key_rotation"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 226, domain: "Observability", class: ScenarioClass::AdversarialAuthority,
        query:    "Is the log stream accessible without authentication?",
        pos_text: "Log stream requires authentication",
        pos_auth: 0.79, pos_prov: "corrected", pos_key: Some("log_stream_auth_required"), pos_polarity: Some(-1),
        neg_text: "Log stream is unauthenticated",
        neg_auth: 0.93, neg_prov: "user_explicit", neg_key: Some("log_stream_auth_required"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 242, domain: "Cloud/IAM", class: ScenarioClass::AdversarialAuthority,
        query:    "Should the IAM user be allowed to create access keys?",
        pos_text: "IAM user cannot create access keys",
        pos_auth: 0.80, pos_prov: "corrected", pos_key: Some("iam_user_create_access_keys"), pos_polarity: Some(-1),
        neg_text: "IAM user can create access keys",
        neg_auth: 0.95, neg_prov: "user_explicit", neg_key: Some("iam_user_create_access_keys"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::AuthorityPolarity, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },

    // ── SAFE-ABSTENTION (6) ────────────────────────────────

    ApprovalScenario {
        id: 227, domain: "Storage/filesystem", class: ScenarioClass::SafeAbstention,
        query:    "Should the database volume use local or network file locking?",
        pos_text: "Database volume uses local file locking",
        pos_auth: 0.80, pos_prov: "user_explicit", pos_key: Some("db_volume_lock_mechanism"), pos_polarity: Some(1),
        neg_text: "Database volume uses network file locking",
        neg_auth: 0.78, neg_prov: "contradicted", neg_key: Some("db_volume_lock_mechanism"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 228, domain: "Infrastructure/OS", class: ScenarioClass::SafeAbstention,
        query:    "What should the default runlevel be for the application server?",
        pos_text: "Application server default runlevel is 3",
        pos_auth: 0.82, pos_prov: "user_explicit", pos_key: Some("app_server_runlevel"), pos_polarity: Some(1),
        neg_text: "Application server default runlevel is 5",
        neg_auth: 0.79, neg_prov: "contradicted", neg_key: Some("app_server_runlevel"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 229, domain: "Database", class: ScenarioClass::SafeAbstention,
        query:    "What is the optimal fetch size for the data access layer?",
        pos_text: "Data access layer fetch size is 100 rows",
        pos_auth: 0.83, pos_prov: "user_explicit", pos_key: Some("data_fetch_size_opt"), pos_polarity: Some(1),
        neg_text: "Data access layer fetch size is 500 rows",
        neg_auth: 0.80, neg_prov: "contradicted", neg_key: Some("data_fetch_size_opt"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 230, domain: "Network/TLS", class: ScenarioClass::SafeAbstention,
        query:    "What connection timeout should the SMTP relay use?",
        pos_text: "SMTP relay connection timeout is 10 seconds",
        pos_auth: 0.81, pos_prov: "user_explicit", pos_key: Some("smtp_relay_timeout"), pos_polarity: Some(1),
        neg_text: "SMTP relay connection timeout is 30 seconds",
        neg_auth: 0.77, neg_prov: "contradicted", neg_key: Some("smtp_relay_timeout"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 231, domain: "Cloud/IAM", class: ScenarioClass::SafeAbstention,
        query:    "What level of access should the IAM role have to the production database?",
        pos_text: "IAM role has read-only access to production database",
        pos_auth: 0.82, pos_prov: "user_explicit", pos_key: Some("iam_role_production_db_access"), pos_polarity: Some(1),
        neg_text: "IAM role has write access to production database",
        neg_auth: 0.80, neg_prov: "contradicted", neg_key: Some("iam_role_production_db_access"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },
    ApprovalScenario {
        id: 243, domain: "Network/TLS", class: ScenarioClass::SafeAbstention,
        query:    "Which TLS renegotiation policy should the server use?",
        pos_text: "TLS server uses secure renegotiation",
        pos_auth: 0.82, pos_prov: "user_explicit", pos_key: Some("tls_renegotiation_policy"), pos_polarity: Some(1),
        neg_text: "TLS server uses legacy renegotiation",
        neg_auth: 0.80, neg_prov: "contradicted", neg_key: Some("tls_renegotiation_policy"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::LogicalContradiction, expected_abstain: true, expected_winner: ExpectedWinner::None,
    },

    // ── UNRESOLVED-CONFLICT (6) ────────────────────────────

    ApprovalScenario {
        id: 232, domain: "Security/crypto", class: ScenarioClass::UnresolvedConflict,
        query:    "Which key exchange algorithm should be used for the VPN?",
        pos_text: "VPN uses ECDHE key exchange",
        pos_auth: 0.85, pos_prov: "user_explicit", pos_key: Some("vpn_key_exchange_algorithm"), pos_polarity: Some(1),
        neg_text: "VPN uses DHE key exchange",
        neg_auth: 0.80, neg_prov: "user_explicit", neg_key: Some("vpn_key_exchange_algorithm"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 233, domain: "Container orchestration", class: ScenarioClass::UnresolvedConflict,
        query:    "What should the termination grace period be for worker pods?",
        pos_text: "Worker pod termination grace period is 30 seconds",
        pos_auth: 0.82, pos_prov: "user_explicit", pos_key: Some("worker_grace_period"), pos_polarity: Some(1),
        neg_text: "Worker pod termination grace period is 60 seconds",
        neg_auth: 0.78, neg_prov: "user_explicit", neg_key: Some("worker_grace_period"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 234, domain: "Distributed consensus", class: ScenarioClass::UnresolvedConflict,
        query:    "How long should the consensus group wait before declaring a leader election?",
        pos_text: "Consensus election timeout is 1 second",
        pos_auth: 0.81, pos_prov: "user_explicit", pos_key: Some("consensus_election_timeout_value"), pos_polarity: Some(1),
        neg_text: "Consensus election timeout is 2 seconds",
        neg_auth: 0.79, neg_prov: "user_explicit", neg_key: Some("consensus_election_timeout_value"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 235, domain: "Observability", class: ScenarioClass::UnresolvedConflict,
        query:    "How many days should metrics be retained before deletion?",
        pos_text: "Metrics retention period is 90 days",
        pos_auth: 0.83, pos_prov: "user_explicit", pos_key: Some("metrics_retention_period"), pos_polarity: Some(1),
        neg_text: "Metrics retention period is 180 days",
        neg_auth: 0.77, neg_prov: "user_explicit", neg_key: Some("metrics_retention_period"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 236, domain: "Storage/filesystem", class: ScenarioClass::UnresolvedConflict,
        query:    "How often should snapshots be taken of the database volume?",
        pos_text: "Database volume snapshot frequency is every 6 hours",
        pos_auth: 0.84, pos_prov: "user_explicit", pos_key: Some("db_snapshot_frequency_value"), pos_polarity: Some(1),
        neg_text: "Database volume snapshot frequency is every 12 hours",
        neg_auth: 0.76, neg_prov: "user_explicit", neg_key: Some("db_snapshot_frequency_value"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
    },
    ApprovalScenario {
        id: 237, domain: "Infrastructure/OS", class: ScenarioClass::UnresolvedConflict,
        query:    "What init system should the container host use?",
        pos_text: "Container host uses systemd",
        pos_auth: 0.80, pos_prov: "user_explicit", pos_key: Some("container_host_init_system"), pos_polarity: Some(1),
        neg_text: "Container host uses SysV init",
        neg_auth: 0.78, neg_prov: "user_explicit", neg_key: Some("container_host_init_system"), neg_polarity: Some(1),
        supersedes_neg: false,
        expected_conflict: ExpectedConflict::None, expected_abstain: false, expected_winner: ExpectedWinner::Pos,
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

// ── Stage 1.5: Pre-run conformance validator (Protocol v1.8 Section 12) ─────────────
//
// Validates every scenario in SCENARIOS against the Section 4.4 class-spec matrix.
// Called by verify_phase3_approval.ps1 before Stage 2. Failure terminates the attempt
// BEFORE the single run is consumed. No corpus content is inspected by Role A.

#[test]
fn phase3_conformance_check() {
    // ── Global: corpus size ────────────────────────────────────────────────────
    assert!(
        !SCENARIOS.is_empty(),
        "CONFORMANCE_CHECK|FAIL|global|field=corpus_size|value=0|expected=>=40 — SCENARIOS is empty"
    );
    assert!(
        SCENARIOS.len() >= 40,
        "CONFORMANCE_CHECK|FAIL|global|field=corpus_size|value={}|expected=>=40",
        SCENARIOS.len()
    );

    // ── Global: unique IDs ─────────────────────────────────────────────────────
    {
        let mut seen_ids = std::collections::HashSet::new();
        for s in SCENARIOS {
            assert!(
                seen_ids.insert(s.id),
                "CONFORMANCE_CHECK|FAIL|id={}|field=id|value={}|expected=unique",
                s.id, s.id
            );
        }
    }

    // ── Global: unique queries (case-insensitive) ──────────────────────────────
    {
        let mut seen_queries = std::collections::HashSet::new();
        for s in SCENARIOS {
            let lower = s.query.to_lowercase();
            assert!(
                seen_queries.insert(lower.clone()),
                "CONFORMANCE_CHECK|FAIL|id={}|field=query|value=duplicate|expected=unique",
                s.id
            );
        }
    }

    // ── Global: domain cap (<= 6 per domain) ──────────────────────────────────
    {
        let mut domain_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for s in SCENARIOS {
            *domain_counts.entry(s.domain).or_insert(0) += 1;
        }
        for (domain, count) in &domain_counts {
            assert!(
                *count <= 6,
                "CONFORMANCE_CHECK|FAIL|global|field=domain_cap|value={}:{}|expected=<=6",
                domain, count
            );
        }
    }

    // ── Global: per-class minimums ─────────────────────────────────────────────
    {
        use ScenarioClass::*;
        let counts = |cls: ScenarioClass| SCENARIOS.iter().filter(|s| s.class == cls).count();
        let contradiction_n = counts(Contradiction);
        let supersession_n  = counts(Supersession);
        let auth_pol_n      = counts(AuthorityPolarity);
        let adv_auth_n      = counts(AdversarialAuthority);
        let safe_abs_n      = counts(SafeAbstention);
        let unresolved_n    = counts(UnresolvedConflict);

        assert!(contradiction_n >= 10,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_Contradiction|value={}|expected=>=10",
            contradiction_n);
        assert!(supersession_n >= 6,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_Supersession|value={}|expected=>=6",
            supersession_n);
        assert!(auth_pol_n >= 6,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_AuthorityPolarity|value={}|expected=>=6",
            auth_pol_n);
        assert!(adv_auth_n >= 6,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_AdversarialAuthority|value={}|expected=>=6",
            adv_auth_n);
        assert!(safe_abs_n >= 6,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_SafeAbstention|value={}|expected=>=6",
            safe_abs_n);
        assert!(unresolved_n >= 6,
            "CONFORMANCE_CHECK|FAIL|global|field=class_min_UnresolvedConflict|value={}|expected=>=6",
            unresolved_n);

        println!(
            "CONFORMANCE_COUNTS|N={}|Contradiction:{},Supersession:{},\
             AuthorityPolarity:{},AdversarialAuthority:{},SafeAbstention:{},\
             UnresolvedConflict:{}",
            SCENARIOS.len(),
            contradiction_n, supersession_n, auth_pol_n,
            adv_auth_n, safe_abs_n, unresolved_n
        );
    }

    // ── Per-scenario validation ────────────────────────────────────────────────
    let mut checked = 0usize;
    for s in SCENARIOS {
        let id = s.id;
        match s.class {

            // ── CONTRADICTION ──────────────────────────────────────────────────
            // Section 12.3: pos_prov=user_explicit, pos_polarity=Some(1), pos_auth>=0.80,
            //   neg_auth in [0.70,0.85], neg_prov=contradicted, neg_polarity=Some(-1),
            //   neg_key==pos_key, supersedes_neg=false.
            ScenarioClass::Contradiction => {
                assert_eq!(s.pos_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} Contradiction field=pos_prov: {:?} -- expected user_explicit",
                    id, s.pos_prov);
                assert_eq!(s.pos_polarity, Some(1),
                    "CONFORMANCE FAIL id={} Contradiction field=pos_polarity: {:?} -- expected Some(1)",
                    id, s.pos_polarity);
                assert!(s.pos_auth >= 0.80,
                    "CONFORMANCE FAIL id={} Contradiction field=pos_auth: {:.3} -- expected >=0.80",
                    id, s.pos_auth);
                assert!(s.neg_auth >= 0.70 && s.neg_auth <= 0.85,
                    "CONFORMANCE FAIL id={} Contradiction field=neg_auth: {:.3} -- expected 0.70..=0.85",
                    id, s.neg_auth);
                assert_eq!(s.neg_prov, "contradicted",
                    "CONFORMANCE FAIL id={} Contradiction field=neg_prov: {:?} -- expected contradicted",
                    id, s.neg_prov);
                assert_eq!(s.neg_polarity, Some(-1),
                    "CONFORMANCE FAIL id={} Contradiction field=neg_polarity: {:?} -- expected Some(-1)",
                    id, s.neg_polarity);
                assert_eq!(s.neg_key, s.pos_key,
                    "CONFORMANCE FAIL id={} Contradiction field=neg_key: {:?} -- expected == pos_key {:?}",
                    id, s.neg_key, s.pos_key);
                assert!(!s.supersedes_neg,
                    "CONFORMANCE FAIL id={} Contradiction field=supersedes_neg: true -- expected false",
                    id);
            }

            // ── SUPERSESSION ───────────────────────────────────────────────────
            // Section 12.3: pos_prov=user_explicit, neg_prov=contradicted,
            //   pos_auth >= neg_auth + 0.05, neg_polarity==pos_polarity,
            //   supersedes_neg=true.
            ScenarioClass::Supersession => {
                assert_eq!(s.pos_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} Supersession field=pos_prov: {:?} -- expected user_explicit",
                    id, s.pos_prov);
                assert_eq!(s.neg_prov, "contradicted",
                    "CONFORMANCE FAIL id={} Supersession field=neg_prov: {:?} -- expected contradicted",
                    id, s.neg_prov);
                assert!(s.pos_auth >= s.neg_auth + 0.05,
                    "CONFORMANCE FAIL id={} Supersession field=auth_gap: pos={:.3} neg={:.3} -- expected pos>=neg+0.05",
                    id, s.pos_auth, s.neg_auth);
                assert_eq!(s.neg_polarity, s.pos_polarity,
                    "CONFORMANCE FAIL id={} Supersession field=polarity_match: neg={:?} pos={:?} -- must be equal (same-fact temporal replacement)",
                    id, s.neg_polarity, s.pos_polarity);
                assert!(s.supersedes_neg,
                    "CONFORMANCE FAIL id={} Supersession field=supersedes_neg: false -- expected true",
                    id);
            }

            // ── AUTHORITY-POLARITY ─────────────────────────────────────────────
            // Section 12.3: pos_auth in [0.80,0.90], pos_prov in {user_explicit,corrected},
            //   neg_auth in [0.70,0.80], neg_prov=contradicted,
            //   neg_key!=pos_key OR neg_key==None, supersedes_neg=false.
            ScenarioClass::AuthorityPolarity => {
                assert!(s.pos_auth >= 0.80 && s.pos_auth <= 0.90,
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=pos_auth: {:.3} -- expected 0.80..=0.90",
                    id, s.pos_auth);
                assert!(s.pos_prov == "user_explicit" || s.pos_prov == "corrected",
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=pos_prov: {:?} -- expected user_explicit or corrected",
                    id, s.pos_prov);
                assert!(s.neg_auth >= 0.70 && s.neg_auth <= 0.80,
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=neg_auth: {:.3} -- expected 0.70..=0.80",
                    id, s.neg_auth);
                assert_eq!(s.neg_prov, "contradicted",
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=neg_prov: {:?} -- expected contradicted",
                    id, s.neg_prov);
                assert!(s.neg_key != s.pos_key || s.neg_key.is_none(),
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=neg_key: {:?} -- expected different from pos_key {:?} or None",
                    id, s.neg_key, s.pos_key);
                assert!(!s.supersedes_neg,
                    "CONFORMANCE FAIL id={} AuthorityPolarity field=supersedes_neg: true -- expected false",
                    id);
            }

            // ── ADVERSARIAL-AUTHORITY ──────────────────────────────────────────
            // Section 12.3: pos_auth in [0.75,0.85], pos_prov=corrected,
            //   neg_auth in [0.90,1.00], neg_prov=user_explicit,
            //   neg_auth > pos_auth, supersedes_neg=false.
            ScenarioClass::AdversarialAuthority => {
                assert!(s.pos_auth >= 0.75 && s.pos_auth <= 0.85,
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=pos_auth: {:.3} -- expected 0.75..=0.85",
                    id, s.pos_auth);
                assert_eq!(s.pos_prov, "corrected",
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=pos_prov: {:?} -- expected corrected",
                    id, s.pos_prov);
                assert!(s.neg_auth >= 0.90 && s.neg_auth <= 1.00,
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=neg_auth: {:.3} -- expected 0.90..=1.00",
                    id, s.neg_auth);
                assert_eq!(s.neg_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=neg_prov: {:?} -- expected user_explicit (adversarial high-authority assertion)",
                    id, s.neg_prov);
                assert!(s.neg_auth > s.pos_auth,
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=auth_ordering: neg={:.3} pos={:.3} -- expected neg_auth > pos_auth",
                    id, s.neg_auth, s.pos_auth);
                assert!(!s.supersedes_neg,
                    "CONFORMANCE FAIL id={} AdversarialAuthority field=supersedes_neg: true -- expected false",
                    id);
            }

            // ── SAFE-ABSTENTION ────────────────────────────────────────────────
            // Section 12.3: pos_auth in [0.75,0.90], pos_prov=user_explicit,
            //   neg_auth in [0.75,0.87], neg_prov=contradicted,
            //   |pos_auth-neg_auth|<=0.10, supersedes_neg=false.
            // NOTE: no pos_auth > neg_auth ordering constraint (near-equal is the spec;
            //   either may be slightly higher -- see Section 12.3 annotation).
            ScenarioClass::SafeAbstention => {
                assert!(s.pos_auth >= 0.75 && s.pos_auth <= 0.90,
                    "CONFORMANCE FAIL id={} SafeAbstention field=pos_auth: {:.3} -- expected 0.75..=0.90",
                    id, s.pos_auth);
                assert_eq!(s.pos_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} SafeAbstention field=pos_prov: {:?} -- expected user_explicit",
                    id, s.pos_prov);
                assert!(s.neg_auth >= 0.75 && s.neg_auth <= 0.87,
                    "CONFORMANCE FAIL id={} SafeAbstention field=neg_auth: {:.3} -- expected 0.75..=0.87",
                    id, s.neg_auth);
                assert_eq!(s.neg_prov, "contradicted",
                    "CONFORMANCE FAIL id={} SafeAbstention field=neg_prov: {:?} -- expected contradicted",
                    id, s.neg_prov);
                assert!((s.pos_auth - s.neg_auth).abs() <= 0.10,
                    "CONFORMANCE FAIL id={} SafeAbstention field=auth_gap: |{:.3}-{:.3}|={:.3} -- expected <=0.10",
                    id, s.pos_auth, s.neg_auth, (s.pos_auth - s.neg_auth).abs());
                assert!(!s.supersedes_neg,
                    "CONFORMANCE FAIL id={} SafeAbstention field=supersedes_neg: true -- expected false",
                    id);
            }

            // ── UNRESOLVED-CONFLICT ────────────────────────────────────────────
            // Section 12.3: pos_auth in [0.80,1.00], pos_prov=user_explicit,
            //   neg_auth in [0.75,0.85], neg_prov=user_explicit,
            //   pos_auth > neg_auth, no polarity clash, supersedes_neg=false.
            ScenarioClass::UnresolvedConflict => {
                assert!(s.pos_auth >= 0.80 && s.pos_auth <= 1.00,
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=pos_auth: {:.3} -- expected 0.80..=1.00",
                    id, s.pos_auth);
                assert_eq!(s.pos_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=pos_prov: {:?} -- expected user_explicit",
                    id, s.pos_prov);
                assert!(s.neg_auth >= 0.75 && s.neg_auth <= 0.85,
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=neg_auth: {:.3} -- expected 0.75..=0.85",
                    id, s.neg_auth);
                assert_eq!(s.neg_prov, "user_explicit",
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=neg_prov: {:?} -- expected user_explicit",
                    id, s.neg_prov);
                assert!(s.pos_auth > s.neg_auth,
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=auth_ordering: pos={:.3} neg={:.3} -- expected pos_auth > neg_auth",
                    id, s.pos_auth, s.neg_auth);
                // Polarity clash check: must NOT have (pos=+1, neg=-1) or (pos=-1, neg=+1)
                let clash = matches!(
                    (s.pos_polarity, s.neg_polarity),
                    (Some(1), Some(-1)) | (Some(-1), Some(1))
                );
                assert!(!clash,
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=polarity_clash: pos={:?} neg={:?} -- expected no clash",
                    id, s.pos_polarity, s.neg_polarity);
                assert!(!s.supersedes_neg,
                    "CONFORMANCE FAIL id={} UnresolvedConflict field=supersedes_neg: true -- expected false",
                    id);
            }
        }
        checked += 1;
    }

    // ── Self-integrity: all scenarios were checked ─────────────────────────────
    assert_eq!(checked, SCENARIOS.len(),
        "CONFORMANCE_CHECK|FAIL|global|field=self_integrity: checked={} but SCENARIOS.len()={}",
        checked, SCENARIOS.len());

    println!(
        "CONFORMANCE_CHECK|PASS|N={}|all_per_scenario_checks=ok|domains_ok=true",
        SCENARIOS.len()
    );
}
