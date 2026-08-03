//! Context Runtime types.
//!
//! A **Context** is an isolated unit of causal memory identified by a UUID v4.
//! Every `memory_learn` / `memory_retrieve` call targets exactly one context.
//! Contexts are never mixed -- zero data leakage between context boundaries.

pub mod registry;
pub mod cache;
pub mod router;
pub mod runtime;
pub mod fsm;
pub mod router_tests;

use serde::{Deserialize, Serialize};
use std::fmt;

// --- UUID generation (no external uuid crate -- uses rand already in workspace) ---

fn new_uuid_v4() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    // Set version 4 bits
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    // Set variant bits (RFC 4122)
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],  bytes[1],  bytes[2],  bytes[3],
        bytes[4],  bytes[5],
        bytes[6],  bytes[7],
        bytes[8],  bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

// --- ContextId ----------------------------------------------------------------

/// Opaque, unique identifier for a context.
/// Internally a UUID v4, serialised as a hyphenated lowercase string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContextId(String);

impl ContextId {
    /// Generate a new random context ID (UUID v4).
    pub fn new() -> Self {
        Self(new_uuid_v4())
    }
    /// Create from an existing string (loaded from registry or passed by caller).
    pub fn from_str(s: &str) -> Self {
        Self(s.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ContextId {
    fn default() -> Self { Self::new() }
}

impl fmt::Display for ContextId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// --- ContextType -------------------------------------------------------------

/// The semantic scope of a context. Determines default lifetime policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextType {
    /// A single conversation or chat session (short-lived).
    Session,
    /// A development or research project (medium-lived).
    Project,
    /// An experiment branch derived from a project.
    Branch,
    /// An organisation-level workspace (long-lived).
    Workspace,
    /// Global, cross-organisation knowledge (permanent, read-mostly).
    Global,
}

impl Default for ContextType {
    fn default() -> Self { Self::Project }
}

impl fmt::Display for ContextType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Session   => "session",
            Self::Project   => "project",
            Self::Branch    => "branch",
            Self::Workspace => "workspace",
            Self::Global    => "global",
        };
        write!(f, "{s}")
    }
}

// --- ContextState (Lifecycle FSM) -------------------------------------------

/// Lifecycle state per the Context OS Spec v1.0 FSM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextState {
    /// Allocation in progress; not yet accessible.
    Creating,
    /// Loaded in RAM; fully operational.
    Active,
    /// In RAM, no recent requests; still fast to access.
    Idle,
    /// Checkpointed; evicted from RAM. Loads on next request.
    Sleeping,
    /// WAL replay in progress after crash.
    Recovering,
    /// Read-only; moved to cold storage.
    Archived,
    /// Permanently erased. Terminal state -- no transitions out.
    Deleted,
}

impl Default for ContextState {
    fn default() -> Self { Self::Creating }
}

impl fmt::Display for ContextState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Creating   => "creating",
            Self::Active     => "active",
            Self::Idle       => "idle",
            Self::Sleeping   => "sleeping",
            Self::Recovering => "recovering",
            Self::Archived   => "archived",
            Self::Deleted    => "deleted",
        };
        write!(f, "{s}")
    }
}


impl ContextState {
    /// All states reachable from this state via a single legal transition.
    /// Delegates to `fsm::successors` -- the transition table lives in one place.
    pub fn allowed_transitions(self) -> Vec<ContextState> {
        fsm::successors(self)
    }

    /// True if this state can serve user memory requests (Active or Idle).
    /// Used by the router's Sa scoring component.
    pub fn is_serviceable(self) -> bool {
        fsm::is_serviceable(self)
    }
}
// --- ContextMeta (Registry Entry) -------------------------------------------

/// Full registry entry for a context.
/// Persisted to `{data_dir}/registry.json`.
/// Contains metadata and routing signals -- NEVER graph data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMeta {
    pub id:            ContextId,
    pub display_name:  String,
    pub context_type:  ContextType,
    pub state:         ContextState,
    pub parent_id:     Option<ContextId>,
    pub description:   String,
    pub tags:          Vec<String>,
    /// Unix timestamp (seconds) of creation.
    pub created_at:    i64,
    /// Unix timestamp of last access (for recency scoring).
    pub last_accessed: i64,
    /// How many times this context has been accessed (for frequency scoring).
    pub access_count:  u64,
    /// Monotonically increasing version, bumped on every committed learn.
    pub version:       u64,
    /// Number of learn operations stored in this context.
    pub learn_count:   u64,
}

impl ContextMeta {
    pub fn new(
        display_name: impl Into<String>,
        context_type: ContextType,
        parent_id: Option<ContextId>,
        description: impl Into<String>,
        tags: Vec<String>,
    ) -> Self {
        let now = now_secs();
        Self {
            id:            ContextId::new(),
            display_name:  display_name.into(),
            context_type,
            state:         ContextState::Creating,
            parent_id,
            description:   description.into(),
            tags,
            created_at:    now,
            last_accessed: now,
            access_count:  0,
            version:       0,
            learn_count:   0,
        }
    }
}

// --- ContextDescriptor (Safe-to-pass view) -----------------------------------

/// Immutable, lightweight, safe-to-share view of a context.
/// This is what external callers (MCP tools) receive.
/// Contains NO operational data (no graph pointer, no WAL position).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextDescriptor {
    pub id:            ContextId,
    pub display_name:  String,
    pub context_type:  ContextType,
    pub state:         ContextState,
    pub parent_id:     Option<ContextId>,
    pub description:   String,
    pub tags:          Vec<String>,
    pub created_at:    i64,
    pub last_accessed: i64,
    pub access_count:  u64,
    pub version:       u64,
    pub learn_count:   u64,
}

impl From<&ContextMeta> for ContextDescriptor {
    fn from(m: &ContextMeta) -> Self {
        Self {
            id:            m.id.clone(),
            display_name:  m.display_name.clone(),
            context_type:  m.context_type,
            state:         m.state,
            parent_id:     m.parent_id.clone(),
            description:   m.description.clone(),
            tags:          m.tags.clone(),
            created_at:    m.created_at,
            last_accessed: m.last_accessed,
            access_count:  m.access_count,
            version:       m.version,
            learn_count:   m.learn_count,
        }
    }
}

// --- Utilities ---------------------------------------------------------------

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

