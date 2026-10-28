//! Cooperative logical-thread runtime for the admitted KVM race fixture.
//!
//! The runtime owns stable logical identities, a safe-point gate, and one
//! deterministic round-robin scheduler. It never uses host signals or sleeps.

#![no_std]

/// Fixed guest-physical gate page used by the thread-pause fixture.
pub const GATE_ADDRESS: usize = 0x6000;
/// Gate magic written by the host before the guest starts.
pub const GATE_MAGIC: u64 = 0x4343_5448_5245_4144;

/// Host-written pause gate and guest-written progress facts.
#[repr(C)]
pub struct PauseGate {
    pub magic: u64,
    pub paused_ordinal: u64,
    pub release_tick: u64,
    pub logical_tick: u64,
    pub paused_steps: u64,
}

/// Stable logical thread identity owned by the cooperative runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalThreadId {
    pub generation: u32,
    pub ordinal: u32,
}

/// One cooperative task with a two-phase read-modify-write body.
#[derive(Debug, Clone, Copy)]
pub struct Task {
    pub id: LogicalThreadId,
    pub delta: u64,
    phase: u8,
    local: u64,
    pub done: bool,
}

impl Task {
    pub const fn new(ordinal: u32, delta: u64) -> Self {
        Self {
            id: LogicalThreadId {
                generation: 1,
                ordinal,
            },
            delta,
            phase: 0,
            local: 0,
            done: false,
        }
    }

    /// One safe point plus one task step.
    pub fn step(&mut self, shared: &mut u64) {
        match self.phase {
            0 => {
                self.local = *shared;
                self.phase = 1;
            }
            1 => {
                *shared = self.local.wrapping_add(self.delta);
                self.done = true;
            }
            _ => {}
        }
    }
}

/// Read the fixed gate page.
///
/// # Safety
/// The caller must run in the admitted guest whose page tables map this
/// physical address writable.
pub unsafe fn gate() -> &'static mut PauseGate {
    // SAFETY: the admitted guest maps the fixed page and the host writes it
    // before the guest starts.
    unsafe { &mut *(GATE_ADDRESS as *mut PauseGate) }
}

/// Run the deterministic two-task race until both tasks complete.
///
/// Each scheduler step is one logical tick. A paused task is skipped without
/// advancing its phase; the logical tick still advances, so release is exact.
pub fn run_with_gate(gate: &mut PauseGate, shared: &mut u64) -> (u64, u64) {
    let mut tasks = [Task::new(0, 1), Task::new(1, 10)];
    while !tasks.iter().all(|task| task.done) {
        for task in &mut tasks {
            let tick = gate.logical_tick;
            let paused =
                gate.paused_ordinal == u64::from(task.id.ordinal) && tick < gate.release_tick;
            if paused {
                gate.paused_steps = gate.paused_steps.saturating_add(1);
            } else {
                task.step(shared);
            }
            gate.logical_tick = tick.saturating_add(1);
        }
    }
    (gate.logical_tick, gate.paused_steps)
}

/// Run with the fixed guest gate page.
///
/// # Safety
/// The caller must run in the admitted guest whose page tables map this
/// physical address writable.
pub unsafe fn run(shared: &mut u64) -> (u64, u64) {
    // SAFETY: the caller guarantees the fixed page mapping.
    let gate = unsafe { gate() };
    run_with_gate(gate, shared)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(paused_ordinal: u64, release_tick: u64) -> PauseGate {
        PauseGate {
            magic: GATE_MAGIC,
            paused_ordinal,
            release_tick,
            logical_tick: 0,
            paused_steps: 0,
        }
    }

    #[test]
    fn control_interleaving_loses_an_update() {
        let mut gate = gate(u64::MAX, 0);
        let mut shared = 0;
        run_with_gate(&mut gate, &mut shared);
        assert_eq!(shared, 10);
    }

    #[test]
    fn pausing_the_second_task_changes_the_interleaving() {
        let mut gate = gate(1, 5);
        let mut shared = 0;
        let (ticks, paused) = run_with_gate(&mut gate, &mut shared);
        assert_eq!(shared, 11);
        assert!(ticks > 0);
        assert!(paused > 0);
    }
}
