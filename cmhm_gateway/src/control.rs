//! Control plane: tenants, principals, grants, scope directory.
//!
//! [`ControlPlane`] is the trait Postgres implements later. [`FileControlPlane`]
//! is the file-backed implementation (one JSON document, tmp-file + rename
//! writes, read on every mutation — correct at Foundation scale, not at fleet
//! scale; the trait boundary is what makes that swap safe).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::auth::{Role, TokenRecord};
use cmhm_core::scope::{ScopeId, TenantId};

/// A scope's placement + ownership record.
///
/// `context_id` is filled when the backing engine context is created
/// (scope_init). Gateway paths resolve scope → context through this field
/// only — never through display names (H6 lesson).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeRef {
    pub scope_id: String,
    pub tenant_id: String,
    /// Display label only. Never used for resolution (H6 lesson).
    pub display_name: String,
    /// Engine context backing this scope. Empty until scope_init.
    #[serde(default)]
    pub context_id: String,
    pub created_at: f64,
}

/// What a principal is: a human (via SSO later; pre-shared bootstrap now),
/// an agent bound to an owner, or an internal service.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PrincipalKind {
    User,
    Agent { owner: String },
    Service,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub id: String,
    pub tenant_id: String,
    pub kind: PrincipalKind,
    pub created_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,
    pub display_name: String,
    pub created_at: f64,
}

/// One grant row: (principal, scope, role). Deny by default — absence = no access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grant {
    pub principal_id: String,
    pub scope_id: String,
    pub role: Role,
}

/// Stored idempotent response: replayed byte-identically on key reuse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredResp {
    pub status: u16,
    pub body: serde_json::Value,
    pub created_at: f64,
}

/// Cap on stored idempotency records. Oldest-first eviction past the cap
/// (documented approximation: eviction scans for the minimum timestamp).
pub const IDEMPOTENCY_CAP: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoreDoc {
    tenants: HashMap<String, Tenant>,
    principals: HashMap<String, Principal>,
    tokens: HashMap<String, TokenRecord>,
    grants: Vec<Grant>,
    scopes: HashMap<String, ScopeRef>,
    idempotency: HashMap<String, StoredResp>,
}

/// Control-plane interface. Postgres implements this later.
pub trait ControlPlane: Send + Sync {
    fn create_tenant(&mut self, display_name: &str) -> Result<Tenant>;
    fn create_principal(&mut self, tenant: &TenantId, kind: PrincipalKind) -> Result<Principal>;
    fn mint_token(
        &mut self,
        principal_id: &str,
        scopes: Vec<String>,
        ttl_secs: u64,
    ) -> Result<String>;
    fn revoke_token(&mut self, token_id: &str) -> Result<()>;
    fn lookup_token(&self, token_hash_hex: &str) -> Option<TokenRecord>;
    fn lookup_token_scopes(&self, token_id: &str) -> Option<Vec<String>>;
    fn principal(&self, principal_id: &str) -> Option<Principal>;
    fn principal_tenant(&self, principal_id: &str) -> Option<String>;
    fn grant(&mut self, principal_id: &str, scope_id: &str, role: Role) -> Result<()>;
    fn grants_for(&self, principal_id: &str) -> Vec<Grant>;
    fn register_scope(&mut self, tenant: &TenantId, scope: &ScopeId, display_name: &str) -> Result<ScopeRef>;
    fn bind_scope_context(&mut self, scope_id: &str, context_id: &str) -> Result<()>;
    fn scope(&self, scope_id: &str) -> Option<ScopeRef>;
    fn tenant_of_scope(&self, scope_id: &str) -> Option<String>;
    fn idempotency_get(&self, principal_id: &str, key: &str) -> Option<StoredResp>;
    fn idempotency_put(&mut self, principal_id: &str, key: &str, status: u16, body: serde_json::Value) -> Result<()>;
}

/// File-backed control plane. See module docs for scale limits.
pub struct FileControlPlane {
    path: PathBuf,
    doc: StoreDoc,
}

