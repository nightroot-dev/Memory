//! Scope layout + exclusive writer lock (Expansion Foundation M1).
//!
//! One engine instance owns one scope. A scope is a directory:
//!
//! ```text
//! /data/tenants/<tenant_id>/scopes/<scope_id>/
//!     meta.json      layout version, model_id, key_ref, epoch, owner_host
//!     LOCK           exclusive OS-level writer lock (held while engine runs)
//!     wal/           WAL segments (engine-owned files live here in later stages)
//!     snapshots/     snapshots (same)
//! ```
//!
//! Rules enforced here:
//! - Tenant/scope IDs are opaque (UUID-style, generated, never client paths).
//! - Layout version mismatch is fail-closed (`Err`, never silent migration).
//! - A second writer for the same scope is refused loudly (`try_lock_exclusive`).
//! - Epoch is recorded and bumpable now; cross-host enforcement is Stage B.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{crypto::Keystore, persistence::register_scope_key};

/// On-disk layout version. Bump when the directory/file scheme changes;
/// `open()` refuses anything else so old and new code never misread a scope.
pub const SCOPE_LAYOUT_VERSION: u32 = 1;

const META_FILE: &str = "meta.json";
const LOCK_FILE: &str = "LOCK";

// ─── Opaque identifiers ─────────────────────────────────────────────────────

fn new_opaque_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    // UUID-style 8-4-4-4-12 hex (version/variant bits not set — opaque, not a real UUID).
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5],
        bytes[6], bytes[7], bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

/// Opaque tenant identifier. Never a name, path, or client-supplied string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(String);

impl TenantId {
    pub fn generate() -> Self {
        Self(new_opaque_id())
    }

