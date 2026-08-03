// cmhm_core/src/context/runtime.rs
//! ContextRuntime: the top-level Context OS orchestrator.
//!
//! Holds the Registry (persisted metadata) and Cache (live engine handles).
//! All MCP tool calls go through ContextRuntime.
//!
//! Thread-safety: wrapped in Arc<Mutex<ContextRuntime>> by the MCP server.

use anyhow::Result;

use crate::{
    config::Config,
    engine::{self, EngineHandle, LearnResult},
    retrieval::RetrievalResult,
};
use super::{
    ContextId, ContextMeta, ContextType, ContextState, ContextDescriptor,
    registry::Registry,
    cache::{ContextCache, context_snapshot_path, ensure_context_dir},
    router::{RouterWeights, top_k},
};

/// The name of the automatic default context.
const DEFAULT_CTX_NAME: &str = "default";

pub struct ContextRuntime {
    registry: Registry,
    cache:    ContextCache,
    cfg:      Config,
}

/// Safety-net: save registry metadata on drop even without explicit shutdown().
/// This ensures version/learn_count survive unexpected drops (crashes, test teardowns).
/// A clean shutdown() is still preferred — it also checkpoints the engine state.
impl Drop for ContextRuntime {
    fn drop(&mut self) {
        let _ = self.registry.save();
    }
}

impl ContextRuntime {
    /// Initialise the Context Runtime.
    /// Loads the registry from disk; does NOT pre-load any engine.
    pub fn new(cfg: Config) -> Result<Self> {
        std::fs::create_dir_all(&cfg.data_dir)?;
        std::fs::create_dir_all(cfg.data_dir.join("contexts"))?;
        std::fs::create_dir_all(&cfg.projections_dir)?;

        let registry = Registry::load(&cfg.data_dir)?;
        let max_active = cfg.max_active_contexts;
        let cache = ContextCache::new(max_active);

        let mut rt = Self { registry, cache, cfg };

        // Ensure the "default" context always exists for backward-compat
        if rt.registry.find_by_name(DEFAULT_CTX_NAME).is_none() {
            let meta = ContextMeta::new(
                DEFAULT_CTX_NAME,
                ContextType::Project,
                None,
                "Auto-created default context for backward compatibility",
                vec!["default".to_string()],
            );
            rt.registry.create(meta)?;
        }

        Ok(rt)
    }

    // ── Context Lifecycle API ────────────────────────────────────────────────

    /// Create a new context. Returns its descriptor.
    pub fn create_context(
        &mut self,
        display_name: impl Into<String>,
        context_type: ContextType,
        parent_id: Option<ContextId>,
        description: impl Into<String>,
        tags: Vec<String>,
    ) -> Result<ContextDescriptor> {
        let meta = ContextMeta::new(display_name, context_type, parent_id, description, tags);
        let desc = self.registry.create(meta)?;
        tracing::info!("Context created: {} ({})", desc.display_name, desc.id);
        Ok(desc)
    }

    /// List all non-deleted contexts.
    pub fn list_contexts(&self) -> Vec<ContextDescriptor> {
        self.registry.list()
    }

    /// Delete a context: shut down its engine, remove its data, mark deleted.
    pub fn delete_context(&mut self, id: &ContextId) -> Result<()> {
        // Check it exists and is not already deleted
        let meta = self.registry.get(id)
            .ok_or_else(|| anyhow::anyhow!("E_NOT_FOUND: {id}"))?;
        if meta.state == ContextState::Deleted {
            anyhow::bail!("E_CONTEXT_DELETED: already deleted");
        }
        // Guard: cannot delete the default context
        if meta.display_name == DEFAULT_CTX_NAME {
            anyhow::bail!("E_PROTECTED: cannot delete the default context");
        }

        // Evict from cache (shuts down engine, takes snapshot)
        self.cache.remove(id);

        // Remove data directory
        let ctx_dir = self.cfg.data_dir.join("contexts").join(id.as_str());
        if ctx_dir.exists() {
            std::fs::remove_dir_all(&ctx_dir)?;
        }

        // Mark deleted via FSM gate (validates current_state -> Deleted is legal)
        self.registry.transition(id, ContextState::Deleted)?;
        tracing::info!("Context deleted: {}", id);
        Ok(())
    }

