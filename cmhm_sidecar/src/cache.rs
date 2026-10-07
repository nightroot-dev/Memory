//! Small LRU cache for repeated query text.
//!
//! Agents repeat queries far more than they invent new ones; a hit avoids a
//! model call entirely. Capacity-bounded; eviction is oldest-insertion (a
//! true LRU costs a linked map — insertion order approximates it well here
//! because hits re-insert).

use std::collections::{HashMap, VecDeque};

pub struct LruCache {
    cap: usize,
    map: HashMap<String, Vec<f32>>,
    order: VecDeque<String>,
    pub hits: u64,
    pub misses: u64,
}

impl LruCache {
    pub fn new(cap: usize) -> Self {
        Self { cap: cap.max(1), map: HashMap::new(), order: VecDeque::new(), hits: 0, misses: 0 }
    }

    pub fn get(&mut self, key: &str) -> Option<Vec<f32>> {
        if let Some(v) = self.map.get(key).cloned() {
            // Refresh recency.
            self.order.retain(|k| k != key);
            self.order.push_back(key.to_string());
            self.hits += 1;
            Some(v)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Non-mutating read (no recency refresh, no stats). For fan-out paths.
    pub fn get_clone(&self, key: &str) -> Option<Vec<f32>> {
        self.map.get(key).cloned()
    }

    pub fn put(&mut self, key: String, value: Vec<f32>) {
        if self.map.contains_key(&key) {
            self.order.retain(|k| k != &key);
        } else if self.map.len() >= self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_miss_and_eviction() {
        let mut c = LruCache::new(2);
        assert!(c.get("a").is_none());
        c.put("a".into(), vec![1.0]);
        c.put("b".into(), vec![2.0]);
        assert_eq!(c.get("a"), Some(vec![1.0]));
        // Inserting c evicts b (a was refreshed).
        c.put("c".into(), vec![3.0]);
        assert!(c.get("b").is_none());
        assert_eq!(c.get("c"), Some(vec![3.0]));
        assert_eq!((c.hits, c.misses), (2, 2));
    }
}
