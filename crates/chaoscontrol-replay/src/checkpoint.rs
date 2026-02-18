//! Checkpoint storage and management.
//!
//! A checkpoint captures a complete simulation state at a point in time,
//! including VM snapshots, serial output, and event history.

use crate::recording::RecordedEvent;
use chaoscontrol_vmm::controller::SimulationSnapshot;
use serde::{Deserialize, Serialize};

/// A checkpoint = snapshot + metadata at a point in time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique ID within the recording.
    pub id: u64,
    /// Simulation tick at checkpoint time.
    pub tick: u64,
    /// Full simulation snapshot.
    #[serde(skip)] // Too large for JSON, recreated by replay
    pub snapshot: Option<SimulationSnapshot>,
    /// Serial output accumulated up to this point (per VM).
    pub serial_output: Vec<String>,
    /// Events that occurred since the last checkpoint.
    pub events_since_last: Vec<RecordedEvent>,
}

/// Manages a sequence of checkpoints with efficient lookup.
#[derive(Clone, Debug, Default)]
pub struct CheckpointStore {
    checkpoints: Vec<Checkpoint>,
}

impl CheckpointStore {
    /// Create a new empty checkpoint store.
    pub fn new() -> Self {
        Self {
            checkpoints: Vec::new(),
        }
    }

    /// Add a checkpoint.
    pub fn push(&mut self, cp: Checkpoint) {
        self.checkpoints.push(cp);
        // Keep sorted by tick
        self.checkpoints.sort_by_key(|c| c.tick);
    }

    /// Find the checkpoint at or just before the given tick.
    pub fn at_or_before(&self, tick: u64) -> Option<&Checkpoint> {
        self.checkpoints
            .iter()
            .rev()
            .find(|cp| cp.tick <= tick)
    }

    /// Get checkpoint by ID.
    pub fn get(&self, id: u64) -> Option<&Checkpoint> {
        self.checkpoints.iter().find(|cp| cp.id == id)
    }

    /// Get all checkpoints.
    pub fn all(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// Number of checkpoints.
    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.checkpoints.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_checkpoint(id: u64, tick: u64) -> Checkpoint {
        Checkpoint {
            id,
            tick,
            snapshot: None,
            serial_output: vec![],
            events_since_last: vec![],
        }
    }

    #[test]
    fn test_checkpoint_store_new() {
        let store = CheckpointStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_checkpoint_store_push() {
        let mut store = CheckpointStore::new();
        store.push(make_checkpoint(0, 100));
        store.push(make_checkpoint(1, 200));
        
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn test_checkpoint_store_sorts_by_tick() {
        let mut store = CheckpointStore::new();
        store.push(make_checkpoint(1, 200));
        store.push(make_checkpoint(0, 100));
        store.push(make_checkpoint(2, 150));
        
        let all = store.all();
        assert_eq!(all[0].tick, 100);
        assert_eq!(all[1].tick, 150);
        assert_eq!(all[2].tick, 200);
    }

    #[test]
    fn test_checkpoint_at_or_before() {
        let mut store = CheckpointStore::new();
        store.push(make_checkpoint(0, 100));
        store.push(make_checkpoint(1, 200));
        store.push(make_checkpoint(2, 300));
        
        assert_eq!(store.at_or_before(50).map(|c| c.tick), None);
        assert_eq!(store.at_or_before(100).map(|c| c.tick), Some(100));
        assert_eq!(store.at_or_before(150).map(|c| c.tick), Some(100));
        assert_eq!(store.at_or_before(200).map(|c| c.tick), Some(200));
        assert_eq!(store.at_or_before(250).map(|c| c.tick), Some(200));
        assert_eq!(store.at_or_before(300).map(|c| c.tick), Some(300));
        assert_eq!(store.at_or_before(350).map(|c| c.tick), Some(300));
    }

    #[test]
    fn test_checkpoint_get_by_id() {
        let mut store = CheckpointStore::new();
        store.push(make_checkpoint(10, 100));
        store.push(make_checkpoint(20, 200));
        
        assert!(store.get(10).is_some());
        assert!(store.get(20).is_some());
        assert!(store.get(30).is_none());
    }

    #[test]
    fn test_checkpoint_all() {
        let mut store = CheckpointStore::new();
        store.push(make_checkpoint(0, 100));
        store.push(make_checkpoint(1, 200));
        
        let all = store.all();
        assert_eq!(all.len(), 2);
    }
}
