//! Coverage-guided exploration engine for ChaosControl.
//!
//! This crate implements the core "multiverse" exploration loop that makes
//! ChaosControl valuable for distributed system testing:
//!
//! 1. **Fork-from-snapshot** to create parallel simulation branches
//! 2. **Coverage collection** from guest VMs
//! 3. **Coverage-guided search** to explore the state space efficiently
//!
//! # Architecture
//!
//! The exploration loop works like AFL/libFuzzer applied to distributed systems:
//!
//! ```text
//! 1. Boot simulation → run until interesting state → SNAPSHOT
//! 2. From snapshot, create N branches with different:
//!    - Fault schedules (different network partitions, crashes, etc.)
//!    - Random seeds (different guest-visible randomness)
//!    - Input sequences (different client requests)
//! 3. Run each branch for M ticks
//! 4. Collect: coverage bitmaps, assertion results, crashes
//! 5. Score each branch by coverage novelty
//! 6. Keep branches that found new coverage → add to frontier
//! 7. From frontier, select highest-scoring snapshots → go to step 2
//! 8. Repeat until time budget exhausted
//! ```
//!
//! # Example Usage
//!
//! ```no_run
//! use chaoscontrol_explore::explorer::{Explorer, ExplorerConfig};
//! use chaoscontrol_explore::report::format_report;
//! use chaoscontrol_vmm::vm::VmConfig;
//!
//! let config = ExplorerConfig {
//!     num_vms: 3,
//!     vm_config: VmConfig::default(),
//!     kernel_path: "/path/to/vmlinux".to_string(),
//!     initrd_path: Some("/path/to/initrd".to_string()),
//!     seed: 42,
//!     branch_factor: 8,
//!     ticks_per_branch: 1000,
//!     max_rounds: 100,
//!     ..Default::default()
//! };
//!
//! let mut explorer = Explorer::new(config);
//! let report = explorer.run().unwrap();
//!
//! println!("{}", format_report(&report));
//! ```
//!
//! # Module Structure
//!
//! - [`coverage`] — Coverage bitmap collection and comparison (AFL-style)
//! - [`frontier`] — Priority queue of interesting simulation states
//! - [`mutator`] — Generates variant fault schedules from a base
//! - [`corpus`] — Stores interesting inputs/schedules found so far
//! - [`explorer`] — The main exploration loop
//! - [`report`] — Exploration session reports
//!
//! # Determinism
//!
//! All exploration is deterministic given the same seed. The explorer uses
//! seeded RNGs throughout and avoids HashMaps (using BTreeMap instead).

pub mod corpus;
pub mod coverage;
pub mod explorer;
pub mod frontier;
pub mod mutator;
pub mod report;

// Re-export main types for convenience
pub use corpus::{BugReport, Corpus, CorpusEntry, CorpusStats};
pub use coverage::{CoverageBitmap, CoverageCollector, CoverageStats};
pub use explorer::{
    BranchResult, ExplorationReport, ExplorationStats, ExploreError, Explorer, ExplorerConfig,
    RoundReport,
};
pub use frontier::{Frontier, FrontierEntry};
pub use mutator::{MutationConfig, ScheduleMutator};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all main types are accessible
        let _ = CoverageBitmap::new();
        let _ = CoverageCollector::new(0);
        let _ = Corpus::new();
        let _ = Frontier::new(10);
        let _ = ScheduleMutator::new(42);
        let _ = ExplorerConfig::default();
    }
}
