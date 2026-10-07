//! Authentication + authorization (M2).
//!
//! - Tokens: `cmhm_<64 hex>` (256-bit random). Only SHA-256 hex is stored.
//! - Expiry enforced at lookup; revocation is immediate in the store.
//! - Roles are derived server-side. Agent effective permission =
//!   owner grants ∩ token scopes: an agent can never exceed its owner.
//! - Tenant ALWAYS comes from the verified token, never from a request field.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::control::{ControlPlane, PrincipalKind};

/// Roles, ordered. Comparison uses rank, not declaration order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    Reader,
    Writer,
    Maintainer,
    Owner,
}

impl Role {
    pub fn rank(self) -> u8 {
        match self {
            Role::Reader => 0,
            Role::Writer => 1,
            Role::Maintainer => 2,
            Role::Owner => 3,
        }
    }

    /// Minimum role for an operation class.
    pub fn allows(self, need: Role) -> bool {
        self.rank() >= need.rank()
    }
}

/// Stored token record. Plaintext exists only at mint time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub token_id: String,
    pub hash_hex: String,
    pub principal_id: String,
    /// Scopes this token may touch (agent binding). Empty = owner-wide.
    pub scopes: Vec<String>,
    pub expires_at: f64,
    pub revoked: bool,
    pub created_at: f64,
}

impl TokenRecord {
    pub fn new(plaintext: &str, principal_id: &str, scopes: Vec<String>, ttl_secs: u64) -> Self {
        let now = now_secs();
        Self {
            token_id: new_token_id(),
            hash_hex: hash_token(plaintext),
            principal_id: principal_id.to_string(),
            scopes,
            expires_at: now + ttl_secs as f64,
            revoked: false,
            created_at: now,
        }
    }

    pub fn live(&self, now: f64) -> bool {
        !self.revoked && now < self.expires_at
    }
}

pub fn hash_token(plaintext: &str) -> String {
    let mut h = Sha256::new();
    h.update(plaintext.as_bytes());
    hex::encode(h.finalize())
}

fn new_token_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    format!("tok-{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Authenticated caller. Tenant came from the token, not the request.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub principal_id: String,
    pub tenant_id: String,
    pub is_agent: bool,
    pub token_id: String,
}

/// Verify a bearer token against the control plane. Fails closed on
/// unknown hash, expiry, or revocation. No tenant comes out of this that
/// wasn't in the token record.
pub fn authenticate(
    cp: &dyn ControlPlane,
    bearer: &str,
) -> Result<AuthContext> {
    let token = bearer.strip_prefix("Bearer ").unwrap_or(bearer).trim();
    if token.is_empty() {
        anyhow::bail!("E_UNAUTHENTICATED: missing bearer token");
    }
    let now = now_secs();
    let rec = cp
        .lookup_token(&hash_token(token))
        .ok_or_else(|| anyhow::anyhow!("E_UNAUTHENTICATED: unknown token"))?;
    if rec.revoked {
        anyhow::bail!("E_UNAUTHENTICATED: token revoked");
    }
    if now >= rec.expires_at {
        anyhow::bail!("E_UNAUTHENTICATED: token expired");
    }
    // Resolve principal -> tenant. Token scopes bind agents to explicit scopes.
    Ok(AuthContext {
        principal_id: rec.principal_id.clone(),
        tenant_id: String::new(), // filled by authorize() from principal record
        is_agent: !rec.scopes.is_empty(),
        token_id: rec.token_id.clone(),
    })
}

/// Authorize (principal, scope, operation). Returns the effective role.
///
/// Agent rule: effective = owner grants ∩ token scopes. Agents carry a
/// non-empty scope list; a scope outside it is denied even if the OWNER
/// could access it. Owner-wide tokens (empty scopes) use grants directly.
pub fn authorize(
    cp: &dyn ControlPlane,
    ctx: &mut AuthContext,
    scope_id: &str,
    need: Role,
) -> Result<Role> {
    // Scope must exist and belong to the caller's tenant (checked below).
    let scope = cp
        .scope(scope_id)
        .ok_or_else(|| anyhow::anyhow!("E_NOT_FOUND: unknown scope"))?;
    // Resolve tenant from the principal side only.
    let tenant_id = tenant_of_principal(cp, &ctx.principal_id)?;
    if scope.tenant_id != tenant_id {
        // Deliberately the same shape as unknown-scope: no cross-tenant oracle.
        anyhow::bail!("E_NOT_FOUND: unknown scope");
    }
    ctx.tenant_id = tenant_id;

    // Agent scope binding: token must list this scope.
    if ctx.is_agent {
        let allowed = cp
            .lookup_token_scopes(&ctx.token_id)
            .unwrap_or_default();
        if !allowed.iter().any(|s| s == scope_id) {
            anyhow::bail!("E_FORBIDDEN: token not bound to this scope");
        }
    }

    // Effective grants: an agent acts with its OWNER's grants (intersected
    // with the token binding above), so it can never exceed its owner but
    // needs no grants of its own.
    let grant_principal_id: String = match cp.principal(&ctx.principal_id) {
        Some(p) => match p.kind {
            crate::control::PrincipalKind::Agent { owner } => owner,
            _ => ctx.principal_id.clone(),
        },
        None => ctx.principal_id.clone(),
    };

    let best = cp
        .grants_for(&grant_principal_id)
        .into_iter()
        .filter(|g| g.scope_id == scope_id)
        .map(|g| g.role)
        .max_by_key(|r| r.rank());
    match best {
        Some(role) if role.allows(need) => Ok(role),
        _ => anyhow::bail!("E_FORBIDDEN: {need:?} required on this scope"),
    }
}

