//! Append-only audit log (M4): who did what, when, to which scope — and whether
//! it worked. Metadata only, never memory content.
//!
//! One JSONL file per tenant (`audit.log` under the tenant dir). Each entry
//! chains the SHA-256 of the previous entry (`prev_hash` → `hash`), so
//! tampering with history breaks the chain. [`verify`] replays it.
//!
//! Audit writes are best-effort for availability (a failed append is logged
//! to tracing but never fails the request); completeness is enforced by
//! keeping the writer infallible-simple (single append, no rotation —
//! rotation policy is M-policy, not code).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// One audit entry. `detail` carries small structured facts (node counts,
/// redaction counts, error classes) — never text content, queries, or keys.
///
/// `ts_millis` is an INTEGER (not float) on purpose: the original `ts_secs:
/// f64` design hashed raw float bits, but an f64 → JSON → f64 round-trip is
/// not bit-stable through every parser (proven: serde_json parses
/// "1791781224.7492247" to different bits than it serialized, failing
/// verification ~20% of timestamp lottery draws). Integers round-trip
/// exactly, so the chain is deterministic. Pre-fix local audit.log files
/// (float `ts_secs`) no longer verify — expected and documented: those
/// chains were already probabilistically unverifiable, which is the bug.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub ts_millis: u64,
    pub principal_id: String,
    pub tenant_id: String,
    pub op: String,
    pub scope_id: String,
    pub outcome: String,
    pub detail: String,
    pub prev_hash: String,
    pub hash: String,
}

fn entry_hash(e: &AuditEntry) -> String {
    let mut h = Sha256::new();
    h.update(e.seq.to_be_bytes());
    h.update(e.ts_millis.to_be_bytes());
    h.update(e.principal_id.as_bytes());
    h.update(e.tenant_id.as_bytes());
    h.update(e.op.as_bytes());
    h.update(e.scope_id.as_bytes());
    h.update(e.outcome.as_bytes());
    h.update(e.detail.as_bytes());
    h.update(e.prev_hash.as_bytes());
    h.update([0xA0]);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Append one entry to the tenant's audit log. Creates directories as needed.
pub fn append(
    tenant_dir: &Path,
    principal_id: &str,
    tenant_id: &str,
    op: &str,
    scope_id: &str,
    outcome: &str,
    detail: &str,
) {
    let path = tenant_dir.join("audit.log");
    let prev_hash = last_hash(&path);
    let seq = next_seq(&path);
    let mut e = AuditEntry {
        seq,
        ts_millis: now_millis(),
        principal_id: principal_id.to_string(),
        tenant_id: tenant_id.to_string(),
        op: op.to_string(),
        scope_id: scope_id.to_string(),
        outcome: outcome.to_string(),
        detail: detail.to_string(),
        prev_hash,
        hash: String::new(),
    };
    e.hash = entry_hash(&e);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        if let Ok(line) = serde_json::to_string(&e) {
            let _ = writeln!(f, "{line}");
        }
    }
}

fn read_entries(path: &PathBuf) -> Vec<AuditEntry> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return vec![];
    };
    text.lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn last_hash(path: &PathBuf) -> String {
    read_entries(path)
        .last()
        .map(|e| e.hash.clone())
        .unwrap_or_else(|| "GENESIS".to_string())
}

fn next_seq(path: &PathBuf) -> u64 {
    read_entries(path).len() as u64
}

/// Verify the whole chain. Returns the entry count on success.
pub fn verify(tenant_dir: &Path) -> Result<usize> {
    let entries = read_entries(&tenant_dir.join("audit.log"));
    let mut prev = "GENESIS".to_string();
    for (i, e) in entries.iter().enumerate() {
        if e.seq != i as u64 {
            anyhow::bail!("audit chain broken at entry {i}: seq mismatch");
        }
        if e.prev_hash != prev {
            anyhow::bail!("audit chain broken at entry {i}: prev_hash mismatch");
        }
        if entry_hash(e) != e.hash {
            anyhow::bail!("audit chain broken at entry {i}: hash mismatch (tampered?)");
        }
        prev = e.hash.clone();
    }
    Ok(entries.len())
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    static AUDIT_DIR_COUNTER: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);

    /// Unique dir per test invocation (Windows lazy-delete races fixed dirs).
    fn unique_audit_dir(stem: &str) -> std::path::PathBuf {
        let n = AUDIT_DIR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "cmhm_audit_{stem}_{:?}_{n}",
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn chain_appends_and_verifies() {
        let dir = unique_audit_dir("t1");
        std::fs::create_dir_all(&dir).unwrap();
        append(&dir, "prn-1", "tnt-1", "learn", "scope-1", "ok", "nodes=3");
        append(&dir, "prn-1", "tnt-1", "retrieve", "scope-1", "ok", "seed=7");
        append(&dir, "prn-2", "tnt-1", "learn", "scope-1", "denied", "E_FORBIDDEN");
        assert_eq!(verify(&dir).unwrap(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tamper_breaks_chain() {
        let dir = unique_audit_dir("t2");
        std::fs::create_dir_all(&dir).unwrap();
        append(&dir, "prn-1", "tnt-1", "learn", "scope-1", "ok", "");
        append(&dir, "prn-1", "tnt-1", "learn", "scope-1", "ok", "");
        // Rewrite entry 2 with a forged outcome, keeping the old hash.
        let path = dir.join("audit.log");
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
        let mut v: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        v["outcome"] = serde_json::Value::String("denied".into());
        lines[1] = serde_json::to_string(&v).unwrap();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        assert!(verify(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
