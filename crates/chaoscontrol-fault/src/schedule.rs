//! Fault scheduling — when and what to inject.
//!
//! A [`FaultSchedule`] defines the sequence of faults to inject during a
//! simulation run.  Schedules can be built manually (for targeted testing)
//! or generated randomly by the [`FaultEngine`](super::engine::FaultEngine).

use crate::faults::Fault;
use crate::outcomes::{fault_schedule_id, FaultScheduleId};

/// A fault scheduled to fire at a specific virtual time.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScheduledFault {
    /// Virtual time (nanoseconds) at which to inject this fault.
    pub time_ns: u64,
    /// The fault to inject.
    pub fault: Fault,
    /// Optional human-readable label for logging.
    pub label: Option<String>,
}

impl ScheduledFault {
    /// Create a new scheduled fault.
    pub fn new(time_ns: u64, fault: Fault) -> Self {
        Self {
            time_ns,
            fault,
            label: None,
        }
    }

    /// Attach a label to this scheduled fault.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// An ordered sequence of faults to inject during a run.
///
/// Faults are sorted by time and consumed in order by the engine.
#[derive(Debug, Clone)]
pub struct FaultSchedule {
    faults: Vec<ScheduledFault>,
    /// Index of the next fault to deliver.
    cursor: usize,
}

impl FaultSchedule {
    /// Create an empty schedule.
    pub fn new() -> Self {
        Self {
            faults: Vec::new(),
            cursor: 0,
        }
    }

    /// Add a fault to the schedule.
    ///
    /// Faults are automatically sorted by time when built.
    pub fn add(&mut self, fault: ScheduledFault) {
        self.faults.push(fault);
        self.faults.sort_by_key(|f| f.time_ns);
    }

    /// Get all faults that should fire at or before `current_time_ns`.
    ///
    /// Advances the internal cursor past any returned faults.
    pub fn drain_due(&mut self, current_time_ns: u64) -> Vec<ScheduledFault> {
        self.drain_due_indexed(current_time_ns)
            .into_iter()
            .map(|(_, fault)| fault)
            .collect()
    }

    /// Get due faults with their authoritative canonical entry indices.
    pub fn drain_due_indexed(&mut self, current_time_ns: u64) -> Vec<(usize, ScheduledFault)> {
        let mut due = Vec::new();
        while self.cursor < self.faults.len() && self.faults[self.cursor].time_ns <= current_time_ns
        {
            due.push((self.cursor, self.faults[self.cursor].clone()));
            self.cursor += 1;
        }
        due
    }

    /// Peek at the next fault's scheduled time without consuming it.
    pub fn next_time(&self) -> Option<u64> {
        self.faults.get(self.cursor).map(|f| f.time_ns)
    }

    /// Number of faults remaining (not yet delivered).
    pub fn remaining(&self) -> usize {
        self.faults.len() - self.cursor
    }

    /// Total number of faults in the schedule.
    pub fn total(&self) -> usize {
        self.faults.len()
    }

    /// Read-only access to the fault list.
    pub fn faults(&self) -> &[ScheduledFault] {
        &self.faults
    }

    /// Return one canonical entry by index.
    pub fn entry(&self, index: usize) -> Option<&ScheduledFault> {
        self.faults.get(index)
    }

    /// Return the canonical BLAKE3 identity for the complete schedule.
    pub fn identity(&self) -> FaultScheduleId {
        fault_schedule_id(
            self.faults
                .iter()
                .map(|entry| (entry.time_ns, entry.label.as_deref(), &entry.fault)),
        )
    }

    /// Build a new schedule from a subset of faults (by index).
    ///
    /// Used by the minimizer to create candidate schedules with some
    /// faults removed.
    pub fn subset(&self, indices: &[usize]) -> Self {
        let mut schedule = Self::new();
        for &i in indices {
            if i < self.faults.len() {
                schedule.add(self.faults[i].clone());
            }
        }
        schedule
    }

    /// Reset the cursor to replay the schedule from the beginning.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Snapshot the schedule state for later restore.
    pub fn snapshot(&self) -> FaultScheduleSnapshot {
        FaultScheduleSnapshot {
            cursor: self.cursor,
        }
    }

    /// Check whether a snapshot cursor is within this schedule.
    pub fn snapshot_is_valid(&self, snapshot: &FaultScheduleSnapshot) -> bool {
        snapshot.cursor <= self.faults.len()
    }

