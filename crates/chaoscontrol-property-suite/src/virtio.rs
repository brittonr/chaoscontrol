// r[impl chaoscontrol.property_coverage.virtio_evidence]
use chaoscontrol_vmm::devices::virtio_types::MAX_QUEUE_SIZE;
use chaoscontrol_vmm::devices::virtio_validation::{
    validate_available_delta, validate_status_transition, VIRTIO_F_VERSION_1,
    VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DEVICE_NEEDS_RESET, VIRTIO_STATUS_DRIVER,
    VIRTIO_STATUS_DRIVER_OK, VIRTIO_STATUS_FAILED, VIRTIO_STATUS_FEATURES_OK,
};
use serde::{Deserialize, Serialize};

use crate::framework::{run_generated, DeterministicRng, Failure, PropertyProfile, SuiteReport};

const SUITE: &str = "virtio";
const COMMAND_VARIANTS: usize = 3;
const COMMAND_QUEUE_DELTA: usize = 0;
const COMMAND_STATUS: usize = 1;
const COMMAND_UNSUPPORTED_FEATURES: usize = 2;
const DELTA_RANGE_MULTIPLIER: u64 = 2;
const STATUS_CHOICES: usize = 7;
const STATUS_ACK_DRIVER: u32 = VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER;
const STATUS_FEATURES: u32 = STATUS_ACK_DRIVER | VIRTIO_STATUS_FEATURES_OK;
const STATUS_ACTIVE: u32 = STATUS_FEATURES | VIRTIO_STATUS_DRIVER_OK;
const KNOWN_DRIVER_BITS: u32 = VIRTIO_STATUS_ACKNOWLEDGE
    | VIRTIO_STATUS_DRIVER
    | VIRTIO_STATUS_DRIVER_OK
    | VIRTIO_STATUS_FEATURES_OK
    | VIRTIO_STATUS_FAILED;
const UNSUPPORTED_FEATURE_BIT: u32 = 9;
const UNSUPPORTED_FEATURE: u64 = 1 << UNSUPPORTED_FEATURE_BIT;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    QueueDelta { delta: u16 },
    Status { next: u32 },
    UnsupportedFeatures { next: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Model {
    last_available: u16,
    status: u32,
}

pub fn run(selected: &PropertyProfile) -> Result<SuiteReport, crate::AnyCounterexample> {
    run_generated(SUITE, selected, generate, check).map_err(crate::AnyCounterexample::virtio)
}

fn generate(rng: &mut DeterministicRng) -> Command {
    match rng.index(COMMAND_VARIANTS) {
        COMMAND_QUEUE_DELTA => {
            let upper = u64::from(MAX_QUEUE_SIZE) * DELTA_RANGE_MULTIPLIER + 1;
            Command::QueueDelta {
                delta: u16::try_from(rng.bounded_u64(upper))
                    .expect("bounded virtio delta must fit in u16"),
            }
        }
        COMMAND_STATUS => Command::Status {
            next: status_choice(rng.index(STATUS_CHOICES)),
        },
        COMMAND_UNSUPPORTED_FEATURES => Command::UnsupportedFeatures {
            next: status_choice(rng.index(STATUS_CHOICES)),
        },
        _ => unreachable!("bounded command selector must produce a known virtio command"),
    }
}

fn status_choice(index: usize) -> u32 {
    match index {
        0 => 0,
        1 => VIRTIO_STATUS_ACKNOWLEDGE,
        2 => STATUS_ACK_DRIVER,
        3 => STATUS_FEATURES,
        4 => STATUS_ACTIVE,
        5 => VIRTIO_STATUS_FAILED,
        6 => VIRTIO_STATUS_DEVICE_NEEDS_RESET,
        _ => unreachable!("bounded status selector must produce a known status"),
    }
}

fn check(commands: &[Command]) -> Result<usize, Failure> {
    let mut model = Model {
        last_available: 0,
        status: 0,
    };
    let mut actual_last_available = 0_u16;
    let mut actual_status = 0_u32;
    let mut rejected = 0_usize;

    for (step, command) in commands.iter().enumerate() {
        match command {
            Command::QueueDelta { delta } => {
                let available = actual_last_available.wrapping_add(*delta);
                let actual =
                    validate_available_delta(actual_last_available, available, MAX_QUEUE_SIZE);
                let expected_valid = *delta <= MAX_QUEUE_SIZE;
                if actual.is_ok() != expected_valid {
                    return Err(Failure::new(
                        "virtio-capacity-reference-agreement",
                        step,
                        format!("delta={delta}, actual={actual:?}"),
                    ));
                }
                if expected_valid {
                    actual_last_available = available;
                    model.last_available = model.last_available.wrapping_add(*delta);
                } else {
                    rejected += 1;
                }
            }
            Command::Status { next } | Command::UnsupportedFeatures { next } => {
                let driver_features = if matches!(command, Command::UnsupportedFeatures { .. }) {
                    VIRTIO_F_VERSION_1 | UNSUPPORTED_FEATURE
                } else {
                    VIRTIO_F_VERSION_1
                };
                let actual = validate_status_transition(
                    actual_status,
                    *next,
                    VIRTIO_F_VERSION_1,
                    driver_features,
                );
                let expected_valid = reference_status_transition(
                    model.status,
                    *next,
                    VIRTIO_F_VERSION_1,
                    driver_features,
                );
                if actual.is_ok() != expected_valid {
                    return Err(Failure::new(
                        "virtio-status-reference-agreement",
                        step,
                        format!(
                            "current={}, next={next}, driver_features={driver_features:#x}, actual={actual:?}",
                            model.status
                        ),
                    ));
                }
                if expected_valid {
                    actual_status = *next;
                    model.status = *next;
                } else {
                    rejected += 1;
                }
            }
        }
        if actual_last_available != model.last_available || actual_status != model.status {
            return Err(Failure::new(
                "virtio-invalid-command-no-mutation",
                step,
                format!(
                    "actual_available={actual_last_available}, model_available={}, actual_status={actual_status}, model_status={}",
                    model.last_available, model.status
                ),
            ));
        }
    }
    Ok(rejected)
}

fn reference_status_transition(
    current: u32,
    next: u32,
    offered_features: u64,
    driver_features: u64,
) -> bool {
    if next == 0 {
        return true;
    }
    if current & VIRTIO_STATUS_DEVICE_NEEDS_RESET != 0
        || next & !KNOWN_DRIVER_BITS != 0
        || next & current != current
    {
        return false;
    }
    let current_base = current & !VIRTIO_STATUS_FAILED;
    let next_base = next & !VIRTIO_STATUS_FAILED;
    let legal_order = [
        0,
        VIRTIO_STATUS_ACKNOWLEDGE,
        STATUS_ACK_DRIVER,
        STATUS_FEATURES,
        STATUS_ACTIVE,
    ];
    let Some(current_index) = legal_order
        .iter()
        .position(|status| *status == current_base)
    else {
        return false;
    };
    let Some(next_index) = legal_order.iter().position(|status| *status == next_base) else {
        return false;
    };
    if next_index != current_index && next_index != current_index + 1 {
        return false;
    }
    if next & VIRTIO_STATUS_FEATURES_OK != 0
        && (driver_features & !offered_features != 0 || driver_features & VIRTIO_F_VERSION_1 == 0)
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_capacity_and_status_regressions() {
        let commands: Vec<Command> = serde_json::from_str(include_str!(
            "../../../contracts/property-coverage/fixtures/regressions/virtio-capacity-status.json"
        ))
        .expect("the virtio regression fixture must be valid JSON");
        assert!(check(&commands).is_ok());
    }
}
