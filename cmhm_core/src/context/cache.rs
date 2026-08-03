// cmhm_core/src/context/cache.rs
//! LRU cache for active EngineHandles.
//!
//! Keeps the N most recently used contexts loaded in RAM.
//! When capacity is exceeded, the least-recently-used context is evicted:
//! its engine thread is shut down (takes a snapshot first).
//!
//! Uses a VecDeque as an ordered LRU list (O(n) eviction is acceptable
//! because max_active_contexts is small, e.g. <= 50).

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use anyhow::Result;
use crate::context::ContextId;
use crate::engine::EngineHandle;

pub struct ContextCache {
    /// Live engine handles, keyed by context ID.
    handles:    HashMap<ContextId, EngineHandle>,
    /// LRU order: front = most recently used, back = LRU.
    lru_order:  VecDeque<ContextId>,
    /// Maximum number of contexts to keep in RAM simultaneously.
    capacity:   usize,
}

impl ContextCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            handles:   HashMap::new(),
            lru_order: VecDeque::new(),
            capacity:  capacity.max(1),
        }
    }

    /// Get a handle if loaded, without changing LRU order.
    pub fn get(&self, id: &ContextId) -> Option<&EngineHandle> {
        self.handles.get(id)
    }

    /// Touch a context (move to front of LRU).
    pub fn touch(&mut self, id: &ContextId) {
        self.lru_order.retain(|x| x != id);
        self.lru_order.push_front(id.clone());
    }

    /// Insert a new engine handle. Evicts LRU if over capacity.
    /// Returns the ID of the evicted context, if any (so caller can update registry).
    pub fn insert(&mut self, id: ContextId, handle: EngineHandle) -> Option<ContextId> {
        // If already present (re-insert after reload), just update
        if self.handles.contains_key(&id) {
            self.handles.insert(id.clone(), handle);
            self.touch(&id);
            return None;
        }

        let evicted = if self.handles.len() >= self.capacity {
            self.evict_lru()
        } else {
            None
        };

        self.handles.insert(id.clone(), handle);
        self.lru_order.push_front(id);
        evicted
    }

    /// Evict the least-recently-used context.
    /// Shuts down its engine thread (which takes a final snapshot).
    fn evict_lru(&mut self) -> Option<ContextId> {
        let lru_id = self.lru_order.pop_back()?;
        if let Some(handle) = self.handles.remove(&lru_id) {
            tracing::info!("Evicting context {} from RAM (LRU)", lru_id);
            // Shutdown gracefully — engine takes a snapshot before exiting
            let _ = handle.shutdown();
        }
        Some(lru_id)
    }

    /// Remove a specific context from the cache and shut down its engine.
    pub fn remove(&mut self, id: &ContextId) -> bool {
        self.lru_order.retain(|x| x != id);
        if let Some(handle) = self.handles.remove(id) {
            let _ = handle.shutdown();
            true
        } else {
            false
        }
    }

    /// Checkpoint all active contexts (flush to disk) without evicting.
    /// Used on server shutdown.
    pub fn checkpoint_all(&mut self, data_dir: &PathBuf) {
        for (id, handle) in &self.handles {
            let snap_path = context_snapshot_path(data_dir, id);
            tracing::info!("Checkpointing context {}", id);
            let _ = handle.snapshot(snap_path);
        }
    }

    /// Shutdown all engines (called on server exit).
    /// Consumes the cache.
    pub fn shutdown_all(&mut self) {
        let ids: Vec<ContextId> = self.handles.keys().cloned().collect();
        for id in ids {
            if let Some(h) = self.handles.remove(&id) {
                tracing::info!("Shutting down context engine {}", id);
                let _ = h.shutdown();
            }
        }
    }

    pub fn is_loaded(&self, id: &ContextId) -> bool {
        self.handles.contains_key(id)
    }

    /// All context IDs currently loaded in the cache.
    pub fn loaded_ids(&self) -> Vec<ContextId> {
        self.handles.keys().cloned().collect()
    }

    pub fn active_count(&self) -> usize {
        self.handles.len()
    }
}

/// Derives the snapshot file path for a given context ID.
pub fn context_snapshot_path(data_dir: &PathBuf, id: &ContextId) -> PathBuf {
    data_dir.join("contexts").join(id.as_str()).join("snapshot.bin")
}

/// Derives the WAL file path for a given context ID.
pub fn context_wal_path(data_dir: &PathBuf, id: &ContextId) -> PathBuf {
    data_dir.join("contexts").join(id.as_str()).join("wal.dat")
}

/// Ensures the per-context data directory exists.
pub fn ensure_context_dir(data_dir: &PathBuf, id: &ContextId) -> Result<()> {
    let dir = data_dir.join("contexts").join(id.as_str());
    std::fs::create_dir_all(&dir)?;
    Ok(())
}