    /// Restore schedule state from a validated snapshot.
    pub fn restore(&mut self, snapshot: &FaultScheduleSnapshot) {
        assert!(self.snapshot_is_valid(snapshot));
        self.cursor = snapshot.cursor;
    }
}

impl Default for FaultSchedule {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of a [`FaultSchedule`]'s progress.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FaultScheduleSnapshot {
    cursor: usize,
}

impl FaultScheduleSnapshot {
    /// Return the index of the next canonical schedule entry.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    #[cfg(test)]
    pub(crate) fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor;
    }
}

/// Builder for constructing fault schedules declaratively.
///
/// # Example
///
/// ```
/// use chaoscontrol_fault::schedule::FaultScheduleBuilder;
/// use chaoscontrol_fault::faults::Fault;
///
/// let schedule = FaultScheduleBuilder::new()
///     .at_ns(1_000_000_000, Fault::NetworkPartition {
///         side_a: vec![0],
///         side_b: vec![1, 2],
///     })
///     .at_ns(5_000_000_000, Fault::NetworkHeal)
///     .at_ns(8_000_000_000, Fault::ProcessKill { target: 1 })
///     .at_ns(10_000_000_000, Fault::ProcessRestart { target: 1 })
///     .build();
///
/// assert_eq!(schedule.total(), 4);
/// ```
pub struct FaultScheduleBuilder {
    schedule: FaultSchedule,
}

impl FaultScheduleBuilder {
    pub fn new() -> Self {
        Self {
            schedule: FaultSchedule::new(),
        }
    }

    /// Schedule a fault at a specific virtual time (nanoseconds).
    pub fn at_ns(mut self, time_ns: u64, fault: Fault) -> Self {
        self.schedule.add(ScheduledFault::new(time_ns, fault));
        self
    }

    /// Schedule a labeled fault at a specific virtual time.
    pub fn at_ns_labeled(mut self, time_ns: u64, fault: Fault, label: &str) -> Self {
        self.schedule
            .add(ScheduledFault::new(time_ns, fault).with_label(label));
        self
    }

    /// Build the final schedule.
    pub fn build(self) -> FaultSchedule {
        self.schedule
    }
}

impl Default for FaultScheduleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faults::Fault;

    #[test]
    fn empty_schedule() {
        let mut sched = FaultSchedule::new();
        assert_eq!(sched.remaining(), 0);
        assert_eq!(sched.next_time(), None);
        assert!(sched.drain_due(1_000_000).is_empty());
    }

    #[test]
    fn faults_sorted_by_time() {
        let mut sched = FaultSchedule::new();
        sched.add(ScheduledFault::new(3000, Fault::ProcessKill { target: 0 }));
        sched.add(ScheduledFault::new(1000, Fault::NetworkHeal));
        sched.add(ScheduledFault::new(2000, Fault::DiskFull { target: 1 }));

        assert_eq!(sched.next_time(), Some(1000));
        assert_eq!(sched.remaining(), 3);
    }

    #[test]
    fn drain_due_returns_correct_faults() {
        let mut sched = FaultSchedule::new();
        sched.add(ScheduledFault::new(100, Fault::NetworkHeal));
        sched.add(ScheduledFault::new(200, Fault::DiskFull { target: 0 }));
        sched.add(ScheduledFault::new(300, Fault::ProcessKill { target: 0 }));

        let due = sched.drain_due(150);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].fault, Fault::NetworkHeal);
        assert_eq!(sched.remaining(), 2);

        let due = sched.drain_due(300);
        assert_eq!(due.len(), 2);
        assert_eq!(sched.remaining(), 0);
    }

    #[test]
    fn drain_due_idempotent_at_same_time() {
        let mut sched = FaultSchedule::new();
        sched.add(ScheduledFault::new(100, Fault::NetworkHeal));

        let due = sched.drain_due(100);
        assert_eq!(due.len(), 1);

        // Draining again at same time yields nothing
        let due = sched.drain_due(100);
        assert!(due.is_empty());
    }

    #[test]
    fn snapshot_restore() {
        let mut sched = FaultSchedule::new();
        sched.add(ScheduledFault::new(100, Fault::NetworkHeal));
        sched.add(ScheduledFault::new(200, Fault::DiskFull { target: 0 }));

        sched.drain_due(100);
        let snap = sched.snapshot();
        assert_eq!(sched.remaining(), 1);

        sched.drain_due(200);
        assert_eq!(sched.remaining(), 0);

        sched.restore(&snap);
        assert_eq!(sched.remaining(), 1);
    }

    #[test]
    fn builder_produces_sorted_schedule() {
        let schedule = FaultScheduleBuilder::new()
            .at_ns(3000, Fault::ProcessKill { target: 0 })
            .at_ns(1000, Fault::NetworkHeal)
            .build();

        assert_eq!(schedule.total(), 2);
        assert_eq!(schedule.next_time(), Some(1000));
    }
}
