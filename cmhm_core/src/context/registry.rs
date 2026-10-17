//! Context Registry: persisted metadata store.
//!
//! Maps ContextId -> ContextMeta.
//! Stored as TOML in `{data_dir}/registry.toml`.
//! The registry NEVER stores graph data -- only metadata.
//!
//! ## State transitions
//! All state changes go through `Registry::transition()`, which delegates to
//! `fsm::validate()`. No caller can bypass the FSM gate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::Result;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use crate::context::{
    ContextId, ContextMeta, ContextState, ContextDescriptor, now_secs,
    fsm::{self, TransitionError},
};

const REGISTRY_FILE: &str = "registry.toml";

/// B11: take the exclusive sidecar lock for `registry.toml`, serialising
/// read-modify-write across concurrent server instances sharing one data
/// dir. Sidecar file (not the registry itself) so writers can keep using
/// tmp+rename atomics. Lock is released when the file handle drops.
fn lock_registry_sidecar(path: &Path) -> Result<std::fs::File> {
    let lock_path = path.with_extension("toml.lock");
    let f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| anyhow::anyhow!("E_REGISTRY_LOCK: open {:?}: {e}", lock_path))?;
    f.lock_exclusive()
        .map_err(|e| anyhow::anyhow!("E_REGISTRY_LOCK: lock {:?}: {e}", lock_path))?;
    Ok(f)
}

/// Best-effort variant for read-only loads: never blocks startup hard; on
/// contention we still read the file (atomic rename ⇒ no torn read).
fn try_lock_registry_sidecar(path: &Path) -> Option<std::fs::File> {
    match lock_registry_sidecar(path) {
        Ok(f) => Some(f),
        Err(e) => {
            tracing::warn!("Registry sidecar lock unavailable (continuing read-only): {e}");
            None
        }
    }
}

/// Wrapper for TOML serialisation (TOML requires a top-level table).
#[derive(Serialize, Deserialize, Default)]
struct RegistryFile {
    #[serde(default)]
    contexts: Vec<ContextMeta>,
}

pub struct Registry {
    entries:      HashMap<ContextId, ContextMeta>,
    path:         PathBuf,
    save_enabled: bool,
}

