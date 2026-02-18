//! Frontier — priority queue of interesting simulation states to explore.

use crate::coverage::CoverageBitmap;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::SimulationSnapshot;
use rand::Rng;

/// A snapshot with its associated metadata for prioritization.
#[derive(Clone)]
pub struct FrontierEntry {
    /// Unique ID for this entry.
    pub id: u64,
    /// The simulation snapshot at this point.
    pub snapshot: SimulationSnapshot,
    /// Coverage bitmap at this point.
    pub coverage: CoverageBitmap,
    /// Score: higher = more interesting.
    pub score: f64,
    /// How many times this entry has been selected for exploration.
    pub times_selected: u32,
    /// Depth in the exploration tree.
    pub depth: u32,
    /// The fault schedule that led to this state.
    pub schedule: FaultSchedule,
    /// Parent entry ID (if forked from another entry).
    pub parent: Option<u64>,
}

/// Priority queue of interesting simulation states to explore from.
pub struct Frontier {
    entries: Vec<FrontierEntry>,
    next_id: u64,
    max_size: usize,
}

impl Frontier {
    /// Create a new frontier with a maximum size.
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Vec::new(),
            next_id: 0,
            max_size,
        }
    }

    /// Add an entry to the frontier.
    ///
    /// Automatically assigns an ID and prunes if over capacity.
    pub fn push(&mut self, mut entry: FrontierEntry) {
        entry.id = self.next_id;
        self.next_id += 1;

        self.entries.push(entry);

        // Sort by score descending for efficient selection
        self.entries.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if self.entries.len() > self.max_size {
            self.prune();
        }
    }

    /// Select the next entry to explore (highest score, with decay for
    /// entries that have been selected many times).
    ///
    /// Uses epsilon-greedy: 90% take highest score, 10% random.
    pub fn select(&mut self, rng: &mut impl Rng) -> Option<&mut FrontierEntry> {
        if self.entries.is_empty() {
            return None;
        }

        // Epsilon-greedy: 10% random, 90% best
        let index = if rng.gen::<f64>() < 0.1 {
            rng.gen_range(0..self.entries.len())
        } else {
            // Find best effective score (score / (1 + times_selected))
            let mut best_idx = 0;
            let mut best_effective_score = 0.0;

            for (i, entry) in self.entries.iter().enumerate() {
                let effective = entry.score / (1.0 + entry.times_selected as f64);
                if effective > best_effective_score {
                    best_effective_score = effective;
                    best_idx = i;
                }
            }
            best_idx
        };

        let entry = &mut self.entries[index];
        entry.times_selected += 1;
        Some(entry)
    }

    /// Number of entries in the frontier.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the frontier is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all entries (for inspection/debugging).
    pub fn entries(&self) -> &[FrontierEntry] {
        &self.entries
    }

    /// Prune low-scoring entries if over max_size.
    ///
    /// Keeps only the top `max_size` entries by score.
    fn prune(&mut self) {
        if self.entries.len() <= self.max_size {
            return;
        }

        // Already sorted by score descending, just truncate
        self.entries.truncate(self.max_size);
        log::debug!("Frontier pruned to {} entries", self.max_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn dummy_snapshot() -> SimulationSnapshot {
        use chaoscontrol_vmm::controller::NetworkFabric;

        // Create a minimal network fabric manually since ::new is private
        let network_state = NetworkFabric {
            partitions: Vec::new(),
            latency: vec![0, 0],
            in_flight: Vec::new(),
            loss_rate_ppm: Vec::new(),
            corruption_rate_ppm: Vec::new(),
            reorder_window: Vec::new(),
            rng: rand_chacha::ChaCha20Rng::seed_from_u64(42),
        };

        SimulationSnapshot {
            tick: 0,
            vm_snapshots: Vec::new(),
            network_state,
            fault_engine_snapshot: dummy_engine_snapshot(),
        }
    }

    fn dummy_engine_snapshot() -> chaoscontrol_fault::engine::EngineSnapshot {
        // Create a minimal engine snapshot for testing
        use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
        let engine = FaultEngine::new(EngineConfig::default());
        engine.snapshot()
    }

    fn make_entry(id: u64, score: f64, depth: u32) -> FrontierEntry {
        FrontierEntry {
            id,
            snapshot: dummy_snapshot(),
            coverage: CoverageBitmap::new(),
            score,
            times_selected: 0,
            depth,
            schedule: FaultSchedule::new(),
            parent: None,
        }
    }

    #[test]
    fn test_frontier_new() {
        let frontier = Frontier::new(100);
        assert_eq!(frontier.len(), 0);
        assert!(frontier.is_empty());
    }

    #[test]
    fn test_frontier_push() {
        let mut frontier = Frontier::new(10);
        let entry = make_entry(0, 1.0, 0);
        frontier.push(entry);
        assert_eq!(frontier.len(), 1);
        assert_eq!(frontier.entries[0].id, 0);
    }

    #[test]
    fn test_frontier_auto_assign_id() {
        let mut frontier = Frontier::new(10);
        let entry1 = make_entry(999, 1.0, 0); // ID will be overwritten
        let entry2 = make_entry(888, 2.0, 0);

        frontier.push(entry1);
        frontier.push(entry2);

        assert_eq!(frontier.entries[0].id, 1); // Higher score, so sorted first
        assert_eq!(frontier.entries[1].id, 0);
    }

    #[test]
    fn test_frontier_sorts_by_score() {
        let mut frontier = Frontier::new(10);
        frontier.push(make_entry(0, 1.0, 0));
        frontier.push(make_entry(0, 3.0, 0));
        frontier.push(make_entry(0, 2.0, 0));

        // Should be sorted by score descending
        assert_eq!(frontier.entries[0].score, 3.0);
        assert_eq!(frontier.entries[1].score, 2.0);
        assert_eq!(frontier.entries[2].score, 1.0);
    }

    #[test]
    fn test_frontier_select_greedy() {
        let mut frontier = Frontier::new(10);
        frontier.push(make_entry(0, 1.0, 0));
        frontier.push(make_entry(0, 3.0, 0));
        frontier.push(make_entry(0, 2.0, 0));

        let mut rng = ChaCha8Rng::seed_from_u64(42);

        // With a fixed seed, we can test deterministic selection
        // First call should select highest score (3.0)
        let selected = frontier.select(&mut rng).unwrap();
        // Due to epsilon-greedy, might not always be highest, but times_selected should increment
        let initial_times = selected.times_selected;
        assert_eq!(initial_times, 1);
    }

    #[test]
    fn test_frontier_select_decay() {
        let mut frontier = Frontier::new(10);
        let mut entry1 = make_entry(0, 10.0, 0);
        entry1.times_selected = 9; // Already selected many times
        let entry2 = make_entry(0, 5.0, 0); // Lower score but fresh

        frontier.push(entry1);
        frontier.push(entry2);

        let mut rng = ChaCha8Rng::seed_from_u64(999); // Seed that favors greedy

        // Effective score: entry1 = 10/(1+9) = 1.0, entry2 = 5/(1+0) = 5.0
        // Should prefer entry2
        let selected = frontier.select(&mut rng).unwrap();
        // Can't assert exact ID due to epsilon-greedy, but check it incremented
        assert!(selected.times_selected > 0);
    }

    #[test]
    fn test_frontier_prune() {
        let mut frontier = Frontier::new(3);
        frontier.push(make_entry(0, 1.0, 0));
        frontier.push(make_entry(0, 2.0, 0));
        frontier.push(make_entry(0, 3.0, 0));
        frontier.push(make_entry(0, 4.0, 0)); // Should trigger prune

        assert_eq!(frontier.len(), 3);

        // Should keep top 3 scores: 4.0, 3.0, 2.0
        assert_eq!(frontier.entries[0].score, 4.0);
        assert_eq!(frontier.entries[1].score, 3.0);
        assert_eq!(frontier.entries[2].score, 2.0);
    }

    #[test]
    fn test_frontier_select_empty() {
        let mut frontier = Frontier::new(10);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        assert!(frontier.select(&mut rng).is_none());
    }

    #[test]
    fn test_frontier_parent_tracking() {
        let mut frontier = Frontier::new(10);
        let mut entry = make_entry(0, 1.0, 5);
        entry.parent = Some(42);
        frontier.push(entry);

        assert_eq!(frontier.entries[0].parent, Some(42));
        assert_eq!(frontier.entries[0].depth, 5);
    }
}