    /// Get a descriptor for a single context.
    pub fn get_descriptor(&self, id: &ContextId) -> Option<ContextDescriptor> {
        self.registry.get(id).map(ContextDescriptor::from)
    }

    // ── Routing API ──────────────────────────────────────────────────────────

    /// Auto-select the best context for a query hint.
    /// Returns the best-matching descriptor and its score.
    pub fn route(&self, query_hint: &str) -> Option<(ContextDescriptor, f32)> {
        let all: Vec<&ContextMeta> = self.registry.list_all()
            .iter()
            .filter_map(|d| self.registry.get(&d.id))
            .filter(|m| m.state != ContextState::Deleted)
            .collect();

        let weights = RouterWeights::from_config(&self.cfg);
        let scored = top_k(&all, query_hint, &weights, 1);
        scored.into_iter().next().map(|(m, s)| (ContextDescriptor::from(m), s))
    }

    /// Top-K context suggestions for a query hint.
    pub fn route_top_k(&self, query_hint: &str, k: usize) -> Vec<(ContextDescriptor, f32)> {
        let all: Vec<&ContextMeta> = self.registry.list_all()
            .iter()
            .filter_map(|d| self.registry.get(&d.id))
            .filter(|m| m.state != ContextState::Deleted)
            .collect();

        let weights = RouterWeights::from_config(&self.cfg);
        top_k(&all, query_hint, &weights, k)
            .into_iter()
            .map(|(m, s)| (ContextDescriptor::from(m), s))
            .collect()
    }

    // ── Memory Operations API ────────────────────────────────────────────────

    /// Learn into a context. If context_id is None, uses the default context.
    pub fn learn(
        &mut self,
        context_id: Option<&str>,
        state_emb:   Vec<f32>,
        action_emb:  Vec<f32>,
        outcome_emb: Vec<f32>,
        prediction_error: f32,
        emotional_tag:    f32,
        model_id: &str,
    ) -> Result<(LearnResult, ContextDescriptor)> {
        let id = self.resolve_context(context_id)?;
        let handle = self.ensure_loaded(&id)?;
        let result = handle.learn(state_emb, action_emb, outcome_emb,
                                   prediction_error, emotional_tag, model_id)?;
        self.registry.record_learn(&id);
        let desc = self.registry.get(&id)
            .map(ContextDescriptor::from)
            .ok_or_else(|| anyhow::anyhow!("context vanished after learn"))?;
        Ok((result, desc))
    }

    /// Retrieve from a context. If context_id is None, uses the default context.
    pub fn retrieve(
        &mut self,
        context_id: Option<&str>,
        query_emb: Vec<f32>,
        model_id:  &str,
    ) -> Result<(RetrievalResult, ContextDescriptor)> {
        let id = self.resolve_context(context_id)?;
        let handle = self.ensure_loaded(&id)?;
        let result = handle.retrieve(query_emb, model_id)?;
        // ensure_loaded already called transition -> Active
        let desc = self.registry.get(&id)
            .map(ContextDescriptor::from)
            .ok_or_else(|| anyhow::anyhow!("context vanished after retrieve"))?;
        Ok((result, desc))
    }

    /// Force a snapshot of a context.
    pub fn checkpoint(&mut self, context_id: Option<&str>) -> Result<ContextDescriptor> {
        let id = self.resolve_context(context_id)?;
        if self.cache.is_loaded(&id) {
            let snap = context_snapshot_path(&self.cfg.data_dir, &id);
            if let Some(handle) = self.cache.get(&id) {
                handle.snapshot(snap)?;
            }
        }
        self.registry.get(&id)
            .map(ContextDescriptor::from)
            .ok_or_else(|| anyhow::anyhow!("E_NOT_FOUND: {id}"))
    }

