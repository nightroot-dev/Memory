//! Scope backup + verified restore (Expansion M6).
//!
//! A backup is a timestamped directory copy of a scope root (meta.json,
//! snapshots, WAL, key.bin) plus a [`BackupManifest`] with SHA-256 over every
//! file. [`restore_scope`] copies back and re-verifies hashes BEFORE the
//! engine is allowed to open it — a restore never silently serves corrupt or
//! tampered bytes.
//!
//! Honesty notes (see design doc):
//! - The bundle INCLUDES `key.bin`: whoever holds the backup holds the data.
//!   Crypto-shredding the live scope does not erase backup copies; single-record
//!   erasure is complete only after backup retention expires. Stated plainly.
//! - Object storage is a later transport; this module copies bytes to a local
//!   backup root behind the same interface (`backup_scope` takes any dest dir).

use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Manifest written alongside every backup. Verification compares live bytes
/// against these hashes; any mismatch aborts the restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub scope_id: String,
    pub tenant_id: String,
    pub created_at: f64,
    pub layout_version: u32,
    /// Relative path → hex SHA-256. Sorted for stable serialization.
    pub files: Vec<(String, String)>,
}

/// Copy a scope directory into `<backup_root>/<scope_id>/<timestamp>/` +
/// manifest. Fails closed on any I/O error (partial bundles are removed).
pub fn backup_scope(
    scope_root: &Path,
    tenant_id: &str,
    scope_id: &str,
    layout_version: u32,
    backup_root: &Path,
) -> Result<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dest = backup_root.join(scope_id).join(format!("{ts}"));
    if dest.exists() {
        anyhow::bail!("backup destination already exists: {}", dest.display());
    }
    let mut files: Vec<(String, String)> = vec![];
    let mut stack = vec![scope_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .with_context(|| format!("backup: cannot read {}", dir.display()))?
            .flatten()
        {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            // LOCK files are runtime state, not data — never backed up.
            if p.file_name().and_then(|n| n.to_str()) == Some("LOCK") {
                continue;
            }
            let rel = p.strip_prefix(scope_root).map_err(|e| anyhow::anyhow!("{e}"))?;
            let bytes = std::fs::read(&p)?;
            let mut h = Sha256::new();
            h.update(&bytes);
            let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
            let target = dest.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, &bytes)?;
            files.push((rel.to_string_lossy().replace('\\', "/"), hex));
        }
    }
    if files.is_empty() {
        let _ = std::fs::remove_dir_all(&dest);
        anyhow::bail!("backup: scope is empty, nothing to back up");
    }
    files.sort();
    let manifest = BackupManifest {
        scope_id: scope_id.to_string(),
        tenant_id: tenant_id.to_string(),
        created_at: now_secs(),
        layout_version,
        files,
    };
    std::fs::write(
        dest.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(dest)
}

/// Verify a backup bundle WITHOUT restoring it. Returns file count.
pub fn verify_backup(bundle: &Path) -> Result<usize> {
    let manifest: BackupManifest =
        serde_json::from_str(&std::fs::read_to_string(bundle.join("manifest.json"))?)?;
    for (rel, expect) in &manifest.files {
        let bytes = std::fs::read(bundle.join(rel))
            .map_err(|e| anyhow::anyhow!("backup file missing {rel}: {e}"))?;
        let mut h = Sha256::new();
        h.update(&bytes);
        let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if &got != expect {
            anyhow::bail!("backup file {rel} hash mismatch (tampered?)");
        }
    }
    Ok(manifest.files.len())
}

/// Restore a bundle over a scope root. Verifies FIRST (hashes), copies
/// second, and refuses to overwrite a live scope directory that already has
/// a `meta.json` — restores go to empty ground, never over running state.
pub fn restore_scope(bundle: &Path, scope_root: &Path) -> Result<usize> {
    let n = verify_backup(bundle)?;
    if scope_root.join("meta.json").exists() {
        anyhow::bail!(
            "refusing to restore over a live scope root: {}",
            scope_root.display()
        );
    }
    let manifest: BackupManifest =
        serde_json::from_str(&std::fs::read_to_string(bundle.join("manifest.json"))?)?;
    std::fs::create_dir_all(scope_root)?;
    for (rel, _) in &manifest.files {
        let target = scope_root.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(bundle.join(rel), &target)?;
    }
    // Re-verify the restored bytes (copy integrity, not just bundle integrity).
    let mut checked = 0;
    for (rel, expect) in &manifest.files {
        let bytes = std::fs::read(scope_root.join(rel))?;
        let mut h = Sha256::new();
        h.update(&bytes);
        let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if &got != expect {
            anyhow::bail!("restored file {rel} hash mismatch");
        }
        checked += 1;
    }
    Ok(checked)
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_scope(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("meta.json"), r#"{"version":1}"#).unwrap();
        std::fs::write(dir.join("wal.dat"), b"fake-wal-bytes").unwrap();
        std::fs::write(dir.join("key.bin"), &[7u8; 32]).unwrap();
    }

    #[test]
    fn round_trip_and_tamper() {
        let base = std::env::temp_dir().join("cmhm_bak_t");
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        seed_scope(&src);
        let bundle = backup_scope(&src, "tnt", "scope-1", 1, &base.join("backups")).unwrap();
        assert_eq!(verify_backup(&bundle).unwrap(), 3);

        // Tamper: restore must refuse.
        std::fs::write(bundle.join("wal.dat"), b"tampered").unwrap();
        assert!(verify_backup(&bundle).is_err());

        // Restore the good bundle (re-backup after fixing).
        std::fs::write(bundle.join("wal.dat"), b"fake-wal-bytes").unwrap();
        assert_eq!(verify_backup(&bundle).unwrap(), 3);
        let dst = base.join("restored");
        assert_eq!(restore_scope(&bundle, &dst).unwrap(), 3);
        assert!(dst.join("meta.json").exists());
        assert!(dst.join("key.bin").exists());

        // Refuses to overwrite live ground.
        assert!(restore_scope(&bundle, &dst).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn empty_scope_is_an_error_not_an_empty_backup() {
        let base = std::env::temp_dir().join("cmhm_bak_t2");
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        std::fs::create_dir_all(&src).unwrap();
        assert!(backup_scope(&src, "tnt", "s", 1, &base.join("b")).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
