//! Coverage bitmap collection and comparison.
//!
//! Uses a 64KB bitmap with 8-bit saturating counters (AFL-style).
//! Coverage is collected from guest memory after each simulation run.

use vm_memory::{Bytes, GuestAddress, GuestMemory};

/// Coverage bitmap size (64 KB, same as AFL).
pub const MAP_SIZE: usize = 65536;

/// Coverage bitmap — tracks which code paths have been hit.
/// Uses 64KB bitmap with 8-bit saturating counters.
#[derive(Clone, Debug)]
pub struct CoverageBitmap {
    /// The bitmap: index = hash(branch_src, branch_dst) % MAP_SIZE
    map: Vec<u8>,
}

impl CoverageBitmap {
    /// Create a new empty coverage bitmap.
    pub fn new() -> Self {
        Self {
            map: vec![0; MAP_SIZE],
        }
    }

    /// Reset all counters to zero.
    pub fn clear(&mut self) {
        self.map.fill(0);
    }

    /// Record a branch hit at the given hash index.
    /// Saturates at 255.
    pub fn record_hit(&mut self, index: usize) {
        if index < MAP_SIZE {
            self.map[index] = self.map[index].saturating_add(1);
        }
    }

    /// Merge another bitmap into this one (union).
    /// Takes the maximum count at each position.
    pub fn merge(&mut self, other: &CoverageBitmap) {
        for (i, &other_count) in other.map.iter().enumerate() {
            self.map[i] = self.map[i].max(other_count);
        }
    }

    /// Count the number of non-zero entries (edges hit).
    pub fn count_bits(&self) -> usize {
        self.map.iter().filter(|&&c| c > 0).count()
    }

    /// Check if this bitmap has any bits NOT present in `global`.
    /// Returns the number of new bits found.
    pub fn has_new_coverage(&self, global: &CoverageBitmap) -> usize {
        self.map
            .iter()
            .zip(global.map.iter())
            .filter(|(&self_count, &global_count)| self_count > 0 && global_count == 0)
            .count()
    }

    /// Classify hit counts into AFL-style buckets for better diversity.
    /// Buckets: 1, 2, 3, 4-7, 8-15, 16-31, 32-127, 128+
    pub fn classify(&mut self) {
        for count in &mut self.map {
            *count = match *count {
                0 => 0,
                1 => 1,
                2 => 2,
                3 => 3,
                4..=7 => 4,
                8..=15 => 8,
                16..=31 => 16,
                32..=127 => 32,
                _ => 128,
            };
        }
    }

    /// Get a reference to the raw map.
    pub fn as_slice(&self) -> &[u8] {
        &self.map
    }

    /// Create from a raw byte slice (truncates/pads to MAP_SIZE).
    pub fn from_slice(data: &[u8]) -> Self {
        let mut map = vec![0; MAP_SIZE];
        let copy_len = data.len().min(MAP_SIZE);
        map[..copy_len].copy_from_slice(&data[..copy_len]);
        Self { map }
    }
}

impl Default for CoverageBitmap {
    fn default() -> Self {
        Self::new()
    }
}

/// How to collect coverage from a guest VM.
///
/// Phase 1 (now): Use a shared-memory page in guest memory where the SDK
/// instruments branches. The SDK writes branch hits to a known guest
/// physical address, and the VMM reads it after each run.
///
/// Phase 2 (future): Use hardware breakpoints or Intel PT for
/// zero-instrumentation coverage.
pub struct CoverageCollector {
    /// Guest physical address of the coverage bitmap.
    bitmap_gpa: u64,
    /// Size of the bitmap in guest memory.
    bitmap_size: usize,
    /// Accumulated global coverage (union of all runs).
    global_coverage: CoverageBitmap,
    /// Total unique edges discovered.
    total_edges: usize,
    /// Total runs processed.
    total_runs: u64,
}

