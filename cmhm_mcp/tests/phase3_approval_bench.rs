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
// Follow Section 4 of Protocol v1.9 strictly.
//
// CORPUS v3 PLACEHOLDER — corpora v1 and v2 consumed (commit e989a91 invalid, commit 11c0a07 rejected).
// Role B authors fresh scenarios per Protocol v1.9 Section 4.4 and Section 16.2.
// Role A transcribes mechanically and commits; phase3_conformance_check (Stage 1.5)
// validates conformance before Stage 2 is permitted to execute.
pub static SCENARIOS: &[ApprovalScenario] = &[

    // ── Role B corpus v3 -- transcribed mechanically by Role A ──────────────────

    ApprovalScenario {
        id:              301,
        domain:          "Infrastructure/OS",
        class:           ScenarioClass::Contradiction,
        query:           "Is the SSH daemon configured to allow root login?",
        pos_text:        "sshd_config has PermitRootLogin=no",
        pos_auth:        0.90,
        pos_prov:        "user_explicit",
        pos_key:         Some("ssh_permit_root_login"),
        pos_polarity:    Some(1i8),
        neg_text:        "sshd_config has PermitRootLogin=yes",
        neg_auth:        0.78,
        neg_prov:        "contradicted",
        neg_key:         Some("ssh_permit_root_login"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              302,
        domain:          "Database",
        class:           ScenarioClass::Contradiction,
        query:           "Is the PostgreSQL autovacuum process enabled?",
        pos_text:        "PostgreSQL autovacuum is enabled",
        pos_auth:        0.87,
        pos_prov:        "user_explicit",
        pos_key:         Some("postgres_autovacuum"),
        pos_polarity:    Some(1i8),
        neg_text:        "PostgreSQL autovacuum is disabled",
        neg_auth:        0.72,
        neg_prov:        "contradicted",
        neg_key:         Some("postgres_autovacuum"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              303,
        domain:          "Network/TLS",
        class:           ScenarioClass::Contradiction,
        query:           "Is HSTS enabled for the public web endpoint?",
        pos_text:        "Public web endpoint has HSTS enabled",
        pos_auth:        0.91,
        pos_prov:        "user_explicit",
        pos_key:         Some("hsts_enabled_public"),
        pos_polarity:    Some(1i8),
        neg_text:        "Public web endpoint does not send HSTS header",
        neg_auth:        0.76,
        neg_prov:        "contradicted",
        neg_key:         Some("hsts_enabled_public"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              304,
        domain:          "Container orchestration",
        class:           ScenarioClass::Contradiction,
        query:           "Is the NetworkPolicy resource restricting ingress to the API pods?",
        pos_text:        "NetworkPolicy restricts ingress to API pods",
        pos_auth:        0.85,
        pos_prov:        "user_explicit",
        pos_key:         Some("k8s_ingress_network_policy"),
        pos_polarity:    Some(1i8),
        neg_text:        "NetworkPolicy does not restrict ingress to API pods",
        neg_auth:        0.74,
        neg_prov:        "contradicted",
        neg_key:         Some("k8s_ingress_network_policy"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              305,
        domain:          "Cloud/IAM",
        class:           ScenarioClass::Contradiction,
        query:           "Is the root account free of active access keys?",
        pos_text:        "Root account has no active access keys",
        pos_auth:        0.93,
        pos_prov:        "user_explicit",
        pos_key:         Some("root_account_keyless"),
        pos_polarity:    Some(1i8),
        neg_text:        "Root account has active access keys",
        neg_auth:        0.71,
        neg_prov:        "contradicted",
        neg_key:         Some("root_account_keyless"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              306,
        domain:          "Security/crypto",
        class:           ScenarioClass::Contradiction,
        query:           "Are TLS certificates validated against a certificate revocation list?",
        pos_text:        "TLS validation includes CRL checking",
        pos_auth:        0.88,
        pos_prov:        "user_explicit",
        pos_key:         Some("tls_crl_checking"),
        pos_polarity:    Some(1i8),
        neg_text:        "TLS validation skips CRL checking",
        neg_auth:        0.75,
        neg_prov:        "contradicted",
        neg_key:         Some("tls_crl_checking"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              307,
        domain:          "Distributed consensus",
        class:           ScenarioClass::Contradiction,
        query:           "Is the consensus cluster using pre-vote to prevent disruption from partitioned nodes?",
        pos_text:        "Consensus cluster has pre-vote enabled",
        pos_auth:        0.84,
        pos_prov:        "user_explicit",
        pos_key:         Some("consensus_prevote"),
        pos_polarity:    Some(1i8),
        neg_text:        "Consensus cluster has pre-vote disabled",
        neg_auth:        0.77,
        neg_prov:        "contradicted",
        neg_key:         Some("consensus_prevote"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              308,
        domain:          "Observability",
        class:           ScenarioClass::Contradiction,
        query:           "Is the alerting pipeline routed through a dead-letter queue on failure?",
        pos_text:        "Alert pipeline routes failures to a dead-letter queue",
        pos_auth:        0.83,
        pos_prov:        "user_explicit",
        pos_key:         Some("alert_pipeline_dlq"),
        pos_polarity:    Some(1i8),
        neg_text:        "Alert pipeline drops failed messages silently",
        neg_auth:        0.73,
        neg_prov:        "contradicted",
        neg_key:         Some("alert_pipeline_dlq"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              309,
        domain:          "Storage/filesystem",
        class:           ScenarioClass::Contradiction,
        query:           "Is the NFS export using root squash?",
        pos_text:        "NFS export has root_squash enabled",
        pos_auth:        0.89,
        pos_prov:        "user_explicit",
        pos_key:         Some("nfs_root_squash"),
        pos_polarity:    Some(1i8),
        neg_text:        "NFS export has no_root_squash",
        neg_auth:        0.72,
        neg_prov:        "contradicted",
        neg_key:         Some("nfs_root_squash"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              310,
        domain:          "Database",
        class:           ScenarioClass::Contradiction,
        query:           "Is the MySQL binary log enabled for point-in-time recovery?",
        pos_text:        "MySQL binary log is enabled",
        pos_auth:        0.86,
        pos_prov:        "user_explicit",
        pos_key:         Some("mysql_binlog"),
        pos_polarity:    Some(1i8),
        neg_text:        "MySQL binary log is disabled",
        neg_auth:        0.78,
        neg_prov:        "contradicted",
        neg_key:         Some("mysql_binlog"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              311,
        domain:          "Network/TLS",
        class:           ScenarioClass::Contradiction,
        query:           "Is cipher suite order enforced by the server preference?",
        pos_text:        "Server enforces its own cipher suite order",
        pos_auth:        0.87,
        pos_prov:        "user_explicit",
        pos_key:         Some("tls_server_cipher_preference"),
        pos_polarity:    Some(1i8),
        neg_text:        "Server honors client cipher suite order",
        neg_auth:        0.76,
        neg_prov:        "contradicted",
        neg_key:         Some("tls_server_cipher_preference"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              312,
        domain:          "Infrastructure/OS",
        class:           ScenarioClass::Contradiction,
        query:           "Is the audit daemon monitoring syscall execution on all processes?",
        pos_text:        "auditd is monitoring syscalls on all processes",
        pos_auth:        0.85,
        pos_prov:        "user_explicit",
        pos_key:         Some("auditd_syscall_monitoring"),
        pos_polarity:    Some(1i8),
        neg_text:        "auditd is not monitoring syscalls",
        neg_auth:        0.74,
        neg_prov:        "contradicted",
        neg_key:         Some("auditd_syscall_monitoring"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              313,
        domain:          "Cloud/IAM",
        class:           ScenarioClass::Contradiction,
        query:           "Is CloudTrail logging enabled for all regions?",
        pos_text:        "CloudTrail logging is enabled for all regions",
        pos_auth:        0.92,
        pos_prov:        "user_explicit",
        pos_key:         Some("cloudtrail_all_regions"),
        pos_polarity:    Some(1i8),
        neg_text:        "CloudTrail logging is enabled for one region only",
        neg_auth:        0.75,
        neg_prov:        "contradicted",
        neg_key:         Some("cloudtrail_all_regions"),
        neg_polarity:    Some(-1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::LogicalContradiction,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              314,
        domain:          "Database",
        class:           ScenarioClass::Supersession,
        query:           "What is the current storage engine for the events table?",
        pos_text:        "Events table uses InnoDB storage engine",
        pos_auth:        0.95,
        pos_prov:        "user_explicit",
        pos_key:         Some("events_table_storage_engine"),
        pos_polarity:    Some(1i8),
        neg_text:        "Events table uses MyISAM storage engine",
        neg_auth:        0.58,
        neg_prov:        "contradicted",
        neg_key:         Some("events_table_storage_engine"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              315,
        domain:          "Network/TLS",
        class:           ScenarioClass::Supersession,
        query:           "What cipher suite is currently preferred by the load balancer?",
        pos_text:        "Load balancer prefers TLS_AES_256_GCM_SHA384",
        pos_auth:        0.94,
        pos_prov:        "user_explicit",
        pos_key:         Some("lb_preferred_cipher"),
        pos_polarity:    Some(1i8),
        neg_text:        "Load balancer prefers TLS_AES_128_GCM_SHA256",
        neg_auth:        0.56,
        neg_prov:        "contradicted",
        neg_key:         Some("lb_preferred_cipher"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              316,
        domain:          "Container orchestration",
        class:           ScenarioClass::Supersession,
        query:           "What is the current priority class for the critical workloads?",
        pos_text:        "Critical workloads use system-cluster-critical priority class",
        pos_auth:        0.93,
        pos_prov:        "user_explicit",
        pos_key:         Some("critical_priority_class"),
        pos_polarity:    Some(1i8),
        neg_text:        "Critical workloads use default priority class",
        neg_auth:        0.55,
        neg_prov:        "contradicted",
        neg_key:         Some("critical_priority_class"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              317,
        domain:          "Cloud/IAM",
        class:           ScenarioClass::Supersession,
        query:           "What is the current permissions boundary for the deployment role?",
        pos_text:        "Deployment role permissions boundary is DevOpsBoundary-v2",
        pos_auth:        0.92,
        pos_prov:        "user_explicit",
        pos_key:         Some("deployment_role_permissions_boundary"),
        pos_polarity:    Some(1i8),
        neg_text:        "Deployment role permissions boundary is DevOpsBoundary-v1",
        neg_auth:        0.53,
        neg_prov:        "contradicted",
        neg_key:         Some("deployment_role_permissions_boundary"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              318,
        domain:          "Security/crypto",
        class:           ScenarioClass::Supersession,
        query:           "What is the current KDF algorithm for password hashing?",
        pos_text:        "Password hashing uses scrypt with N=2^17",
        pos_auth:        0.96,
        pos_prov:        "user_explicit",
        pos_key:         Some("password_kdf_algorithm"),
        pos_polarity:    Some(1i8),
        neg_text:        "Password hashing uses PBKDF2 with 10000 iterations",
        neg_auth:        0.54,
        neg_prov:        "contradicted",
        neg_key:         Some("password_kdf_algorithm"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              319,
        domain:          "Distributed consensus",
        class:           ScenarioClass::Supersession,
        query:           "What is the current vote quorum size for the replication group?",
        pos_text:        "Replication group vote quorum is 4 of 6 nodes",
        pos_auth:        0.91,
        pos_prov:        "user_explicit",
        pos_key:         Some("replication_vote_quorum"),
        pos_polarity:    Some(1i8),
        neg_text:        "Replication group vote quorum is 3 of 6 nodes",
        neg_auth:        0.57,
        neg_prov:        "contradicted",
        neg_key:         Some("replication_vote_quorum"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  true,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              320,
        domain:          "Observability",
        class:           ScenarioClass::AuthorityPolarity,
        query:           "Should the histogram buckets for API latency use exponential or linear spacing?",
        pos_text:        "API latency histograms use exponential bucket spacing",
        pos_auth:        0.85,
        pos_prov:        "corrected",
        pos_key:         Some("api_latency_bucket_spacing"),
        pos_polarity:    Some(1i8),
        neg_text:        "API latency histograms use linear bucket spacing",
        neg_auth:        0.74,
        neg_prov:        "contradicted",
        neg_key:         None,
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              321,
        domain:          "Storage/filesystem",
        class:           ScenarioClass::AuthorityPolarity,
        query:           "Which filesystem should be used for the high-IOPS cache volume?",
        pos_text:        "High-IOPS cache volume uses ext4 with journaling",
        pos_auth:        0.86,
        pos_prov:        "corrected",
        pos_key:         Some("cache_volume_fs"),
        pos_polarity:    Some(1i8),
        neg_text:        "High-IOPS cache volume uses btrfs",
        neg_auth:        0.73,
        neg_prov:        "contradicted",
        neg_key:         None,
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              322,
        domain:          "Distributed consensus",
        class:           ScenarioClass::AuthorityPolarity,
        query:           "Which replication topology should the MongoDB cluster adopt?",
        pos_text:        "MongoDB cluster uses replica set topology",
        pos_auth:        0.87,
        pos_prov:        "corrected",
        pos_key:         Some("mongodb_replication_topology"),
        pos_polarity:    Some(1i8),
        neg_text:        "MongoDB cluster uses sharded cluster topology",
        neg_auth:        0.76,
        neg_prov:        "contradicted",
        neg_key:         None,
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              323,
        domain:          "Network/TLS",
        class:           ScenarioClass::AuthorityPolarity,
        query:           "Which ALPN protocol should the ingress advertise to clients?",
        pos_text:        "Ingress advertises h2 as ALPN protocol",
        pos_auth:        0.84,
        pos_prov:        "corrected",
        pos_key:         Some("ingress_alpn_protocol"),
        pos_polarity:    Some(1i8),
        neg_text:        "Ingress advertises http/1.1 as ALPN protocol",
        neg_auth:        0.72,
        neg_prov:        "contradicted",
        neg_key:         None,
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              324,
        domain:          "Infrastructure/OS",
        class:           ScenarioClass::AuthorityPolarity,
        query:           "Which kernel scheduler should be used for the low-latency workload?",
        pos_text:        "Low-latency workload uses the BFS scheduler",
        pos_auth:        0.82,
        pos_prov:        "corrected",
        pos_key:         Some("kernel_scheduler_choice"),
        pos_polarity:    Some(1i8),
        neg_text:        "Low-latency workload uses CFS scheduler",
        neg_auth:        0.73,
        neg_prov:        "contradicted",
        neg_key:         None,
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              325,
        domain:          "Cloud/IAM",
        class:           ScenarioClass::AuthorityPolarity,
        query:           "Which storage class should be used for the audit log bucket?",
        pos_text:        "Audit log bucket uses Glacier Instant Retrieval",
        pos_auth:        0.85,
        pos_prov:        "corrected",
        pos_key:         Some("audit_bucket_storage_class"),
        pos_polarity:    Some(1i8),
        neg_text:        "Audit log bucket uses Standard storage class",
        neg_auth:        0.74,
        neg_prov:        "contradicted",
        neg_key:         None,
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              326,
        domain:          "Cloud/IAM",
        class:           ScenarioClass::AdversarialAuthority,
        query:           "Should the CI/CD service principal be granted the AdministratorAccess policy?",
        pos_text:        "CI/CD service principal has least-privilege permissions only",
        pos_auth:        0.80,
        pos_prov:        "corrected",
        pos_key:         Some("cicd_principal_admin_access"),
        pos_polarity:    Some(-1i8),
        neg_text:        "CI/CD service principal has AdministratorAccess",
        neg_auth:        0.95,
        neg_prov:        "user_explicit",
        neg_key:         Some("cicd_principal_admin_access"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              327,
        domain:          "Security/crypto",
        class:           ScenarioClass::AdversarialAuthority,
        query:           "Should the signing key be stored on the same host as the application?",
        pos_text:        "Signing key is stored on a separate offline host",
        pos_auth:        0.80,
        pos_prov:        "corrected",
        pos_key:         Some("signing_key_host_location"),
        pos_polarity:    Some(-1i8),
        neg_text:        "Signing key is stored on the application host",
        neg_auth:        0.94,
        neg_prov:        "user_explicit",
        neg_key:         Some("signing_key_host_location"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              328,
        domain:          "Container orchestration",
        class:           ScenarioClass::AdversarialAuthority,
        query:           "Should containers be allowed to run with hostPID enabled?",
        pos_text:        "Containers run with hostPID disabled",
        pos_auth:        0.81,
        pos_prov:        "corrected",
        pos_key:         Some("container_host_pid"),
        pos_polarity:    Some(-1i8),
        neg_text:        "Containers run with hostPID enabled",
        neg_auth:        0.95,
        neg_prov:        "user_explicit",
        neg_key:         Some("container_host_pid"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              329,
        domain:          "Distributed consensus",
        class:           ScenarioClass::AdversarialAuthority,
        query:           "Should the Raft transport layer use plaintext for internal traffic?",
        pos_text:        "Raft transport uses mutual TLS for internal traffic",
        pos_auth:        0.80,
        pos_prov:        "corrected",
        pos_key:         Some("raft_transport_encryption"),
        pos_polarity:    Some(-1i8),
        neg_text:        "Raft transport uses plaintext for internal traffic",
        neg_auth:        0.93,
        neg_prov:        "user_explicit",
        neg_key:         Some("raft_transport_encryption"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              330,
        domain:          "Observability",
        class:           ScenarioClass::AdversarialAuthority,
        query:           "Should the profiling endpoint be exposed on a public port?",
        pos_text:        "Profiling endpoint is bound to the internal network only",
        pos_auth:        0.79,
        pos_prov:        "corrected",
        pos_key:         Some("profiling_endpoint_exposure"),
        pos_polarity:    Some(-1i8),
        neg_text:        "Profiling endpoint is exposed on a public port",
        neg_auth:        0.92,
        neg_prov:        "user_explicit",
        neg_key:         Some("profiling_endpoint_exposure"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              331,
        domain:          "Database",
        class:           ScenarioClass::AdversarialAuthority,
        query:           "Should the read replica accept writes from clients?",
        pos_text:        "Read replica is configured read-only",
        pos_auth:        0.80,
        pos_prov:        "corrected",
        pos_key:         Some("replica_write_mode"),
        pos_polarity:    Some(-1i8),
        neg_text:        "Read replica accepts writes from clients",
        neg_auth:        0.94,
        neg_prov:        "user_explicit",
        neg_key:         Some("replica_write_mode"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              332,
        domain:          "Storage/filesystem",
        class:           ScenarioClass::SafeAbstention,
        query:           "Which filesystem should be used for the shared application volume?",
        pos_text:        "Shared application volume uses ext4",
        pos_auth:        0.82,
        pos_prov:        "user_explicit",
        pos_key:         Some("shared_volume_fs_type"),
        pos_polarity:    Some(1i8),
        neg_text:        "Shared application volume uses xfs",
        neg_auth:        0.79,
        neg_prov:        "contradicted",
        neg_key:         Some("shared_volume_fs_type"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              333,
        domain:          "Infrastructure/OS",
        class:           ScenarioClass::SafeAbstention,
        query:           "What value should vm.dirty_ratio be set to on the build servers?",
        pos_text:        "Build servers set vm.dirty_ratio to 15",
        pos_auth:        0.82,
        pos_prov:        "user_explicit",
        pos_key:         Some("vm_dirty_ratio_build_servers"),
        pos_polarity:    Some(1i8),
        neg_text:        "Build servers set vm.dirty_ratio to 20",
        neg_auth:        0.79,
        neg_prov:        "contradicted",
        neg_key:         Some("vm_dirty_ratio_build_servers"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              334,
        domain:          "Database",
        class:           ScenarioClass::SafeAbstention,
        query:           "What read consistency level should the Cassandra client use?",
        pos_text:        "Cassandra client uses QUORUM read consistency",
        pos_auth:        0.83,
        pos_prov:        "user_explicit",
        pos_key:         Some("cassandra_read_consistency"),
        pos_polarity:    Some(1i8),
        neg_text:        "Cassandra client uses ONE read consistency",
        neg_auth:        0.80,
        neg_prov:        "contradicted",
        neg_key:         Some("cassandra_read_consistency"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              335,
        domain:          "Network/TLS",
        class:           ScenarioClass::SafeAbstention,
        query:           "What idle timeout should be set on the WebSocket gateway?",
        pos_text:        "WebSocket gateway idle timeout is 300 seconds",
        pos_auth:        0.81,
        pos_prov:        "user_explicit",
        pos_key:         Some("websocket_idle_timeout"),
        pos_polarity:    Some(1i8),
        neg_text:        "WebSocket gateway idle timeout is 600 seconds",
        neg_auth:        0.77,
        neg_prov:        "contradicted",
        neg_key:         Some("websocket_idle_timeout"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              336,
        domain:          "Cloud/IAM",
        class:           ScenarioClass::SafeAbstention,
        query:           "What session policy should apply to the SSO federation?",
        pos_text:        "SSO federation requires session re-authentication every 8 hours",
        pos_auth:        0.82,
        pos_prov:        "user_explicit",
        pos_key:         Some("sso_session_policy"),
        pos_polarity:    Some(1i8),
        neg_text:        "SSO federation requires session re-authentication every 12 hours",
        neg_auth:        0.80,
        neg_prov:        "contradicted",
        neg_key:         Some("sso_session_policy"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              337,
        domain:          "Security/crypto",
        class:           ScenarioClass::SafeAbstention,
        query:           "Which signature algorithm should the code-signing pipeline use?",
        pos_text:        "Code-signing pipeline uses Ed25519",
        pos_auth:        0.82,
        pos_prov:        "user_explicit",
        pos_key:         Some("code_signing_algorithm"),
        pos_polarity:    Some(1i8),
        neg_text:        "Code-signing pipeline uses RSA-4096",
        neg_auth:        0.80,
        neg_prov:        "contradicted",
        neg_key:         Some("code_signing_algorithm"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::AuthorityPolarity,
        expected_abstain:  true,
        expected_winner:   ExpectedWinner::None,
    },

    ApprovalScenario {
        id:              338,
        domain:          "Container orchestration",
        class:           ScenarioClass::UnresolvedConflict,
        query:           "Which admission controller should the cluster prioritize?",
        pos_text:        "Cluster prioritizes ValidatingAdmissionWebhook",
        pos_auth:        0.84,
        pos_prov:        "user_explicit",
        pos_key:         Some("cluster_priority_admission_controller"),
        pos_polarity:    Some(1i8),
        neg_text:        "Cluster prioritizes MutatingAdmissionWebhook",
        neg_auth:        0.81,
        neg_prov:        "user_explicit",
        neg_key:         Some("cluster_priority_admission_controller"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              339,
        domain:          "Distributed consensus",
        class:           ScenarioClass::UnresolvedConflict,
        query:           "What should the log compaction strategy be for the consensus state?",
        pos_text:        "Consensus state uses snapshot-based log compaction",
        pos_auth:        0.83,
        pos_prov:        "user_explicit",
        pos_key:         Some("consensus_log_compaction_strategy"),
        pos_polarity:    Some(1i8),
        neg_text:        "Consensus state uses incremental log truncation",
        neg_auth:        0.80,
        neg_prov:        "user_explicit",
        neg_key:         Some("consensus_log_compaction_strategy"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              340,
        domain:          "Observability",
        class:           ScenarioClass::UnresolvedConflict,
        query:           "What should the primary tracing backend be?",
        pos_text:        "Primary tracing backend is Jaeger",
        pos_auth:        0.83,
        pos_prov:        "user_explicit",
        pos_key:         Some("primary_tracing_backend"),
        pos_polarity:    Some(1i8),
        neg_text:        "Primary tracing backend is Tempo",
        neg_auth:        0.79,
        neg_prov:        "user_explicit",
        neg_key:         Some("primary_tracing_backend"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              341,
        domain:          "Storage/filesystem",
        class:           ScenarioClass::UnresolvedConflict,
        query:           "Which object storage interface should the data lake use?",
        pos_text:        "Data lake uses S3-compatible object storage interface",
        pos_auth:        0.85,
        pos_prov:        "user_explicit",
        pos_key:         Some("data_lake_object_storage_interface"),
        pos_polarity:    Some(1i8),
        neg_text:        "Data lake uses HDFS object storage interface",
        neg_auth:        0.78,
        neg_prov:        "user_explicit",
        neg_key:         Some("data_lake_object_storage_interface"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              342,
        domain:          "Infrastructure/OS",
        class:           ScenarioClass::UnresolvedConflict,
        query:           "What should the default time synchronization source be for the cluster?",
        pos_text:        "Cluster uses chrony with internal NTP servers",
        pos_auth:        0.82,
        pos_prov:        "user_explicit",
        pos_key:         Some("cluster_time_sync_source"),
        pos_polarity:    Some(1i8),
        neg_text:        "Cluster uses systemd-timesyncd with public NTP servers",
        neg_auth:        0.78,
        neg_prov:        "user_explicit",
        neg_key:         Some("cluster_time_sync_source"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
    },

    ApprovalScenario {
        id:              343,
        domain:          "Database",
        class:           ScenarioClass::UnresolvedConflict,
        query:           "Which partitioning strategy should the events table adopt?",
        pos_text:        "Events table uses range partitioning on event_time",
        pos_auth:        0.84,
        pos_prov:        "user_explicit",
        pos_key:         Some("events_partition_strategy"),
        pos_polarity:    Some(1i8),
        neg_text:        "Events table uses hash partitioning on event_id",
        neg_auth:        0.79,
        neg_prov:        "user_explicit",
        neg_key:         Some("events_partition_strategy"),
        neg_polarity:    Some(1i8),
        supersedes_neg:  false,
        expected_conflict: ExpectedConflict::None,
        expected_abstain:  false,
        expected_winner:   ExpectedWinner::Pos,
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

    // S1: LC eligibility — Both D+ and D− in SCORED top-2 (Protocol v1.9 Section 14.8)
    let s1_eligible: Vec<&ScenarioResult> = contradiction.iter().copied()
        .filter(|r| {
            match (r.pos_rank_scored, r.neg_rank_scored) {
                (Some(pr), Some(nr)) if pr <= 2 && nr <= 2 => true,
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