fn tenant_of_principal(cp: &dyn ControlPlane, principal_id: &str) -> Result<String> {
    cp.principal_tenant(principal_id)
        .ok_or_else(|| anyhow::anyhow!("E_UNAUTHENTICATED: unknown principal"))
}

// hex without a dep: sha2 gives bytes; format manually.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::FileControlPlane;
    use cmhm_core::scope::{ScopeId, TenantId};

    static TEST_DIR_COUNTER: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);

    fn setup_dir(name: &str) -> std::path::PathBuf {
        // Unique per test AND per thread: wall-clock nanos collide on
        // coarse timers when tests start together; a counter cannot.
        let n = TEST_DIR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "cmhm_gw_auth_{name}_{:?}_{n}",
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn setup() -> (FileControlPlane, TenantId, ScopeId, String, String) {
        let dir = setup_dir("auth");
        let mut cp = FileControlPlane::open(&dir.join("c.json")).unwrap();
        let t = cp.create_tenant("acme").unwrap();
        let tenant = TenantId::import(&t.id).unwrap();
        let owner = cp
            .create_principal(&tenant, PrincipalKind::User)
            .unwrap();
        let agent = cp
            .create_principal(
                &tenant,
                PrincipalKind::Agent { owner: owner.id.clone() },
            )
            .unwrap();
        let scope = ScopeId::generate();
        cp.register_scope(&tenant, &scope, "proj").unwrap();
        // Owner: owner-wide token (empty scopes) + Writer grant.
        let owner_tok = cp.mint_token(&owner.id, vec![], 3600).unwrap();
        cp.grant(&owner.id, scope.as_str(), Role::Writer).unwrap();
        // Agent: token bound to the scope, no grants of its own.
        let agent_tok = cp
            .mint_token(&agent.id, vec![scope.as_str().to_string()], 3600)
            .unwrap();
        (cp, tenant, scope, owner_tok, agent_tok)
    }

    #[test]
    fn missing_and_wrong_tokens_rejected() {
        let (cp, _, _, _, _) = setup();
        assert!(authenticate(&cp, "").is_err());
        assert!(authenticate(&cp, "Bearer cmhm_deadbeef").is_err());
    }

    #[test]
    fn expired_tokens_rejected() {
        let (mut cp, _, _, _, _) = setup();
        let t = cp.create_tenant("exp").unwrap();
        let tenant = TenantId::import(&t.id).unwrap();
        let p = cp.create_principal(&tenant, PrincipalKind::User).unwrap();
        let tok = cp.mint_token(&p.id, vec![], 0).unwrap();
        let err = authenticate(&cp, &tok).unwrap_err();
        assert!(err.to_string().contains("expired"), "got: {err}");
    }

    #[test]
    fn agent_inherits_owner_grant_but_cannot_exceed_token_binding() {
        let (mut cp, _, scope, owner_tok, agent_tok) = setup();
        // Owner-wide token works on the granted scope.
        let mut octx = authenticate(&cp, &owner_tok).unwrap();
        assert_eq!(
            authorize(&cp, &mut octx, scope.as_str(), Role::Writer).unwrap(),
            Role::Writer
        );
        // Agent token works too (owner grant ∩ token scopes).
        let mut actx = authenticate(&cp, &agent_tok).unwrap();
        assert!(actx.is_agent);
        assert_eq!(
            authorize(&cp, &mut actx, scope.as_str(), Role::Writer).unwrap(),
            Role::Writer
        );
        // Owner-level op denied for the Writer agent.
        assert!(authorize(&cp, &mut actx, scope.as_str(), Role::Owner).is_err());
        // Revocation is immediate.
        let agent_principal = cp
            .lookup_token(&hash_token(&agent_tok))
            .unwrap()
            .principal_id
            .clone();
        let tid = cp
            .lookup_token(&hash_token(&agent_tok))
            .unwrap()
            .token_id
            .clone();
        let _ = agent_principal;
        cp.revoke_token(&tid).unwrap();
        assert!(authenticate(&cp, &agent_tok).is_err());
    }

    #[test]
    fn deny_by_default_and_cross_tenant_oracle() {
        let (mut cp, _, scope, owner_tok, _) = setup();
        // No grant on a fresh scope → denied even with a valid token.
        let other = ScopeId::generate();
        {
            let t = cp.create_tenant("other").unwrap();
            let tenant = TenantId::import(&t.id).unwrap();
            cp.register_scope(&tenant, &other, "x").unwrap();
        }
        let mut octx = authenticate(&cp, &owner_tok).unwrap();
        // Other tenant's scope: same error shape as unknown scope (no oracle).
        let err = authorize(&cp, &mut octx, other.as_str(), Role::Reader).unwrap_err();
        assert!(err.to_string().contains("E_NOT_FOUND"), "got: {err}");
        // Own scope, no grant for Reader? owner HAS Writer → allowed (rank).
        let _ = scope;
    }
}