impl Registry {
    /// Load registry from disk, or create an empty one.
    pub fn load(data_dir: &Path) -> Result<Self> {
        let path = data_dir.join(REGISTRY_FILE);
        let entries: HashMap<ContextId, ContextMeta> = if path.exists() {
            // Best-effort serialisation against a concurrent writer; the
            // write itself is an atomic rename, so an uncontended read is
            // already torn-free.
            let _guard = try_lock_registry_sidecar(&path);
            match std::fs::read_to_string(&path) {
                Ok(s) => match toml::from_str::<RegistryFile>(&s) {
                    Ok(file) => file.contexts.into_iter().map(|m| (m.id.clone(), m)).collect(),
                    Err(e) => {
                        // Never silently discard: quarantine the corrupt file for
                        // forensics, then start fresh with a loud warning.
                        let quarantine = data_dir.join(format!(
                            "{}.corrupt.{}",
                            REGISTRY_FILE,
                            crate::core_state::now_secs() as u64
                        ));
                        if let Err(rename_err) = std::fs::rename(&path, &quarantine) {
                            tracing::error!(
                                "Registry {:?} is corrupt ({e}) and quarantine rename failed: {rename_err}",
                                path
                            );
                        } else {
                            tracing::warn!(
                                "Registry {:?} corrupt ({e}); quarantined to {:?}, starting fresh",
                                path, quarantine
                            );
                        }
                        HashMap::new()
                    }
                },
                Err(e) => {
                    tracing::warn!("Could not read registry {:?}: {e} -- starting fresh", path);
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };
        tracing::info!("Registry loaded: {} contexts from {:?}", entries.len(), path);
        Ok(Self { entries, path, save_enabled: true })
    }

    /// Persist registry to disk (atomic write via temp file + rename).
    /// No-op when save_enabled is false (test mode).
    ///
    /// B11 fix: exclusive sidecar lock around the whole write, and a
    /// reload-merge against the on-disk file first, so a long-lived
    /// instance never rewrites the registry from a stale load-time map and
    /// clobber entries created by a concurrent instance.
    pub fn save(&self) -> Result<()> {
        if !self.save_enabled { return Ok(()); }
        let _lock = lock_registry_sidecar(&self.path)?;
        let merged = self.merge_with_disk_locked();
        let file = RegistryFile {
            contexts: merged.values().cloned().collect(),
        };
        let toml_str = toml::to_string_pretty(&file)
            .map_err(|e| anyhow::anyhow!("Registry serialise error: {e}"))?;
        let tmp = self.path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml_str.as_bytes())?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// Merge in-memory entries with the on-disk registry (caller holds the
    /// sidecar lock).
    ///
    /// - Entries only on disk are adopted (created by another instance).
    /// - Entries on both sides: fresher copy wins — higher `version` first,
    ///   then newer `last_accessed`; on an exact tie a `Deleted` tombstone
    ///   beats a live state (deletion races resolve toward delete).
    /// - Unreadable disk file merges to the in-memory map (F3 quarantine
    ///   semantics unchanged); atomic rename previous state survives.
    fn merge_with_disk_locked(&self) -> HashMap<ContextId, ContextMeta> {
        let mut merged = self.entries.clone();
        let disk_str = match std::fs::read_to_string(&self.path) {
            Ok(s) => s,
            Err(_) => return merged, // no file yet or unreadable: our map is it
        };
        let Ok(file) = toml::from_str::<RegistryFile>(&disk_str) else {
            tracing::warn!("Registry {:?} unparseable mid-save; writing in-memory map only", self.path);
            return merged;
        };
        for m in file.contexts {
            match merged.get_mut(&m.id) {
                None => { merged.insert(m.id.clone(), m); }
                Some(mine) => {
                    let take_disk = m.version > mine.version
                        || (m.version == mine.version
                            && ((m.state == ContextState::Deleted
                                 && mine.state != ContextState::Deleted)
                                || m.last_accessed > mine.last_accessed));
                    if take_disk { *mine = m; }
                }
            }
        }
        merged
    }

    // -------------------------------------------------------------------------
    // Creation
    // -------------------------------------------------------------------------

    /// Register a new context (state = Creating). Errors if ID already exists.
    pub fn create(&mut self, mut meta: ContextMeta) -> Result<ContextDescriptor> {
        if self.entries.contains_key(&meta.id) {
            anyhow::bail!("E_DUPLICATE_ID: context {} already exists", meta.id);
        }
        meta.state = ContextState::Creating;
        let desc = ContextDescriptor::from(&meta);
        self.entries.insert(meta.id.clone(), meta);
        self.save()?;
        Ok(desc)
    }

    // -------------------------------------------------------------------------
    // State transitions (ALL state changes go through here)
    // -------------------------------------------------------------------------

    /// Attempt a state transition for the given context.
    ///
    /// - Validates the transition via `fsm::validate` (hard gate).
    /// - Applies state-specific side effects (timestamp, access count).
    /// - Persists to disk after every successful transition.
    ///
    /// Returns `Err` for:
    ///   - `E_NOT_FOUND`          — context ID does not exist
    ///   - `E_INVALID_TRANSITION` — FSM rejects (from, to) pair
    ///   - I/O errors on save
    pub fn transition(
        &mut self,
        id: &ContextId,
        target: ContextState,
    ) -> Result<()> {
        let meta = self.entries.get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("E_NOT_FOUND: {id}"))?;

        // FSM gate: reject invalid transitions immediately
        fsm::validate(meta.state, target)
            .map_err(|e: TransitionError| anyhow::anyhow!("{e}"))?;

        let now = now_secs();

        // Apply state-specific side effects BEFORE updating state
        match target {
            ContextState::Active => {
                meta.last_accessed = now;
                meta.access_count  += 1;
            }
            ContextState::Deleted => {
                // Terminal state: timestamp the deletion
                meta.last_accessed = now;
            }
            // Idle, Sleeping, Recovering, Archived, Creating:
            // No extra side effects needed beyond the state change
            _ => {}
        }

        let prev = meta.state;
        meta.state = target;

        tracing::info!(
            "Context {} transitioned: {} -> {}",
            id, prev, target
        );

        self.save()
    }

    // -------------------------------------------------------------------------
    // Non-state mutations
    // -------------------------------------------------------------------------

    /// Bump version and learn_count after a successful learn operation.
    /// Does NOT change state.
    pub fn record_learn(&mut self, id: &ContextId) {
        if let Some(meta) = self.entries.get_mut(id) {
            meta.version      += 1;
            meta.learn_count  += 1;
            meta.last_accessed = now_secs();
        }
    }

    /// Bump access_count and last_accessed without changing state.
    /// Called when a context is already Active and we just want to track usage.
    pub fn record_access(&mut self, id: &ContextId) {
        if let Some(meta) = self.entries.get_mut(id) {
            meta.access_count  += 1;
            meta.last_accessed  = now_secs();
        }
    }

    /// Merge keyword tags into a context's routing metadata.
    ///
    /// New tags are lowercased, deduplicated against existing tags, and
    /// appended up to `max_tags` total. Excess tags (oldest first) are
    /// dropped if the cap is hit. Persists to disk.
    ///
    /// Does NOT change context state.
    pub fn add_tags(&mut self, id: &ContextId, new_tags: Vec<String>, max_tags: usize) -> Result<()> {
        let meta = self.entries.get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("E_NOT_FOUND: {id}"))?;

        for tag in new_tags {
            let tag_lower = tag.to_lowercase();
            if tag_lower.len() < 2 { continue; }
            if !meta.tags.contains(&tag_lower) {
                meta.tags.push(tag_lower);
            }
        }

        // Cap: keep the most-recently-added tags (tail of the vec)
        if meta.tags.len() > max_tags {
            let excess = meta.tags.len() - max_tags;
            meta.tags.drain(0..excess);
        }

        self.save()
    }

