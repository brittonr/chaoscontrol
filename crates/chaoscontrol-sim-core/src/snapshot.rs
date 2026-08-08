use crate::kernel::MAX_SIMULATION_VMS;
use crate::network::NetworkFabric;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const CORE_SNAPSHOT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationCoreSnapshot {
    pub schema_version: u16,
    pub tick: u64,
    pub vm_count: usize,
    pub network: NetworkFabric,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreSnapshotError {
    UnsupportedSchema {
        found: u16,
    },
    NoVirtualMachines,
    TooManyVirtualMachines {
        found: usize,
        maximum: usize,
    },
    NetworkVectorLength {
        field: &'static str,
        found: usize,
        expected: usize,
    },
    InvalidNetworkEndpoint {
        field: &'static str,
        endpoint: usize,
        vm_count: usize,
    },
}

impl fmt::Display for CoreSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CoreSnapshotError {}

impl SimulationCoreSnapshot {
    pub fn validate(&self) -> Result<(), CoreSnapshotError> {
        if self.schema_version != CORE_SNAPSHOT_SCHEMA_VERSION {
            return Err(CoreSnapshotError::UnsupportedSchema {
                found: self.schema_version,
            });
        }
        if self.vm_count == 0 {
            return Err(CoreSnapshotError::NoVirtualMachines);
        }
        if self.vm_count > MAX_SIMULATION_VMS {
            return Err(CoreSnapshotError::TooManyVirtualMachines {
                found: self.vm_count,
                maximum: MAX_SIMULATION_VMS,
            });
        }
        for (field, found) in [
            ("latency", self.network.latency.len()),
            (
                "latency_attempt_ids",
                self.network.latency_attempt_ids.len(),
            ),
            ("jitter", self.network.jitter.len()),
            ("jitter_attempt_ids", self.network.jitter_attempt_ids.len()),
            ("bandwidth_bps", self.network.bandwidth_bps.len()),
            (
                "bandwidth_attempt_ids",
                self.network.bandwidth_attempt_ids.len(),
            ),
            ("next_free_tick", self.network.next_free_tick.len()),
            ("loss_rate_ppm", self.network.loss_rate_ppm.len()),
            ("loss_attempt_ids", self.network.loss_attempt_ids.len()),
            (
                "corruption_rate_ppm",
                self.network.corruption_rate_ppm.len(),
            ),
            (
                "corruption_attempt_ids",
                self.network.corruption_attempt_ids.len(),
            ),
            ("reorder_window", self.network.reorder_window.len()),
            (
                "reorder_attempt_ids",
                self.network.reorder_attempt_ids.len(),
            ),
            ("duplicate_rate_ppm", self.network.duplicate_rate_ppm.len()),
            (
                "duplicate_attempt_ids",
                self.network.duplicate_attempt_ids.len(),
            ),
        ] {
            if found != self.vm_count {
                return Err(CoreSnapshotError::NetworkVectorLength {
                    field,
                    found,
                    expected: self.vm_count,
                });
            }
        }
        for message in &self.network.in_flight {
            validate_endpoint("in_flight.from", message.from, self.vm_count)?;
            validate_endpoint("in_flight.to", message.to, self.vm_count)?;
        }
        for packet in &self.network.packet_in_flight {
            validate_endpoint("packet_in_flight.from", packet.from, self.vm_count)?;
            validate_endpoint("packet_in_flight.to", packet.to, self.vm_count)?;
        }
        for (side_a, side_b) in &self.network.partitions {
            for endpoint in side_a {
                validate_endpoint("partitions.side_a", *endpoint, self.vm_count)?;
            }
            for endpoint in side_b {
                validate_endpoint("partitions.side_b", *endpoint, self.vm_count)?;
            }
        }
        Ok(())
    }
}

fn validate_endpoint(
    field: &'static str,
    endpoint: usize,
    vm_count: usize,
) -> Result<(), CoreSnapshotError> {
    if endpoint >= vm_count {
        return Err(CoreSnapshotError::InvalidNetworkEndpoint {
            field,
            endpoint,
            vm_count,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VM_COUNT: usize = 2;
    const TEST_SEED: u64 = 17;

    fn snapshot() -> SimulationCoreSnapshot {
        SimulationCoreSnapshot {
            schema_version: CORE_SNAPSHOT_SCHEMA_VERSION,
            tick: 0,
            vm_count: VM_COUNT,
            network: NetworkFabric::new(VM_COUNT, TEST_SEED),
        }
    }

    #[test]
    fn exact_network_snapshot_shape_is_accepted() {
        snapshot().validate().unwrap();
    }

    #[test]
    fn wrong_vector_length_and_endpoint_are_rejected() {
        let mut wrong_length = snapshot();
        wrong_length.network.latency.pop();
        assert!(matches!(
            wrong_length.validate(),
            Err(CoreSnapshotError::NetworkVectorLength {
                field: "latency",
                ..
            })
        ));

        let mut wrong_endpoint = snapshot();
        wrong_endpoint
            .network
            .in_flight
            .push(crate::network::NetworkMessage {
                from: VM_COUNT,
                to: 0,
                data: Vec::new(),
                deliver_at_tick: 0,
            });
        assert!(matches!(
            wrong_endpoint.validate(),
            Err(CoreSnapshotError::InvalidNetworkEndpoint {
                field: "in_flight.from",
                ..
            })
        ));
    }
}
