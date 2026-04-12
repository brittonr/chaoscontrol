//! Helical fault scenario generators.
//!
//! Named multi-phase fault generators that rotate failures around
//! the cluster. Each [`ScenarioFamily`] deterministically materializes
//! a concrete [`FaultSchedule`] and [`PhaseSummary`] from a
//! [`ScenarioConfig`] plus seed.

use crate::faults::Fault;
use crate::schedule::{FaultSchedule, FaultScheduleBuilder};
use serde::{Deserialize, Serialize};

/// Built-in helical scenario families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenarioFamily {
    /// Rotating network partitions and restarts.
    NetworkRing,
    /// Rotating `DiskFsyncLie` + kill/restart cycles.
    VolatileWriteRing,
    /// Rotating `DiskSlow` / `DiskPartialRead` with recovery windows.
    DegradedIoRing,
}

impl ScenarioFamily {
    /// Parse from CLI string.
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s {
            "network-ring" | "network_ring" | "NetworkRing" => Some(Self::NetworkRing),
            "volatile-write-ring" | "volatile_write_ring" | "VolatileWriteRing" => {
                Some(Self::VolatileWriteRing)
            }
            "degraded-io-ring" | "degraded_io_ring" | "DegradedIoRing" => {
                Some(Self::DegradedIoRing)
            }
            _ => None,
        }
    }

    /// Canonical kebab-case name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::NetworkRing => "network-ring",
            Self::VolatileWriteRing => "volatile-write-ring",
            Self::DegradedIoRing => "degraded-io-ring",
        }
    }

    /// All built-in families.
    pub const ALL: [ScenarioFamily; 3] = [
        Self::NetworkRing,
        Self::VolatileWriteRing,
        Self::DegradedIoRing,
    ];
}

impl std::fmt::Display for ScenarioFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Configuration for materializing a helical scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    /// Which scenario family to generate.
    pub family: ScenarioFamily,
    /// Number of VMs in the cluster.
    pub num_vms: usize,
    /// Duration of each phase in virtual nanoseconds.
    pub phase_ticks: u64,
    /// Number of helical turns (one turn = one target rotation).
    pub turns: usize,
}

impl ScenarioConfig {
    pub fn new(family: ScenarioFamily, num_vms: usize, phase_ticks: u64, turns: usize) -> Self {
        Self {
            family,
            num_vms,
            phase_ticks,
            turns,
        }
    }
}

/// Summary of a single phase within a materialized scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseEntry {
    /// Turn index (0-based).
    pub turn: usize,
    /// Primary target VM for this turn.
    pub target_vm: usize,
    /// Phase kind within the turn (e.g. "inject", "recovery").
    pub kind: String,
    /// Start time in virtual nanoseconds.
    pub start_ns: u64,
    /// End time in virtual nanoseconds.
    pub end_ns: u64,
    /// Short description of faults active in this phase.
    pub description: String,
}

/// Complete phase summary for a materialized scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseSummary {
    /// The scenario config that produced this summary.
    pub config: ScenarioConfig,
    /// Ordered list of phases.
    pub phases: Vec<PhaseEntry>,
    /// Total duration in virtual nanoseconds.
    pub total_duration_ns: u64,
}

/// Result of materializing a scenario: a concrete schedule + phase summary.
pub struct MaterializedScenario {
    /// The concrete fault schedule (ready for the engine).
    pub schedule: FaultSchedule,
    /// Human-readable phase summary.
    pub summary: PhaseSummary,
}