    // -------------------------------------------------------------------------
    // Queries
    // -------------------------------------------------------------------------

    /// Get metadata for a context.
    pub fn get(&self, id: &ContextId) -> Option<&ContextMeta> {
        self.entries.get(id)
    }

    /// All non-Deleted contexts as descriptors.
    pub fn list(&self) -> Vec<ContextDescriptor> {
        self.entries.values()
            .filter(|m| m.state != ContextState::Deleted)
            .map(ContextDescriptor::from)
            .collect()
    }

    /// All contexts including Deleted (for audit / tombstone checks).
    pub fn list_all(&self) -> Vec<ContextDescriptor> {
        self.entries.values().map(ContextDescriptor::from).collect()
    }

    /// Find by display name (case-insensitive, first match).
    pub fn find_by_name(&self, name: &str) -> Option<&ContextMeta> {
        let lower = name.to_lowercase();
        self.entries.values()
            .find(|m| m.display_name.to_lowercase() == lower
                   && m.state != ContextState::Deleted)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    // -------------------------------------------------------------------------
    // Integration tests
    // -------------------------------------------------------------------------

    #[cfg(test)]
    pub fn new_in_memory() -> Self {
        Self {
            entries:      HashMap::new(),
            path:         PathBuf::from("_test_registry_unused.toml"),
            save_enabled: false,
        }
    }
}

// =============================================================================
// Integration tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ContextType, now_secs};

    fn make_meta(name: &str) -> ContextMeta {
        ContextMeta {
            id:            ContextId::new(),
            display_name:  name.to_string(),
            context_type:  ContextType::Project,
            state:         ContextState::Creating,
            parent_id:     None,
            description:   String::new(),
            tags:          vec![],
            created_at:    now_secs(),
            last_accessed: now_secs(),
            access_count:  0,
            version:       0,
            learn_count:   0,
        }
    }

    fn registry_with(name: &str) -> (Registry, ContextId) {
        let mut reg = Registry::new_in_memory();
        // Bypass normal create() to inject test meta directly
        let meta = make_meta(name);
        let id = meta.id.clone();
        reg.entries.insert(id.clone(), meta);
        (reg, id)
    }

    // --- Valid transitions via registry --------------------------------------

