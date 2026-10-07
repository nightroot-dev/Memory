//! CMHM multi-tenant HTTP gateway (Expansion M2).
//!
//! Layers, outside in:
//! - HTTP routes (`routes`): versioned `/v1` JSON API, bearer auth, deny by default.
//! - Auth (`auth`): token mint/verify/revoke, RBAC grants, agent⊆owner rule.
//! - Control plane (`control`): tenants, principals, grants, scope directory.
//!   File-backed today behind the [`control::ControlPlane`] trait; Postgres
//!   later without touching routes or auth.
//!
//! Roles are derived server-side from the credential. Clients never declare
//! their own role, authority, or tenant (the C-01 lesson).

pub mod auth;
pub mod audit;
pub mod control;
pub mod routes;

pub use auth::{AuthContext, Role};
pub use control::{ControlPlane, FileControlPlane, ScopeRef};
// Tenant/scope identity is single-sourced from the engine crate.
pub use cmhm_core::scope::{ScopeId, TenantId};