impl CoverageCollector {
    /// Create a new collector.
    ///
    /// If `bitmap_gpa` is 0, operates in "blind mode" (no guest coverage).
    pub fn new(bitmap_gpa: u64) -> Self {
        Self {
            bitmap_gpa,
            bitmap_size: MAP_SIZE,
            global_coverage: CoverageBitmap::new(),
            total_edges: 0,
            total_runs: 0,
        }
    }

    /// Read the coverage bitmap from guest memory after a run.
    ///
    /// Returns an empty bitmap if in blind mode or read fails.
    pub fn collect_from_guest<M: GuestMemory>(&mut self, mem: &M) -> CoverageBitmap {
        if self.bitmap_gpa == 0 {
            // Blind mode: return empty coverage
            return CoverageBitmap::new();
        }

        let mut buffer = vec![0u8; self.bitmap_size];
        if mem
            .read_slice(&mut buffer, GuestAddress(self.bitmap_gpa))
            .is_err()
        {
            log::warn!(
                "Failed to read coverage bitmap from guest memory at {:#x}",
                self.bitmap_gpa
            );
            return CoverageBitmap::new();
        }

        self.total_runs += 1;
        CoverageBitmap::from_slice(&buffer)
    }

    /// Check if a bitmap has new coverage not yet in the global map.
    pub fn is_interesting(&self, bitmap: &CoverageBitmap) -> bool {
        bitmap.has_new_coverage(&self.global_coverage) > 0
    }

    /// Merge a bitmap into the global coverage.
    pub fn update_global(&mut self, bitmap: &CoverageBitmap) {
        let before = self.global_coverage.count_bits();
        self.global_coverage.merge(bitmap);
        let after = self.global_coverage.count_bits();
        let new_edges = after.saturating_sub(before);
        self.total_edges = after;
        if new_edges > 0 {
            log::info!(
                "New coverage: {} edges (total: {})",
                new_edges,
                self.total_edges
            );
        }
    }

    /// Get global coverage stats.
    pub fn stats(&self) -> CoverageStats {
        CoverageStats {
            total_edges: self.total_edges,
            total_runs: self.total_runs,
            edges_per_run_avg: if self.total_runs > 0 {
                self.total_edges as f64 / self.total_runs as f64
            } else {
                0.0
            },
        }
    }

    /// Get a reference to the global coverage bitmap.
    pub fn global_coverage(&self) -> &CoverageBitmap {
        &self.global_coverage
    }
}

/// Coverage statistics.
#[derive(Debug, Clone)]
pub struct CoverageStats {
    pub total_edges: usize,
    pub total_runs: u64,
    pub edges_per_run_avg: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_new() {
        let bitmap = CoverageBitmap::new();
        assert_eq!(bitmap.count_bits(), 0);
    }

    #[test]
    fn test_bitmap_record_hit() {
        let mut bitmap = CoverageBitmap::new();
        bitmap.record_hit(0);
        bitmap.record_hit(100);
        bitmap.record_hit(100); // Hit twice
        assert_eq!(bitmap.count_bits(), 2);
        assert_eq!(bitmap.as_slice()[0], 1);
        assert_eq!(bitmap.as_slice()[100], 2);
    }

    #[test]
    fn test_bitmap_saturate() {
        let mut bitmap = CoverageBitmap::new();
        for _ in 0..300 {
            bitmap.record_hit(5);
        }
        assert_eq!(bitmap.as_slice()[5], 255); // Saturated
    }

    #[test]
    fn test_bitmap_clear() {
        let mut bitmap = CoverageBitmap::new();
        bitmap.record_hit(10);
        bitmap.record_hit(20);
        assert_eq!(bitmap.count_bits(), 2);
        bitmap.clear();
        assert_eq!(bitmap.count_bits(), 0);
    }

