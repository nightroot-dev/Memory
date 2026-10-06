use serde::{Deserialize, Serialize};

/// A single entry deferred credit propagation.
///
/// When an Action→Outcome edge (CausalHypothesis) gets updated, if a State→Action
/// Sequence edge preceded it in the retrieval path, we log this entry and propagate
/// η=0.1 fraction of the reinforcement back to the State→Action edge during consolidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeLogEntry {
    pub state_id: u64,
    pub action_id: u64,
    /// The actual increment (s_k * lr) that was added to alpha or beta.
    pub delta: f32,
    /// Whether this was a success (alpha) or failure (beta) update.
    pub was_success: bool,
}

/// Temporary store for deferred credit propagation.
/// Drained and cleared during each consolidation pass.
/// Not persisted to disk — loss on crash is acceptable.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct EpisodeLog {
    entries: Vec<EpisodeLogEntry>,
}

impl EpisodeLog {
    /// M-04: max entries before oldest are evicted.
    /// Credit propagation is approximate — losing some entries is preferable to
    /// unbounded RAM growth in long sessions that never hit consolidation threshold.
    pub const MAX_SIZE: usize = 1_000;

    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn log(&mut self, state_id: u64, action_id: u64, delta: f32, was_success: bool) {
        // M-04 fix: evict oldest when full to prevent unbounded accumulation.
        if self.entries.len() >= Self::MAX_SIZE {
            self.entries.remove(0); // oldest-first eviction
        }
        self.entries.push(EpisodeLogEntry { state_id, action_id, delta, was_success });
    }

    /// Drain all entries for consolidation. Leaves the log empty.
    pub fn drain(&mut self) -> Vec<EpisodeLogEntry> {
        std::mem::take(&mut self.entries)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
