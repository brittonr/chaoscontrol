use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};

use super::{checked_usize, valid_identifier};

const SCHEDULE_SCHEMA: &str = "chaoscontrol.fault-schedule-profile.v1";
const SCHEDULE_SCOPE: &str =
    "finite pre-run fault intent; not attempted, applied, observed, or effective fault evidence";
const BLAKE3_PREFIX: &str = "blake3:";
const DIGEST_HEX_LENGTH: usize = 64;
const MAX_FAULTS: usize = 4096;
const MAX_VMS: u64 = 64;
const MAX_RATE_PPM: u32 = 1_000_000;
const MAX_CLOCK_SKEW_NS: i64 = 86_400_000_000_000;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FaultScheduleProfile {
    pub schema: String,
    pub schedule_id: String,
    pub source_identity: String,
    pub num_vms: u64,
    pub faults: Vec<FaultDescriptor>,
    pub scope: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FaultDescriptor {
    ProcessKill {
        time_ns: u64,
        label: Option<String>,
        target: u64,
    },
    NetworkPartition {
        time_ns: u64,
        label: Option<String>,
        side_a: Vec<u64>,
        side_b: Vec<u64>,
    },
    NetworkLatency {
        time_ns: u64,
        label: Option<String>,
        target: u64,
        latency_ns: u64,
    },
    PacketLoss {
        time_ns: u64,
        label: Option<String>,
        target: u64,
        rate_ppm: u32,
    },
    DiskWriteError {
        time_ns: u64,
        label: Option<String>,
        target: u64,
        offset: u64,
    },
    ClockSkew {
        time_ns: u64,
        label: Option<String>,
        target: u64,
        offset_ns: i64,
    },
}

impl FaultDescriptor {
    fn time_ns(&self) -> u64 {
        match self {
            Self::ProcessKill { time_ns, .. }
            | Self::NetworkPartition { time_ns, .. }
            | Self::NetworkLatency { time_ns, .. }
            | Self::PacketLoss { time_ns, .. }
            | Self::DiskWriteError { time_ns, .. }
            | Self::ClockSkew { time_ns, .. } => *time_ns,
        }
    }

    fn label(&self) -> Option<&str> {
        match self {
            Self::ProcessKill { label, .. }
            | Self::NetworkPartition { label, .. }
            | Self::NetworkLatency { label, .. }
            | Self::PacketLoss { label, .. }
            | Self::DiskWriteError { label, .. }
            | Self::ClockSkew { label, .. } => label.as_deref(),
        }
    }

    fn validate(&self, num_vms: u64) -> Result<(), String> {
        if self.label().is_some_and(str::is_empty) {
            return Err("fault schedule label must be non-empty when present".to_string());
        }
        match self {
            Self::NetworkPartition { side_a, side_b, .. } => {
                let a = side_a
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>();
                let b = side_b
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>();
                if side_a.is_empty()
                    || side_b.is_empty()
                    || a.len() != side_a.len()
                    || b.len() != side_b.len()
                    || !a.is_disjoint(&b)
                    || a.iter().chain(&b).any(|target| *target >= num_vms)
                {
                    return Err("fault schedule partition is invalid".to_string());
                }
            }
            Self::PacketLoss {
                target, rate_ppm, ..
            } => {
                validate_target(*target, num_vms)?;
                if *rate_ppm == 0 || *rate_ppm > MAX_RATE_PPM {
                    return Err("fault schedule packet-loss rate is invalid".to_string());
                }
            }
            Self::NetworkLatency {
                target, latency_ns, ..
            } => {
                validate_target(*target, num_vms)?;
                if *latency_ns == 0 {
                    return Err("fault schedule latency must be positive".to_string());
                }
            }
            Self::ProcessKill { target, .. } | Self::DiskWriteError { target, .. } => {
                validate_target(*target, num_vms)?
            }
            Self::ClockSkew {
                target, offset_ns, ..
            } => {
                validate_target(*target, num_vms)?;
                if !(-MAX_CLOCK_SKEW_NS..=MAX_CLOCK_SKEW_NS).contains(offset_ns) {
                    return Err("fault schedule clock skew is out of range".to_string());
                }
            }
        }
        Ok(())
    }

    fn to_scheduled_fault(&self) -> Result<ScheduledFault, String> {
        let fault = match self {
            Self::ProcessKill { target, .. } => Fault::ProcessKill {
                target: checked_usize("fault target", *target)?,
            },
            Self::NetworkPartition { side_a, side_b, .. } => Fault::NetworkPartition {
                side_a: convert_targets(side_a)?,
                side_b: convert_targets(side_b)?,
            },
            Self::NetworkLatency {
                target, latency_ns, ..
            } => Fault::NetworkLatency {
                target: checked_usize("fault target", *target)?,
                latency_ns: *latency_ns,
            },
            Self::PacketLoss {
                target, rate_ppm, ..
            } => Fault::PacketLoss {
                target: checked_usize("fault target", *target)?,
                rate_ppm: *rate_ppm,
            },
            Self::DiskWriteError { target, offset, .. } => Fault::DiskWriteError {
                target: checked_usize("fault target", *target)?,
                offset: *offset,
            },
            Self::ClockSkew {
                target, offset_ns, ..
            } => Fault::ClockSkew {
                target: checked_usize("fault target", *target)?,
                offset_ns: *offset_ns,
            },
        };
        let mut scheduled = ScheduledFault::new(self.time_ns(), fault);
        if let Some(label) = self.label() {
            scheduled = scheduled.with_label(label);
        }
        Ok(scheduled)
    }
}

impl FaultScheduleProfile {
    pub fn try_into_schedule(self) -> Result<FaultSchedule, String> {
        self.validate()?;
        let mut schedule = FaultSchedule::new();
        for descriptor in &self.faults {
            schedule.add(descriptor.to_scheduled_fault()?);
        }
        Ok(schedule)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEDULE_SCHEMA
            || self.scope != SCHEDULE_SCOPE
            || self.num_vms == 0
            || self.num_vms > MAX_VMS
            || self.faults.len() > MAX_FAULTS
            || !valid_identifier(&self.schedule_id)
            || !valid_blake3(&self.source_identity)
        {
            return Err("fault schedule profile header or bounds are invalid".to_string());
        }
        let mut previous = None;
        for descriptor in &self.faults {
            descriptor.validate(self.num_vms)?;
            if previous.is_some_and(|time| time >= descriptor.time_ns()) {
                return Err("fault schedule times must be strictly ordered".to_string());
            }
            previous = Some(descriptor.time_ns());
        }
        Ok(())
    }
}

fn validate_target(target: u64, num_vms: u64) -> Result<(), String> {
    if target >= num_vms {
        return Err("fault schedule target is outside topology".to_string());
    }
    Ok(())
}

fn convert_targets(targets: &[u64]) -> Result<Vec<usize>, String> {
    targets
        .iter()
        .map(|target| checked_usize("fault target", *target))
        .collect()
}

fn valid_blake3(value: &str) -> bool {
    let Some(hex) = value.strip_prefix(BLAKE3_PREFIX) else {
        return false;
    };
    hex.len() == DIGEST_HEX_LENGTH
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