    #[test]
    fn creating_to_active_succeeds() {
        let (mut reg, id) = registry_with("ctx");
        assert!(reg.transition(&id, ContextState::Active).is_ok());
        assert_eq!(reg.get(&id).unwrap().state, ContextState::Active);
    }

    #[test]
    fn active_to_idle_succeeds() {
        let (mut reg, id) = registry_with("ctx");
        reg.transition(&id, ContextState::Active).unwrap();
        assert!(reg.transition(&id, ContextState::Idle).is_ok());
        assert_eq!(reg.get(&id).unwrap().state, ContextState::Idle);
    }

    #[test]
    fn idle_to_sleeping_succeeds() {
        let (mut reg, id) = registry_with("ctx");
        reg.transition(&id, ContextState::Active).unwrap();
        reg.transition(&id, ContextState::Idle).unwrap();
        assert!(reg.transition(&id, ContextState::Sleeping).is_ok());
    }

    #[test]
    fn sleeping_to_recovering_succeeds() {
        let (mut reg, id) = registry_with("ctx");
        reg.transition(&id, ContextState::Active).unwrap();
        reg.transition(&id, ContextState::Sleeping).unwrap();
        assert!(reg.transition(&id, ContextState::Recovering).is_ok());
    }

    #[test]
    fn recovering_to_active_succeeds() {
        let (mut reg, id) = registry_with("ctx");
        reg.transition(&id, ContextState::Active).unwrap();
        reg.transition(&id, ContextState::Sleeping).unwrap();
        reg.transition(&id, ContextState::Recovering).unwrap();
        assert!(reg.transition(&id, ContextState::Active).is_ok());
    }

    // --- Invalid transitions are rejected ------------------------------------

    #[test]
    fn creating_to_idle_rejected() {
        let (mut reg, id) = registry_with("ctx");
        let err = reg.transition(&id, ContextState::Idle).unwrap_err();
        assert!(err.to_string().contains("E_INVALID_TRANSITION"));
    }

    #[test]
    fn deleted_to_active_rejected() {
        let (mut reg, id) = registry_with("ctx");
        reg.transition(&id, ContextState::Active).unwrap();
        reg.transition(&id, ContextState::Deleted).unwrap();
        let err = reg.transition(&id, ContextState::Active).unwrap_err();
        assert!(err.to_string().contains("E_INVALID_TRANSITION"));
    }

    #[test]
    fn active_to_archived_rejected() {
        let (mut reg, id) = registry_with("ctx");
        reg.transition(&id, ContextState::Active).unwrap();
        let err = reg.transition(&id, ContextState::Archived).unwrap_err();
        assert!(err.to_string().contains("E_INVALID_TRANSITION"));
    }

    // --- Side effects --------------------------------------------------------

    #[test]
    fn transition_to_active_bumps_access_count() {
        let (mut reg, id) = registry_with("ctx");
        assert_eq!(reg.get(&id).unwrap().access_count, 0);
        reg.transition(&id, ContextState::Active).unwrap();
        assert_eq!(reg.get(&id).unwrap().access_count, 1);
        // Idle -> Active bumps again
        reg.transition(&id, ContextState::Idle).unwrap();
        reg.transition(&id, ContextState::Active).unwrap();
        assert_eq!(reg.get(&id).unwrap().access_count, 2);
    }

    #[test]
    fn record_learn_bumps_version_and_count() {
        let (mut reg, id) = registry_with("ctx");
        reg.transition(&id, ContextState::Active).unwrap();
        reg.record_learn(&id);
        reg.record_learn(&id);
        let m = reg.get(&id).unwrap();
        assert_eq!(m.version, 2);
        assert_eq!(m.learn_count, 2);
    }