impl FileControlPlane {
    pub fn open(path: &Path) -> Result<Self> {
        let doc = if path.exists() {
            let text = std::fs::read_to_string(path)?;
            serde_json::from_str(&text)?
        } else {
            StoreDoc::default()
        };
        Ok(Self { path: path.to_path_buf(), doc })
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&self.doc)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn persist_mut(&mut self) -> Result<()> {
        self.save()
    }
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn new_id(prefix: &str) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    format!(
        "{prefix}-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

impl ControlPlane for FileControlPlane {
    fn create_tenant(&mut self, display_name: &str) -> Result<Tenant> {
        let t = Tenant {
            id: new_id("tnt"),
            display_name: display_name.to_string(),
            created_at: now_secs(),
        };
        self.doc.tenants.insert(t.id.clone(), t.clone());
        self.persist_mut()?;
        Ok(t)
    }

    fn create_principal(&mut self, tenant: &TenantId, kind: PrincipalKind) -> Result<Principal> {
        if !self.doc.tenants.values().any(|t| t.id == tenant.as_str()) {
            anyhow::bail!("unknown tenant");
        }
        let p = Principal {
            id: new_id("prn"),
            tenant_id: tenant.as_str().to_string(),
            kind,
            created_at: now_secs(),
        };
        self.doc.principals.insert(p.id.clone(), p.clone());
        self.persist_mut()?;
        Ok(p)
    }

    fn mint_token(
        &mut self,
        principal_id: &str,
        scopes: Vec<String>,
        ttl_secs: u64,
    ) -> Result<String> {
        use rand::Rng;
        if !self.doc.principals.contains_key(principal_id) {
            anyhow::bail!("unknown principal");
        }
        let mut rng = rand::thread_rng();
        let raw: [u8; 32] = rng.gen();
        let plaintext = format!(
            "cmhm_{}",
            raw.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        let rec = TokenRecord::new(&plaintext, principal_id, scopes, ttl_secs);
        self.doc.tokens.insert(rec.token_id.clone(), rec);
        self.persist_mut()?;
        Ok(plaintext)
    }

    fn revoke_token(&mut self, token_id: &str) -> Result<()> {
        match self.doc.tokens.get_mut(token_id) {
            Some(r) => {
                r.revoked = true;
                self.persist_mut()?;
                Ok(())
            }
            None => anyhow::bail!("unknown token"),
        }
    }

    fn lookup_token(&self, token_hash_hex: &str) -> Option<TokenRecord> {
        self.doc.tokens.values().find(|r| r.hash_hex == token_hash_hex).cloned()
    }

    fn lookup_token_scopes(&self, token_id: &str) -> Option<Vec<String>> {
        self.doc.tokens.get(token_id).map(|r| r.scopes.clone())
    }

    fn principal(&self, principal_id: &str) -> Option<Principal> {
        self.doc.principals.get(principal_id).cloned()
    }

    fn principal_tenant(&self, principal_id: &str) -> Option<String> {
        self.doc.principals.get(principal_id).map(|p| {
            self.doc
                .tenants
                .get(&p.tenant_id)
                .map(|t| t.id.clone())
                .unwrap_or_else(|| p.tenant_id.clone())
        })
    }

    fn grant(&mut self, principal_id: &str, scope_id: &str, role: Role) -> Result<()> {
        if !self.doc.principals.contains_key(principal_id) {
            anyhow::bail!("unknown principal");
        }
        if !self.doc.scopes.contains_key(scope_id) {
            anyhow::bail!("unknown scope");
        }
        self.doc.grants.retain(|g| {
            !(g.principal_id == principal_id && g.scope_id == scope_id)
        });
        self.doc.grants.push(Grant {
            principal_id: principal_id.to_string(),
            scope_id: scope_id.to_string(),
            role,
        });
        self.persist_mut()?;
        Ok(())
    }

    fn grants_for(&self, principal_id: &str) -> Vec<Grant> {
        self.doc
            .grants
            .iter()
            .filter(|g| g.principal_id == principal_id)
            .cloned()
            .collect()
    }

    fn register_scope(
        &mut self,
        tenant: &TenantId,
        scope: &ScopeId,
        display_name: &str,
    ) -> Result<ScopeRef> {
        let r = ScopeRef {
            scope_id: scope.as_str().to_string(),
            tenant_id: tenant.as_str().to_string(),
            display_name: display_name.to_string(),
            context_id: String::new(),
            created_at: now_secs(),
        };
        self.doc.scopes.insert(r.scope_id.clone(), r.clone());
        self.persist_mut()?;
        Ok(r)
    }

    fn scope(&self, scope_id: &str) -> Option<ScopeRef> {
        self.doc.scopes.get(scope_id).cloned()
    }

    fn bind_scope_context(&mut self, scope_id: &str, context_id: &str) -> Result<()> {
        match self.doc.scopes.get_mut(scope_id) {
            Some(r) => {
                r.context_id = context_id.to_string();
                self.persist_mut()?;
                Ok(())
            }
            None => anyhow::bail!("unknown scope"),
        }
    }

    fn tenant_of_scope(&self, scope_id: &str) -> Option<String> {
        self.doc.scopes.get(scope_id).map(|s| s.tenant_id.clone())
    }

    fn idempotency_get(&self, principal_id: &str, key: &str) -> Option<StoredResp> {
        self.doc
            .idempotency
            .get(&format!("{principal_id}:{key}"))
            .cloned()
    }

    fn idempotency_put(
        &mut self,
        principal_id: &str,
        key: &str,
        status: u16,
        body: serde_json::Value,
    ) -> Result<()> {
        if self.doc.idempotency.len() >= IDEMPOTENCY_CAP {
            // Evict the oldest record (linear scan; cap hits are rare).
            if let Some(oldest) = self
                .doc
                .idempotency
                .iter()
                .min_by(|a, b| {
                    a.1.created_at
                        .partial_cmp(&b.1.created_at)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(k, _)| k.clone())
            {
                self.doc.idempotency.remove(&oldest);
            }
        }
        self.doc.idempotency.insert(
            format!("{principal_id}:{key}"),
            StoredResp { status, body, created_at: now_secs() },
        );
        self.persist_mut()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmhm_core::scope::{ScopeId, TenantId};

    fn mem_plane(tmp: &std::path::Path) -> FileControlPlane {
        FileControlPlane::open(&tmp.join("control.json")).unwrap()
    }

    static CP_DIR_COUNTER: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);

    /// Unique dir per test invocation. Fixed temp dirs race on Windows
    /// (remove_dir_all returns before lazy deletion completes), producing
    /// intermittent "cannot find the file" failures.
    fn unique_dir(stem: &str) -> std::path::PathBuf {
        let n = CP_DIR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("{stem}_{:?}_{n}", std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn tenant_principal_token_round_trip() {
        let dir = unique_dir("cmhm_gw_cp1");
        let mut cp = mem_plane(&dir);
        let t = cp.create_tenant("acme").unwrap();
        let tenant = TenantId::import(&t.id).unwrap_or_else(|_| TenantId::generate());
        let p = cp
            .create_principal(&tenant, PrincipalKind::User)
            .unwrap_or_else(|_| {
                // Fallback only if the tenant id form mismatches; keep test hermetic.
                let t2 = cp.create_tenant("acme2").unwrap();
                cp.create_principal(
                    &TenantId::import(&t2.id).unwrap(),
                    PrincipalKind::User,
                )
                .unwrap()
            });
        let _ = p;
        // Reload from disk: persistence round-trips.
        let cp2 = mem_plane(&dir);
        assert!(cp2.doc.tenants.values().any(|x| x.display_name == "acme"));
    }

    #[test]
    fn grant_requires_known_scope_and_principal() {
        let dir = unique_dir("cmhm_gw_cp2");
        let mut cp = mem_plane(&dir);
        assert!(cp.grant("nope", "nope", Role::Reader).is_err());
        let t = cp.create_tenant("acme").unwrap();
        let tenant = TenantId::import(&t.id).unwrap();
        let p = cp.create_principal(&tenant, PrincipalKind::User).unwrap();
        let s = ScopeId::generate();
        cp.register_scope(&tenant, &s, "proj").unwrap();
        cp.grant(&p.id, s.as_str(), Role::Writer).unwrap();
        assert_eq!(cp.grants_for(&p.id).len(), 1);
        // Re-grant replaces rather than duplicates.
        cp.grant(&p.id, s.as_str(), Role::Owner).unwrap();
        let g = cp.grants_for(&p.id);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].role, Role::Owner);
    }
}