/// Materialize a scenario into a concrete `FaultSchedule`.
///
/// Deterministic: the same `(config, seed)` always produces the same output.
pub fn materialize(config: &ScenarioConfig, seed: u64) -> MaterializedScenario {
    match config.family {
        ScenarioFamily::NetworkRing => materialize_network_ring(config, seed),
        ScenarioFamily::VolatileWriteRing => materialize_volatile_write_ring(config, seed),
        ScenarioFamily::DegradedIoRing => materialize_degraded_io_ring(config, seed),
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  network-ring
// ═══════════════════════════════════════════════════════════════════════

/// Rotating partition + restart phases.
///
/// Each turn:
///   1. Partition the target VM from the rest
///   2. After half the phase window, kill it
///   3. After the full phase window, heal + restart
///   4. Recovery window before next turn
fn materialize_network_ring(config: &ScenarioConfig, _seed: u64) -> MaterializedScenario {
    let mut builder = FaultScheduleBuilder::new();
    let mut phases = Vec::new();
    let mut t = 0u64;
    let phase = config.phase_ticks;

    for turn in 0..config.turns {
        let target = turn % config.num_vms;
        let others: Vec<usize> = (0..config.num_vms).filter(|&v| v != target).collect();

        // Phase 1: partition target from rest
        let inject_start = t;
        builder = builder.at_ns_labeled(
            t,
            Fault::NetworkPartition {
                side_a: vec![target],
                side_b: others.clone(),
            },
            &format!("turn{}-partition-vm{}", turn, target),
        );

        // Half-phase: kill the target
        let kill_time = t + phase / 2;
        builder = builder.at_ns_labeled(
            kill_time,
            Fault::ProcessKill { target },
            &format!("turn{}-kill-vm{}", turn, target),
        );
        t += phase;
        let inject_end = t;

        phases.push(PhaseEntry {
            turn,
            target_vm: target,
            kind: "inject".into(),
            start_ns: inject_start,
            end_ns: inject_end,
            description: format!(
                "partition vm{} from {:?}, kill vm{} at +{}ns",
                target,
                others,
                target,
                phase / 2
            ),
        });

        // Phase 2: heal + restart + recovery window
        let recovery_start = t;
        builder = builder.at_ns_labeled(t, Fault::NetworkHeal, &format!("turn{}-heal", turn));
        builder = builder.at_ns_labeled(
            t,
            Fault::ProcessRestart { target },
            &format!("turn{}-restart-vm{}", turn, target),
        );
        t += phase;
        let recovery_end = t;

        phases.push(PhaseEntry {
            turn,
            target_vm: target,
            kind: "recovery".into(),
            start_ns: recovery_start,
            end_ns: recovery_end,
            description: format!("heal network, restart vm{}", target),
        });
    }

    MaterializedScenario {
        schedule: builder.build(),
        summary: PhaseSummary {
            config: config.clone(),
            phases,
            total_duration_ns: t,
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  volatile-write-ring
// ═══════════════════════════════════════════════════════════════════════

/// Rotating DiskFsyncLie + kill/restart.
///
/// Each turn:
///   1. Enable DiskFsyncLie on target (volatile writes accumulate)
///   2. After half the phase, kill the target (discards volatile buffer)
///   3. Restart the target
///   4. Recovery window (no new disk faults on previous target)
fn materialize_volatile_write_ring(config: &ScenarioConfig, _seed: u64) -> MaterializedScenario {
    let mut builder = FaultScheduleBuilder::new();
    let mut phases = Vec::new();
    let mut t = 0u64;
    let phase = config.phase_ticks;

    for turn in 0..config.turns {
        let target = turn % config.num_vms;

        // Phase 1: inject DiskFsyncLie + kill
        let inject_start = t;
        builder = builder.at_ns_labeled(
            t,
            Fault::DiskFsyncLie { target },
            &format!("turn{}-fsync-lie-vm{}", turn, target),
        );

        let kill_time = t + phase / 2;
        builder = builder.at_ns_labeled(
            kill_time,
            Fault::ProcessKill { target },
            &format!("turn{}-kill-vm{}", turn, target),
        );
        t += phase;
        let inject_end = t;

        phases.push(PhaseEntry {
            turn,
            target_vm: target,
            kind: "inject".into(),
            start_ns: inject_start,
            end_ns: inject_end,
            description: format!(
                "DiskFsyncLie on vm{}, kill vm{} at +{}ns",
                target,
                target,
                phase / 2
            ),
        });

        // Phase 2: restart + recovery
        let recovery_start = t;
        builder = builder.at_ns_labeled(
            t,
            Fault::ProcessRestart { target },
            &format!("turn{}-restart-vm{}", turn, target),
        );
        t += phase;
        let recovery_end = t;

        phases.push(PhaseEntry {
            turn,
            target_vm: target,
            kind: "recovery".into(),
            start_ns: recovery_start,
            end_ns: recovery_end,
            description: format!("restart vm{}, recovery window", target),
        });
    }

    MaterializedScenario {
        schedule: builder.build(),
        summary: PhaseSummary {
            config: config.clone(),
            phases,
            total_duration_ns: t,
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  degraded-io-ring
// ═══════════════════════════════════════════════════════════════════════

/// Rotating DiskSlow + ProcessRestart with recovery windows.
///
/// Each turn:
///   1. Inject DiskSlow on target + ProcessRestart at half-phase
///   2. Clear DiskSlow (delay_ns: 0) at phase end
///   3. Recovery window
fn materialize_degraded_io_ring(config: &ScenarioConfig, seed: u64) -> MaterializedScenario {
    use rand::SeedableRng;
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    let mut builder = FaultScheduleBuilder::new();
    let mut phases = Vec::new();
    let mut t = 0u64;
    let phase = config.phase_ticks;

    // Use seed to choose between DiskSlow and DiskPartialRead per turn
    use rand::Rng;

    for turn in 0..config.turns {
        let target = turn % config.num_vms;
        let use_partial_read: bool = rng.gen();

        // Phase 1: inject degraded I/O + restart pressure
        let inject_start = t;

        if use_partial_read {
            builder = builder.at_ns_labeled(
                t,
                Fault::DiskPartialRead {
                    target,
                    offset: 0,
                    max_bytes: 256,
                },
                &format!("turn{}-partial-read-vm{}", turn, target),
            );
        } else {
            builder = builder.at_ns_labeled(
                t,
                Fault::DiskSlow {
                    target,
                    delay_ns: 50_000_000, // 50ms per I/O
                },
                &format!("turn{}-disk-slow-vm{}", turn, target),
            );
        }

        // Mid-phase: restart the target for extra pressure
        let restart_time = t + phase / 2;
        builder = builder.at_ns_labeled(
            restart_time,
            Fault::ProcessRestart { target },
            &format!("turn{}-restart-vm{}", turn, target),
        );

        t += phase;
        let inject_end = t;

        // Clear DiskSlow at phase end (DiskPartialRead is one-shot, no clear needed)
        if !use_partial_read {
            builder = builder.at_ns_labeled(
                t,
                Fault::DiskSlow {
                    target,
                    delay_ns: 0,
                },
                &format!("turn{}-clear-slow-vm{}", turn, target),
            );
        }

        phases.push(PhaseEntry {
            turn,
            target_vm: target,
            kind: "inject".into(),
            start_ns: inject_start,
            end_ns: inject_end,
            description: format!(
                "{} on vm{}, restart vm{} at +{}ns",
                if use_partial_read {
                    "DiskPartialRead"
                } else {
                    "DiskSlow"
                },
                target,
                target,
                phase / 2
            ),
        });

        // Phase 2: recovery window (no new destructive disk faults for this target)
        let recovery_start = t;
        t += phase;
        let recovery_end = t;

        phases.push(PhaseEntry {
            turn,
            target_vm: target,
            kind: "recovery".into(),
            start_ns: recovery_start,
            end_ns: recovery_end,
            description: format!("recovery window for vm{}", target),
        });
    }

    MaterializedScenario {
        schedule: builder.build(),
        summary: PhaseSummary {
            config: config.clone(),
            phases,
            total_duration_ns: t,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_schedule() {
        let config = ScenarioConfig::new(ScenarioFamily::NetworkRing, 3, 1000, 6);
        let a = materialize(&config, 42);
        let b = materialize(&config, 42);

        assert_eq!(a.schedule.total(), b.schedule.total());
        assert_eq!(a.summary.phases.len(), b.summary.phases.len());

        for (fa, fb) in a.schedule.faults().iter().zip(b.schedule.faults()) {
            assert_eq!(fa.time_ns, fb.time_ns);
            assert_eq!(fa.fault, fb.fault);
        }
    }

    #[test]
    fn different_seed_different_schedule_degraded_io() {
        let config = ScenarioConfig::new(ScenarioFamily::DegradedIoRing, 3, 1000, 6);
        let a = materialize(&config, 42);
        let b = materialize(&config, 99);

        // degraded-io-ring uses RNG to pick DiskSlow vs DiskPartialRead,
        // so different seeds should produce different schedules
        let a_faults: Vec<_> = a
            .schedule
            .faults()
            .iter()
            .map(|f| format!("{}", f.fault))
            .collect();
        let b_faults: Vec<_> = b
            .schedule
            .faults()
            .iter()
            .map(|f| format!("{}", f.fault))
            .collect();
        // With high probability these differ; check at least structure matches
        assert_eq!(a.summary.phases.len(), b.summary.phases.len());
        // The actual faults should differ for at least one entry
        assert!(
            a_faults != b_faults,
            "Expected different schedules for different seeds"
        );
    }

    #[test]
    fn network_ring_rotates_targets() {
        let config = ScenarioConfig::new(ScenarioFamily::NetworkRing, 3, 1000, 3);
        let result = materialize(&config, 0);

        // 3 turns × 2 phases (inject + recovery) = 6 phases
        assert_eq!(result.summary.phases.len(), 6);

        // Inject phases should target VMs 0, 1, 2
        let targets: Vec<usize> = result
            .summary
            .phases
            .iter()
            .filter(|p| p.kind == "inject")
            .map(|p| p.target_vm)
            .collect();
        assert_eq!(targets, vec![0, 1, 2]);
    }

    #[test]
    fn volatile_write_ring_has_fsync_lie() {
        let config = ScenarioConfig::new(ScenarioFamily::VolatileWriteRing, 3, 1000, 3);
        let result = materialize(&config, 0);

        let has_fsync_lie = result
            .schedule
            .faults()
            .iter()
            .any(|f| matches!(f.fault, Fault::DiskFsyncLie { .. }));
        assert!(
            has_fsync_lie,
            "volatile-write-ring must include DiskFsyncLie"
        );

        let has_kill = result
            .schedule
            .faults()
            .iter()
            .any(|f| matches!(f.fault, Fault::ProcessKill { .. }));
        assert!(has_kill, "volatile-write-ring must include ProcessKill");
    }

    #[test]
    fn degraded_io_ring_has_disk_fault() {
        let config = ScenarioConfig::new(ScenarioFamily::DegradedIoRing, 3, 1000, 6);
        let result = materialize(&config, 42);

        let has_disk_fault = result.schedule.faults().iter().any(|f| {
            matches!(
                f.fault,
                Fault::DiskSlow { .. } | Fault::DiskPartialRead { .. }
            )
        });
        assert!(
            has_disk_fault,
            "degraded-io-ring must include DiskSlow or DiskPartialRead"
        );

        let has_restart = result
            .schedule
            .faults()
            .iter()
            .any(|f| matches!(f.fault, Fault::ProcessRestart { .. }));
        assert!(has_restart, "degraded-io-ring must include ProcessRestart");
    }

    #[test]
    fn recovery_windows_present() {
        for family in ScenarioFamily::ALL {
            let config = ScenarioConfig::new(family, 3, 1000, 3);
            let result = materialize(&config, 0);

            let recovery_count = result
                .summary
                .phases
                .iter()
                .filter(|p| p.kind == "recovery")
                .count();
            assert!(
                recovery_count >= 3,
                "{}: expected at least 3 recovery phases, got {}",
                family,
                recovery_count
            );
        }
    }

    #[test]
    fn scenario_family_parse() {
        assert_eq!(
            ScenarioFamily::from_str_loose("network-ring"),
            Some(ScenarioFamily::NetworkRing)
        );
        assert_eq!(
            ScenarioFamily::from_str_loose("volatile-write-ring"),
            Some(ScenarioFamily::VolatileWriteRing)
        );
        assert_eq!(
            ScenarioFamily::from_str_loose("degraded-io-ring"),
            Some(ScenarioFamily::DegradedIoRing)
        );
        assert_eq!(ScenarioFamily::from_str_loose("unknown"), None);
    }

    #[test]
    fn scenario_family_display() {
        assert_eq!(ScenarioFamily::NetworkRing.to_string(), "network-ring");
        assert_eq!(
            ScenarioFamily::VolatileWriteRing.to_string(),
            "volatile-write-ring"
        );
        assert_eq!(
            ScenarioFamily::DegradedIoRing.to_string(),
            "degraded-io-ring"
        );
    }

    #[test]
    fn scenario_config_serde_roundtrip() {
        let config = ScenarioConfig::new(ScenarioFamily::NetworkRing, 3, 1000, 6);
        let json = serde_json::to_string(&config).unwrap();
        let roundtrip: ScenarioConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.family, ScenarioFamily::NetworkRing);
        assert_eq!(roundtrip.num_vms, 3);
        assert_eq!(roundtrip.phase_ticks, 1000);
        assert_eq!(roundtrip.turns, 6);
    }

    #[test]
    fn phase_summary_serde_roundtrip() {
        let config = ScenarioConfig::new(ScenarioFamily::VolatileWriteRing, 3, 500, 3);
        let result = materialize(&config, 42);
        let json = serde_json::to_string(&result.summary).unwrap();
        let roundtrip: PhaseSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.phases.len(), result.summary.phases.len());
        assert_eq!(
            roundtrip.total_duration_ns,
            result.summary.total_duration_ns
        );
    }
}