    /// Re-import an ID that was generated earlier (e.g. read back from disk).
    /// Rejects anything usable as a path.
    pub fn import(s: &str) -> Result<Self> {
        reject_path_like(s, "tenant")?;
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque scope identifier, unique inside its tenant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScopeId(String);

impl ScopeId {
    pub fn generate() -> Self {
        Self(new_opaque_id())
    }

    /// Re-import an existing ID (single-user mode maps context UUIDs to scope IDs).
    pub fn import(s: &str) -> Result<Self> {
        reject_path_like(s, "scope")?;
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn reject_path_like(s: &str, what: &str) -> Result<()> {
    if s.is_empty() || s.contains('/') || s.contains('\\') || s.contains("..") {
        anyhow::bail!("{what} ID must be opaque (no paths): {s:?}");
    }
    Ok(())
}

// ─── Scope metadata ─────────────────────────────────────────────────────────

/// Persisted scope metadata (`meta.json`). Versioned so layout changes fail closed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeMeta {
    /// Must equal [`SCOPE_LAYOUT_VERSION`] or `open()` refuses the scope.
    pub version: u32,
    /// Embedder model pinned to this scope (e.g. `"hash-128d"`). Never changes in place.
    pub model_id: String,
    /// Reference to the scope's data key (never the key itself). Empty until M3.
    #[serde(default)]
    pub key_ref: String,
    /// Monotonic epoch for fencing. Bumped on ownership changes; cross-host
    /// enforcement arrives in Stage B. Recorded now so the interface is stable.
    #[serde(default)]
    pub epoch: u64,
    /// Hostname that created/owns the scope (informational until Stage B).
    #[serde(default)]
    pub owner_host: String,
    /// Unix seconds at creation.
    pub created_at: f64,
}

// ─── Layout ─────────────────────────────────────────────────────────────────

/// Resolved on-disk layout for one scope.
#[derive(Debug, Clone)]
pub struct ScopeLayout {
    root: PathBuf,
    tenant: TenantId,
    scope: ScopeId,
}

impl ScopeLayout {
    fn root_for(data_root: &Path, tenant: &TenantId, scope: &ScopeId) -> PathBuf {
        data_root
            .join("tenants")
            .join(tenant.as_str())
            .join("scopes")
            .join(scope.as_str())
    }

    /// Create a brand-new scope (directories + `meta.json`). Errors if the scope
    /// already has a `meta.json` — creation never overwrites.
    pub fn create(
        data_root: &Path,
        tenant: TenantId,
        scope: ScopeId,
        model_id: impl Into<String>,
    ) -> Result<Self> {
        let root = Self::root_for(data_root, &tenant, &scope);
        std::fs::create_dir_all(root.join("wal"))?;
        std::fs::create_dir_all(root.join("snapshots"))?;
        let meta_path = root.join(META_FILE);
        if meta_path.exists() {
            anyhow::bail!(
                "scope already exists: tenant={} scope={}",
                tenant.as_str(),
                scope.as_str()
            );
        }
        let meta = ScopeMeta {
            version: SCOPE_LAYOUT_VERSION,
            model_id: model_id.into(),
            key_ref: "file:key.bin".to_string(),
            epoch: 0,
            owner_host: hostname(),
            created_at: now_secs(),
        };
        // create_new: fail if a concurrent creator won the race.
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&meta_path)?;
        let text = serde_json::to_string_pretty(&meta)?;
        use std::io::Write;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        let layout = Self { root, tenant, scope };
        layout.ensure_key()?;
        Ok(layout)
    }

    /// Open an existing scope. Refuses version mismatch loudly (fail-closed).
    pub fn open(data_root: &Path, tenant: TenantId, scope: ScopeId) -> Result<Self> {
        let root = Self::root_for(data_root, &tenant, &scope);
        let text = std::fs::read_to_string(root.join(META_FILE)).map_err(|e| {
            anyhow::anyhow!(
                "scope meta missing (tenant={} scope={}): {e}",
                tenant.as_str(),
                scope.as_str()
            )
        })?;
        let meta: ScopeMeta = serde_json::from_str(&text)?;
        if meta.version != SCOPE_LAYOUT_VERSION {
            anyhow::bail!(
                "scope layout version mismatch: scope has v{}, this build reads v{} (tenant={} scope={})",
                meta.version,
                SCOPE_LAYOUT_VERSION,
                tenant.as_str(),
                scope.as_str()
            );
        }
        let layout = Self { root, tenant, scope };
        layout.ensure_key()?;
        Ok(layout)
    }

    /// Ensure a data key exists for this scope and register it for this
    /// process (so WAL/snapshot calls seal transparently). Idempotent.
    pub fn ensure_key(&self) -> Result<()> {
        let key = Keystore::load_or_create(&self.root)?;
        register_scope_key(&self.root, key);
        Ok(())
    }

    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    pub fn scope(&self) -> &ScopeId {
        &self.scope
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Adopt an EXISTING directory as a scope root (single-user mode maps each
    /// context directory to a scope without migrating data). Creates `meta.json`
    /// on first adopt; version-checks it afterwards. Never touches other files.
    pub fn adopt(
        dir: &Path,
        tenant: TenantId,
        scope: ScopeId,
        model_id: impl Into<String>,
    ) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let layout = Self {
            root: dir.to_path_buf(),
            tenant,
            scope,
        };
        let meta_path = layout.root.join(META_FILE);
        if !meta_path.exists() {
            let meta = ScopeMeta {
                version: SCOPE_LAYOUT_VERSION,
                model_id: model_id.into(),
                // Key file reference. The key itself lives in key.bin next to
                // meta.json; KMS URIs replace this value later.
                key_ref: "file:key.bin".to_string(),
                epoch: 0,
                owner_host: hostname(),
                created_at: now_secs(),
            };
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&meta_path)?;
            let text = serde_json::to_string_pretty(&meta)?;
            use std::io::Write;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        // Version-check even pre-existing meta (fail-closed on foreign layouts).
        let meta = layout.meta()?;
        if meta.version != SCOPE_LAYOUT_VERSION {
            anyhow::bail!(
                "scope layout version mismatch: scope has v{}, this build reads v{}",
                meta.version,
                SCOPE_LAYOUT_VERSION
            );
        }
        layout.ensure_key()?;
        Ok(layout)
    }

    pub fn wal_dir(&self) -> PathBuf {
        self.root.join("wal")
    }

    pub fn snapshots_dir(&self) -> PathBuf {
        self.root.join("snapshots")
    }

    pub fn meta(&self) -> Result<ScopeMeta> {
        let text = std::fs::read_to_string(self.root.join(META_FILE))?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Bump the fencing epoch and persist it. Cross-host enforcement is Stage B;
    /// recording the monotonic counter now keeps the interface stable.
    pub fn bump_epoch(&self) -> Result<u64> {
        let mut meta = self.meta()?;
        meta.epoch = meta.epoch.saturating_add(1);
        let text = serde_json::to_string_pretty(&meta)?;
        std::fs::write(self.root.join(META_FILE), text)?;
        Ok(meta.epoch)
    }

    /// Stage-B hook: reject records from a stale epoch. Currently always `Ok`
    /// (single host needs no fencing); the signature is what Stage B implements.
    pub fn check_epoch(&self, _record_epoch: u64) -> Result<()> {
        Ok(())
    }

    /// Acquire the exclusive writer lock for this scope. A second holder —
    /// same process, another process, another host on shared disk — gets an
    /// explicit error (fail-closed, never silent). Hold the returned guard for
    /// as long as the engine runs; dropping it releases the lock.
    pub fn acquire_lock(&self) -> Result<ScopeLock> {
        let path = self.root.join(LOCK_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;
        use fs2::FileExt;
        file.try_lock_exclusive().map_err(|e| {
            let msg = format!(
                "scope already locked by another writer (tenant={} scope={} lock={}): {e}. \
                 Refusing to start a second engine on this scope.",
                self.tenant.as_str(),
                self.scope.as_str(),
                path.display()
            );
            tracing::error!("{}", msg);
            anyhow::anyhow!(msg)
        })?;
        Ok(ScopeLock {
            _file: file,
            tenant: self.tenant.clone(),
            scope: self.scope.clone(),
        })
    }
}

// ─── Lock guard ─────────────────────────────────────────────────────────────

/// Exclusive writer-lock guard. The OS lock lives on the open file handle;
/// dropping this guard releases it.
#[derive(Debug)]
pub struct ScopeLock {
    _file: File,
    tenant: TenantId,
    scope: ScopeId,
}

impl ScopeLock {
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    pub fn scope(&self) -> &ScopeId {
        &self.scope
    }
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
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

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cmhm_scope_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ids_are_opaque_and_unique() {
        let a = TenantId::generate();
        let b = TenantId::generate();
        assert_ne!(a.as_str(), b.as_str());
        for id in [a.as_str(), b.as_str(), ScopeId::generate().as_str()] {
            assert!(!id.contains('/') && !id.contains('\\') && !id.contains(".."),
                "scope IDs must never be usable as paths: {id}");
        }
    }

    #[test]
    fn create_then_open_round_trips() {
        let root = tmp_root("roundtrip");
        let layout = ScopeLayout::create(&root, TenantId::generate(), ScopeId::generate(), "hash-128d").unwrap();
        assert!(layout.wal_dir().is_dir());
        assert!(layout.snapshots_dir().is_dir());
        let meta = layout.meta().unwrap();
        assert_eq!(meta.version, SCOPE_LAYOUT_VERSION);
        assert_eq!(meta.model_id, "hash-128d");
        assert_eq!(meta.epoch, 0);
        let reopened = ScopeLayout::open(&root, layout.tenant().clone(), layout.scope().clone()).unwrap();
        assert_eq!(reopened.meta().unwrap().model_id, "hash-128d");
    }

    #[test]
    fn create_twice_is_refused() {
        let root = tmp_root("twice");
        let t = TenantId::generate();
        let s = ScopeId::generate();
        ScopeLayout::create(&root, t.clone(), s.clone(), "hash-128d").unwrap();
        let err = ScopeLayout::create(&root, t, s, "hash-128d").unwrap_err();
        assert!(err.to_string().contains("already exists"), "unexpected: {err}");
    }

    #[test]
    fn version_mismatch_is_fail_closed() {
        let root = tmp_root("version");
        let t = TenantId::generate();
        let s = ScopeId::generate();
        let layout = ScopeLayout::create(&root, t.clone(), s.clone(), "hash-128d").unwrap();
        let mut meta = layout.meta().unwrap();
        meta.version = 99;
        std::fs::write(layout.root().join(META_FILE), serde_json::to_string_pretty(&meta).unwrap()).unwrap();
        let err = ScopeLayout::open(&root, t, s).unwrap_err();
        assert!(err.to_string().contains("version mismatch"), "unexpected: {err}");
    }

    #[test]
    fn second_writer_is_refused_loudly() {
        let root = tmp_root("lock");
        let layout = ScopeLayout::create(&root, TenantId::generate(), ScopeId::generate(), "hash-128d").unwrap();
        let _first = layout.acquire_lock().expect("first lock must succeed");
        let err = layout.acquire_lock().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("already locked by another writer"), "unexpected: {msg}");
        assert!(msg.contains("Refusing to start a second engine"), "unexpected: {msg}");
    }

    #[test]
    fn epoch_bumps_monotonically() {
        let root = tmp_root("epoch");
        let layout = ScopeLayout::create(&root, TenantId::generate(), ScopeId::generate(), "hash-128d").unwrap();
        assert_eq!(layout.bump_epoch().unwrap(), 1);
        assert_eq!(layout.bump_epoch().unwrap(), 2);
        assert!(layout.check_epoch(0).is_ok());
    }
}
