use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

/// Working Memory Buffer — sliding window of the 5 most recently active node IDs.
///
/// Used during Edge Birth to find candidate source nodes for new edges.
/// Implements a VecDeque with max capacity 5; push evicts oldest on overflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingMemory {
    buffer: VecDeque<u64>,
    capacity: usize,
}

impl WorkingMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a new node ID to the front. Evicts oldest if at capacity.
    pub fn push(&mut self, node_id: u64) {
        if self.capacity == 0 {
            return; // degenerate buffer: refuse to grow
        }
        // Duplicate touches refresh recency instead of filling the buffer.
        self.buffer.retain(|&id| id != node_id);
        if self.buffer.len() == self.capacity {
            self.buffer.pop_back();
        }
        self.buffer.push_front(node_id);
    }

    /// Iterate IDs from most recent to oldest.
    pub fn recent(&self) -> impl Iterator<Item = u64> + '_ {
        self.buffer.iter().copied()
    }

    /// All IDs excluding a given one (used during edge birth to exclude the new node itself).
    pub fn recent_excluding(&self, exclude: u64) -> impl Iterator<Item = u64> + '_ {
        self.buffer.iter().copied().filter(move |&id| id != exclude)
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_evict() {
        let mut wm = WorkingMemory::new(5);
        for i in 1..=6 {
            wm.push(i);
        }
        assert_eq!(wm.len(), 5);
        // Most recent is 6, oldest (1) should be evicted
        let ids: Vec<u64> = wm.recent().collect();
        assert_eq!(ids[0], 6);
        assert!(!ids.contains(&1), "id 1 should have been evicted");
    }

    #[test]
    fn test_exclude() {
        let mut wm = WorkingMemory::new(5);
        wm.push(1); wm.push(2); wm.push(3);
        let ids: Vec<u64> = wm.recent_excluding(2).collect();
        assert!(!ids.contains(&2));
        assert!(ids.contains(&1) && ids.contains(&3));
    }
}
