//! Controlled model of per-exit versus progress-based guest time.
//!
//! This is an evaluation fixture, not the VMM clock. It models a workload as
//! ordered phases of retired instructions and VM exits, advances a virtual TSC
//! under one of two policies, and records when a TSC-based timeout fires.
//! Runtime mode, PMU measurement, skid, and KVM behavior are out of scope.

/// How the virtual TSC advances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockMode {
    /// One fixed advance per VM exit; the current default.
    PerExit { advance_per_exit: u64 },
    /// Advance proportional to guest retired instructions; the evaluated mode.
    Counter { ticks_per_instruction: u64 },
}

/// One contiguous phase of guest behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Guest executes this many instructions without exiting.
    Work { instructions: u64 },
    /// Guest exits once after executing `work_before_exit` instructions.
    Exit { work_before_exit: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    pub phases: Vec<Phase>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeoutOutcome {
    pub fired: bool,
    /// Instruction index at which the timeout fired, when it did.
    pub fired_at_instruction: Option<u64>,
    /// Exit index at the firing boundary (0 for the first exit), when fired.
    pub fired_at_exit: Option<u64>,
    pub final_tsc: u64,
    pub instructions_total: u64,
    pub exits_total: u64,
}

impl Trace {
    pub fn instructions(&self) -> u64 {
        self.phases
            .iter()
            .map(|phase| match phase {
                Phase::Work { instructions } => *instructions,
                Phase::Exit { work_before_exit } => *work_before_exit,
            })
            .sum()
    }

    pub fn exits(&self) -> u64 {
        self.phases
            .iter()
            .filter(|phase| matches!(phase, Phase::Exit { .. }))
            .count() as u64
    }

    /// Run the trace under one clock policy and report when `timeout_ticks` fires.
    pub fn run(&self, mode: ClockMode, timeout_ticks: u64) -> TimeoutOutcome {
        let mut tsc = 0u64;
        let mut instruction_index = 0u64;
        let mut exit_index = 0u64;
        let mut fired = None;
        'phases: for phase in &self.phases {
            match *phase {
                Phase::Work { instructions } => {
                    for _ in 0..instructions {
                        instruction_index += 1;
                        if let ClockMode::Counter {
                            ticks_per_instruction,
                        } = mode
                        {
                            tsc = tsc.saturating_add(ticks_per_instruction);
                        }
                        if tsc >= timeout_ticks {
                            fired = Some((instruction_index, exit_index));
                            break 'phases;
                        }
                    }
                }
                Phase::Exit { work_before_exit } => {
                    instruction_index += work_before_exit;
                    if let ClockMode::Counter {
                        ticks_per_instruction,
                    } = mode
                    {
                        tsc = tsc
                            .saturating_add(work_before_exit.saturating_mul(ticks_per_instruction));
                    }
                    if tsc >= timeout_ticks {
                        fired = Some((instruction_index, exit_index));
                        break 'phases;
                    }
                    if let ClockMode::PerExit { advance_per_exit } = mode {
                        tsc = tsc.saturating_add(advance_per_exit);
                    }
                    exit_index += 1;
                    if tsc >= timeout_ticks {
                        fired = Some((instruction_index, exit_index));
                        break 'phases;
                    }
                }
            }
        }
        TimeoutOutcome {
            fired: fired.is_some(),
            fired_at_instruction: fired.map(|(instruction, _)| instruction),
            fired_at_exit: fired.map(|(_, exit)| exit),
            final_tsc: tsc,
            instructions_total: self.instructions(),
            exits_total: self.exits(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn busy_loop(instructions: u64, exits: u64, work_per_exit: u64) -> Trace {
        let mut phases = vec![Phase::Work { instructions }];
        for _ in 0..exits {
            phases.push(Phase::Exit {
                work_before_exit: work_per_exit,
            });
        }
        Trace { phases }
    }

    #[test]
    fn exit_clock_cannot_time_out_a_cpu_bound_loop() {
        let trace = busy_loop(10_000_000, 0, 0);
        let per_exit = trace.run(
            ClockMode::PerExit {
                advance_per_exit: 1_000,
            },
            1_000_000,
        );
        assert!(!per_exit.fired);
        assert_eq!(per_exit.final_tsc, 0);

        let counter = trace.run(
            ClockMode::Counter {
                ticks_per_instruction: 1,
            },
            1_000_000,
        );
        assert!(counter.fired);
        assert_eq!(counter.fired_at_instruction, Some(1_000_000));
        assert_eq!(counter.fired_at_exit, Some(0));
    }

    #[test]
    fn exit_clock_fires_on_exit_frequency_not_work() {
        let trace = busy_loop(100, 2_000, 10);
        let per_exit = trace.run(
            ClockMode::PerExit {
                advance_per_exit: 1_000,
            },
            1_000_000,
        );
        assert!(per_exit.fired);
        assert_eq!(per_exit.fired_at_exit, Some(1_000));

        let counter = trace.run(
            ClockMode::Counter {
                ticks_per_instruction: 1,
            },
            1_000_000,
        );
        assert!(!counter.fired);
    }

    #[test]
    fn counter_mode_tracks_retired_work_across_exits() {
        let trace = busy_loop(0, 4, 250_000);
        let outcome = trace.run(
            ClockMode::Counter {
                ticks_per_instruction: 2,
            },
            1_000_000,
        );
        assert!(outcome.fired);
        assert_eq!(outcome.fired_at_instruction, Some(500_000));
        assert_eq!(outcome.fired_at_exit, Some(1));
    }

    #[test]
    fn totals_are_stable() {
        let trace = busy_loop(10, 3, 5);
        assert_eq!(trace.instructions(), 25);
        assert_eq!(trace.exits(), 3);
    }
}