    #[test]
    fn test_bitmap_merge() {
        let mut b1 = CoverageBitmap::new();
        b1.record_hit(1);
        b1.record_hit(1);
        b1.record_hit(1); // count=3

        let mut b2 = CoverageBitmap::new();
        b2.record_hit(1);
        b2.record_hit(2);

        b1.merge(&b2);
        assert_eq!(b1.count_bits(), 2);
        assert_eq!(b1.as_slice()[1], 3); // max(3, 1)
        assert_eq!(b1.as_slice()[2], 1);
    }

    #[test]
    fn test_bitmap_has_new_coverage() {
        let mut global = CoverageBitmap::new();
        global.record_hit(10);

        let mut new = CoverageBitmap::new();
        new.record_hit(10);
        new.record_hit(20);

        assert_eq!(new.has_new_coverage(&global), 1); // edge 20 is new
    }

    #[test]
    fn test_bitmap_classify() {
        let mut bitmap = CoverageBitmap::new();
        bitmap.map[0] = 1;
        bitmap.map[1] = 2;
        bitmap.map[2] = 3;
        bitmap.map[3] = 5; // 4-7
        bitmap.map[4] = 10; // 8-15
        bitmap.map[5] = 20; // 16-31
        bitmap.map[6] = 50; // 32-127
        bitmap.map[7] = 200; // 128+

        bitmap.classify();

        assert_eq!(bitmap.map[0], 1);
        assert_eq!(bitmap.map[1], 2);
        assert_eq!(bitmap.map[2], 3);
        assert_eq!(bitmap.map[3], 4);
        assert_eq!(bitmap.map[4], 8);
        assert_eq!(bitmap.map[5], 16);
        assert_eq!(bitmap.map[6], 32);
        assert_eq!(bitmap.map[7], 128);
    }

    #[test]
    fn test_collector_blind_mode() {
        use vm_memory::GuestMemoryMmap;

        let mut collector = CoverageCollector::new(0); // 0 = blind mode

        // Create a dummy guest memory (won't be used in blind mode)
        let mem: GuestMemoryMmap =
            GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 1024 * 1024)]).unwrap();

        let bitmap = collector.collect_from_guest(&mem);
        assert_eq!(bitmap.count_bits(), 0);
        // In blind mode (bitmap_gpa == 0), we return early without reading
        // or incrementing total_runs
        assert_eq!(collector.total_runs, 0);
    }

    #[test]
    fn test_collector_is_interesting() {
        let collector = CoverageCollector::new(0x1000);

        let mut bitmap = CoverageBitmap::new();
        bitmap.record_hit(10);

        assert!(collector.is_interesting(&bitmap));
    }

    #[test]
    fn test_collector_update_global() {
        let mut collector = CoverageCollector::new(0x1000);

        let mut b1 = CoverageBitmap::new();
        b1.record_hit(1);
        b1.record_hit(2);

        collector.update_global(&b1);
        assert_eq!(collector.total_edges, 2);

        let mut b2 = CoverageBitmap::new();
        b2.record_hit(2);
        b2.record_hit(3);

        collector.update_global(&b2);
        assert_eq!(collector.total_edges, 3); // edges 1, 2, 3
    }

    #[test]
    fn test_collector_stats() {
        let mut collector = CoverageCollector::new(0x1000);
        collector.total_runs = 5;
        collector.total_edges = 100;

        let stats = collector.stats();
        assert_eq!(stats.total_edges, 100);
        assert_eq!(stats.total_runs, 5);
        assert_eq!(stats.edges_per_run_avg, 20.0);
    }

    #[test]
    fn test_bitmap_from_slice() {
        let data = vec![1, 2, 3, 4, 5];
        let bitmap = CoverageBitmap::from_slice(&data);
        assert_eq!(bitmap.as_slice()[0], 1);
        assert_eq!(bitmap.as_slice()[4], 5);
        assert_eq!(bitmap.as_slice()[5], 0);
        assert_eq!(bitmap.as_slice().len(), MAP_SIZE);
    }
}