    #[test]
    fn corrupt_registry_is_quarantined_not_silently_reset() {
        let dir = std::env::temp_dir().join(format!("cmhm_registry_corrupt_{}", ContextId::new().as_str()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("registry.toml");
        std::fs::write(&path, "this is = not [[ valid toml =").unwrap();

        let reg = Registry::load(&dir).expect("load must recover on corrupt file");

        // Fresh registry (no phantom contexts from the corrupt file)
        assert_eq!(reg.entries.len(), 0);
        // Original path is either gone or replaced — crucially the corrupt
        // bytes must survive in a quarantined sibling file.
        let quarantined: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("registry.toml.corrupt."))
            .collect();
        assert!(!quarantined.is_empty(), "expected a quarantined registry file, dir had: {:?}",
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect::<Vec<_>>());
        let q_path = dir.join(&quarantined[0]);
        assert_eq!(std::fs::read_to_string(&q_path).unwrap(), "this is = not [[ valid toml =");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- B11: cross-instance lost update ------------------------------------

    #[test]
    fn concurrent_instance_create_is_not_lost() {
        let dir = std::env::temp_dir().join(format!("cmhm_registry_b11_{}", ContextId::new().as_str()));
        std::fs::create_dir_all(&dir).unwrap();

        // Instance 1 boots on an empty dir and creates ctx-a.
        let mut r1 = Registry::load(&dir).unwrap();
        let d_a = r1.create(make_meta("ctx-a")).unwrap();

        // Instance 2 boots (sees ctx-a) and creates ctx-b.
        let mut r2 = Registry::load(&dir).unwrap();
        let d_b = r2.create(make_meta("ctx-b")).unwrap();

        // Instance 1's map predates ctx-b; its next save must NOT clobber B11.
        r1.create(make_meta("ctx-c")).unwrap();

        let r3 = Registry::load(&dir).unwrap();
        assert!(r3.get(&d_a.id).is_some(), "own create (a) lost from file");
        assert!(r3.get(&d_b.id).is_some(), "B11: foreign create (b) clobbered by stale-instance save");
        assert_eq!(r3.find_by_name("ctx-b").map(|m| m.id.clone()), Some(d_b.id.clone()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_delete_tombstone_survives_stale_writer() {
        let dir = std::env::temp_dir().join(format!("cmhm_registry_b11d_{}", ContextId::new().as_str()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut r1 = Registry::load(&dir).unwrap();
        let d_a = r1.create(make_meta("ctx-a")).unwrap();
        let mut r2 = Registry::load(&dir).unwrap();

        // Both instances transition a to Active (millisecond coarseness of
        // now_secs makes timestamps often equal — the state tie-break is the
        // thing under test, not the clock).
        r1.transition(&d_a.id, ContextState::Active).unwrap();
        r2.transition(&d_a.id, ContextState::Active).unwrap();

        // Instance 1 deletes (Deleted tombstone, kept in map) then writes.
        r1.transition(&d_a.id, ContextState::Deleted).unwrap();
        r1.create(make_meta("ctx-b")).unwrap();

        // Instance 2 writes with its stale map (a: Active).
        r2.create(make_meta("ctx-c")).unwrap();

        let r3 = Registry::load(&dir).unwrap();
        let a = r3.get(&d_a.id).expect("tombstone entry must survive");
        assert_eq!(a.state, ContextState::Deleted,
            "stale Active copy must not resurrect a Deleted context");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_adopt_keeps_higher_version_disk_copy() {
        let dir = std::env::temp_dir().join(format!("cmhm_registry_b11v_{}", ContextId::new().as_str()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut r1 = Registry::load(&dir).unwrap();
        let d_a = r1.create(make_meta("ctx-a")).unwrap();

        // Simulate a foreign instance that learned twice (version 2 on disk).
        let mut foreign = make_meta("ctx-a");
        foreign.id = d_a.id.clone();
        foreign.version = 2;
        foreign.learn_count = 2;
        {
            let raw = RegistryFile { contexts: vec![foreign.clone()] };
            let s = toml::to_string_pretty(&raw).unwrap();
            std::fs::write(dir.join("registry.toml"), s).unwrap();
        }

        // Instance 1 (stale version 0) merely re-saves; disk must win on version.
        r1.transition(&d_a.id, ContextState::Active).unwrap();

        let r3 = Registry::load(&dir).unwrap();
        let a = r3.get(&d_a.id).expect("entry must survive");
        assert_eq!(a.version, 2, "fresher disk copy (learn version) must win arbitration");
        assert_eq!(a.learn_count, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