    /// Stats for a context.
    pub fn stats(&mut self, context_id: Option<&str>) -> Result<ContextStats> {
        let id = self.resolve_context(context_id)?;
        let meta = self.registry.get(&id)
            .ok_or_else(|| anyhow::anyhow!("E_NOT_FOUND: {id}"))?;
        Ok(ContextStats {
            descriptor:          ContextDescriptor::from(meta),
            is_loaded:           self.cache.is_loaded(&id),
            active_count:        self.cache.active_count(),
            max_active_contexts: self.cfg.max_active_contexts,
        })
    }

    /// Merge keyword tags into a context's routing metadata.
    ///
    /// Automatically called after `memory_learn_text` to improve future
    /// `context_route` accuracy. Tags are lowercased, deduplicated, and capped
    /// at 64 per context. Persists to the registry on disk.
    ///
    /// Pass `context_id = None` to target the default context.
    pub fn add_tags(&mut self, context_id: Option<&str>, new_tags: Vec<String>) -> Result<()> {
        let id = self.resolve_context(context_id)?;
        self.registry.add_tags(&id, new_tags, 64)
    }

    /// Provide outcome feedback for a list of node IDs after a retrieve → act → observe cycle.
    ///
    /// Updates each node's Beta(α, β) bandit posterior and half-life.
    /// Returns (updated_count, ContextDescriptor).
    pub fn feedback_nodes(
        &mut self,
        context_id: Option<&str>,
        node_ids: Vec<u64>,
        success: bool,
    ) -> Result<(usize, ContextDescriptor)> {
        let id = self.resolve_context(context_id)?;
        let handle = self.ensure_loaded(&id)?;
        let updated = handle.feedback_nodes(node_ids, success)?;
        let desc = self.registry.get(&id)
            .map(ContextDescriptor::from)
            .ok_or_else(|| anyhow::anyhow!("context vanished after feedback"))?;
        Ok((updated, desc))
    }

    /// Graceful shutdown: checkpoint all loaded contexts.
    pub fn shutdown(&mut self) {
        // Transition all loaded Active/Idle contexts to Sleeping so registry
        // reflects correct state on next startup.
        let loaded_ids: Vec<ContextId> = self.cache.loaded_ids();
        for id in &loaded_ids {
            let state = self.registry.get(id).map(|m| m.state);
            if state == Some(ContextState::Active) || state == Some(ContextState::Idle) {
                let _ = self.registry.transition(id, ContextState::Sleeping);
            }
        }
        self.cache.checkpoint_all(&self.cfg.data_dir.clone());
        self.cache.shutdown_all();
        let _ = self.registry.save();
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    /// Resolve an optional context_id string to a ContextId.
    /// None → "default" context.
    /// Also accepts display names for convenience.
    fn resolve_context(&self, context_id: Option<&str>) -> Result<ContextId> {
        let name = context_id.unwrap_or(DEFAULT_CTX_NAME);

        // First try exact UUID match
        let probe = ContextId::from_str(name);
        if let Some(m) = self.registry.get(&probe) {
            if m.state != ContextState::Deleted {
                return Ok(probe);
            } else {
                anyhow::bail!("E_CONTEXT_DELETED: {name}");
            }
        }

        // Fallback: match by display name
        if let Some(m) = self.registry.find_by_name(name) {
            return Ok(m.id.clone());
        }

        anyhow::bail!("E_NOT_FOUND: context '{}' does not exist. Use context_create first.", name)
    }

    /// Ensure a context's engine is loaded in the cache.
    /// If not loaded, starts a new engine from the existing snapshot (if any).
    ///
    /// Crash recovery path:
    ///   - If the context is Sleeping and a WAL file exists alongside the snapshot,
    ///     we transition it to Recovering before starting the engine.
    ///     The engine replays the WAL on startup. On success we transition to Active.
    ///   - If the context is Sleeping with no WAL (clean shutdown), we go directly
    ///     Creating -> Active... wait, state is Sleeping -> Active (clean load).
    fn ensure_loaded(&mut self, id: &ContextId) -> Result<&EngineHandle> {
        use crate::context::cache::context_wal_path;

        if !self.cache.is_loaded(id) {
            tracing::info!("Loading context {} into RAM", id);
            ensure_context_dir(&self.cfg.data_dir, id)?;

            // --- State classification for loading ---
            // A context might be Sleeping (clean eviction or clean shutdown),
            // Active (saved by old code / partial shutdown), or Creating (new).
            // If WAL exists, we need recovery regardless of the recorded state.
            let current_state = self.registry.get(id)
                .map(|m| m.state)
                .unwrap_or(ContextState::Creating);

            let wal_path = context_wal_path(&self.cfg.data_dir, id);
            let needs_recovery = wal_path.exists() && matches!(
                current_state,
                ContextState::Sleeping | ContextState::Active | ContextState::Idle
            );

            if needs_recovery {
                // Ensure we're in Sleeping before transitioning to Recovering
                if current_state == ContextState::Active || current_state == ContextState::Idle {
                    let _ = self.registry.transition(id, ContextState::Sleeping);
                }
                tracing::warn!("Context {} has unclean WAL -- entering Recovering state", id);
                if let Err(e) = self.registry.transition(id, ContextState::Recovering) {
                    tracing::error!("Could not enter Recovering state for {}: {e}", id);
                }
            } else if current_state == ContextState::Active || current_state == ContextState::Idle {
                // Context was saved as Active/Idle (old shutdown or partial flush).
                // Treat as Sleeping for the load path.
                let _ = self.registry.transition(id, ContextState::Sleeping);
            }

            let ctx_dir = self.cfg.data_dir.join("contexts").join(id.as_str());
            let mut ctx_cfg = self.cfg.clone();
            ctx_cfg.data_dir = ctx_dir;

            let handle = match engine::start(ctx_cfg) {
                Ok(h) => h,
                Err(e) => {
                    if needs_recovery {
                        tracing::error!("WAL replay failed for context {}: {e}", id);
                        let _ = self.registry.transition(id, ContextState::Deleted);
                    }
                    return Err(e);
                }
            };

            let evicted = self.cache.insert(id.clone(), handle);
            if let Some(evicted_id) = evicted {
                let evicted_state = self.registry.get(&evicted_id)
                    .map(|m| m.state)
                    .unwrap_or(ContextState::Active);
                if evicted_state == ContextState::Active || evicted_state == ContextState::Idle {
                    let _ = self.registry.transition(&evicted_id, ContextState::Sleeping);
                }
            }

            // Transition to Active (from Creating, Sleeping, or Recovering)
            self.registry.transition(id, ContextState::Active)?;

        } else {
            // Already in RAM: touch LRU.
            // If Idle, transition to Active (request arrived).
            // If already Active, just bump access stats directly — no state change needed.
            self.cache.touch(id);
            let current = self.registry.get(id).map(|m| m.state);
            if current == Some(ContextState::Idle) {
                // Idle -> Active (request arrived)
                let _ = self.registry.transition(id, ContextState::Active);
            } else if current == Some(ContextState::Active) {
                // Already Active: bump stats without touching state
                self.registry.record_access(id);
            }
        }

        self.cache.get(id)
            .ok_or_else(|| anyhow::anyhow!("internal: handle missing after ensure_loaded"))
    }

    /// Get the default context ID.
    pub fn default_context_id(&self) -> Option<ContextId> {
        self.registry.find_by_name(DEFAULT_CTX_NAME).map(|m| m.id.clone())
    }
}

// ── Stats ────────────────────────────────────────────────────────────────────

pub struct ContextStats {
    pub descriptor:          ContextDescriptor,
    pub is_loaded:           bool,
    /// How many contexts are currently loaded in RAM.
    pub active_count:        usize,
    /// The configured cap on simultaneous in-RAM contexts.
    pub max_active_contexts: usize,
}
