//! Pure comparison for adapter-based protocol-simulation replay receipts.

use crate::protocol_receipt::{validate_protocol_simulation_receipt, ProtocolReceiptError};
use crate::protocol_simulation::ProtocolSimulationReceipt;

/// Ordered, bounded mismatch classes for protocol-simulation replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolReplayMismatchClass {
    Seed,
    FaultSchedule,
    Config,
    History,
    Output,
}

/// First mismatch between two otherwise valid protocol receipts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolReplayMismatch {
    pub class: ProtocolReplayMismatchClass,
}

/// Pure comparison result for two protocol-simulation receipts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolReplayComparison {
    pub matched: bool,
    pub mismatch: Option<ProtocolReplayMismatch>,
}

/// r[protocol-fault-sim.replay]
/// r[protocol-fault-sim.replay.reproduce]
/// r[protocol-fault-sim.replay.mismatch]
/// Compare two validated receipts and report only the first bounded class.
pub fn compare_protocol_simulation_receipts(
    left: &ProtocolSimulationReceipt,
    right: &ProtocolSimulationReceipt,
) -> Result<ProtocolReplayComparison, ProtocolReceiptError> {
    validate_protocol_simulation_receipt(left)?;
    validate_protocol_simulation_receipt(right)?;

    let mismatch = if left.config.seed != right.config.seed {
        Some(ProtocolReplayMismatchClass::Seed)
    } else if left.config.schedule != right.config.schedule
        || left.fault_schedule_digest != right.fault_schedule_digest
    {
        Some(ProtocolReplayMismatchClass::FaultSchedule)
    } else if left.config_digest != right.config_digest {
        Some(ProtocolReplayMismatchClass::Config)
    } else if left.history_digest != right.history_digest {
        Some(ProtocolReplayMismatchClass::History)
    } else if left.output_digest != right.output_digest {
        Some(ProtocolReplayMismatchClass::Output)
    } else {
        None
    };
    Ok(comparison_from_mismatch(mismatch))
}

fn comparison_from_mismatch(
    mismatch: Option<ProtocolReplayMismatchClass>,
) -> ProtocolReplayComparison {
    match mismatch {
        Some(class) => ProtocolReplayComparison {
            matched: false,
            mismatch: Some(ProtocolReplayMismatch { class }),
        },
        None => ProtocolReplayComparison {
            matched: true,
            mismatch: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_protocol_simulation_receipt, plan_protocol_fault, schedule_next_protocol_event,
        PendingProtocolEvent, ProtocolEventSchedulerState, ProtocolFaultContext,
        ProtocolFaultEffect, ProtocolFaultHook, ProtocolIdentity, ProtocolRngPolicy,
        ProtocolScheduleRef, ProtocolSchedulerPolicy, ProtocolSimulationConfig,
        ProtocolVirtualClockPolicy, ScheduledProtocolFault, PROTOCOL_SIMULATION_CONFIG_SCHEMA,
    };
    use std::collections::{BTreeMap, BTreeSet};

    const TEST_SEED: u64 = 53;
    const TEST_FAULT_TICK: u64 = 3;
    const TEST_FAULT_SEQUENCE: u64 = 0;
    const TEST_EVENT_SEQUENCE: u64 = 1;
    const TEST_MAXIMUM_STEPS: u64 = 8;
    const TEST_MAXIMUM_PARTITION_NODES: usize = 2;
    const TEST_MAXIMUM_ADDITIONAL_COPIES: u32 = 1;
    const TEST_DIGEST_HEX: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TEST_ALTERNATE_DIGEST_HEX: &str =
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const FAILURE_OUTPUT: &[u8] = b"failure:lease-renewal-blocked-by-partition";

    #[derive(serde::Serialize)]
    struct FailingFixtureHistory<'a> {
        seed: u64,
        schedule_id: &'a str,
        schedule_digest: &'a str,
        selected_sequence: u64,
        selected_target: &'a str,
        selected_event: &'a str,
        fault_effect: &'a ProtocolFaultEffect,
        failure: &'a str,
    }

    fn digest() -> String {
        format!("blake3:{TEST_DIGEST_HEX}")
    }

    fn config() -> ProtocolSimulationConfig {
        ProtocolSimulationConfig {
            schema: PROTOCOL_SIMULATION_CONFIG_SCHEMA.to_string(),
            seed: TEST_SEED,
            schedule: ProtocolScheduleRef {
                schedule_id: "lease-partition-failure".to_string(),
                digest: digest(),
            },
            scheduler: ProtocolSchedulerPolicy {
                policy_id: "deterministic-ready-order-v1".to_string(),
                maximum_steps: TEST_MAXIMUM_STEPS,
            },
            virtual_clock: ProtocolVirtualClockPolicy {
                policy_id: "simulation-ticks-v1".to_string(),
                initial_tick: TEST_FAULT_TICK,
                tick_quantum: 1,
            },
            rng: ProtocolRngPolicy {
                algorithm: "chacha20-v1".to_string(),
                seed_derivation: "config-seed-v1".to_string(),
            },
            protocol: ProtocolIdentity {
                protocol_id: "replicated-lease".to_string(),
                protocol_version: "v1".to_string(),
                adapter_id: "replicated-lease-fixture".to_string(),
                adapter_version: "v1".to_string(),
            },
            artifact_digests: BTreeMap::from([("adapter".to_string(), digest())]),
        }
    }

    fn run_failing_schedule(config: &ProtocolSimulationConfig) -> (Vec<u8>, Vec<u8>) {
        let fault_context = ProtocolFaultContext {
            current_tick: TEST_FAULT_TICK,
            expected_sequence: TEST_FAULT_SEQUENCE,
            known_nodes: BTreeSet::from(["node-a".to_string(), "node-b".to_string()]),
            known_messages: BTreeSet::from(["lease-renewal".to_string()]),
            maximum_partition_nodes: TEST_MAXIMUM_PARTITION_NODES,
            maximum_additional_copies: TEST_MAXIMUM_ADDITIONAL_COPIES,
        };
        let fault = ScheduledProtocolFault {
            sequence: TEST_FAULT_SEQUENCE,
            at_tick: TEST_FAULT_TICK,
            hook: ProtocolFaultHook::Partition {
                side_a: vec!["node-a".to_string()],
                side_b: vec!["node-b".to_string()],
            },
        };
        let fault_decision =
            plan_protocol_fault(&fault_context, &fault).expect("partition fault plans");
        assert_eq!(
            fault_decision.effect,
            ProtocolFaultEffect::PartitionLinks {
                side_a: vec!["node-a".to_string()],
                side_b: vec!["node-b".to_string()],
            }
        );

        let scheduler = ProtocolEventSchedulerState {
            next_step: 0,
            current_tick: TEST_FAULT_TICK,
            policy: config.scheduler.clone(),
            pending: vec![PendingProtocolEvent {
                sequence: TEST_EVENT_SEQUENCE,
                ready_tick: TEST_FAULT_TICK,
                target: "node-b".to_string(),
                event: "renew-lease".to_string(),
            }],
        };
        let schedule_decision =
            schedule_next_protocol_event(&scheduler).expect("lease renewal schedules");
        assert_eq!(schedule_decision.selected.sequence, TEST_EVENT_SEQUENCE);
        assert_eq!(schedule_decision.selected.target, "node-b");

        let history = FailingFixtureHistory {
            seed: config.seed,
            schedule_id: &config.schedule.schedule_id,
            schedule_digest: &config.schedule.digest,
            selected_sequence: schedule_decision.selected.sequence,
            selected_target: &schedule_decision.selected.target,
            selected_event: &schedule_decision.selected.event,
            fault_effect: &fault_decision.effect,
            failure: "lease-renewal-blocked-by-partition",
        };
        let history_bytes = serde_json::to_vec(&history).expect("fixture history serializes");
        (history_bytes, FAILURE_OUTPUT.to_vec())
    }

    #[test]
    fn one_failing_seed_and_schedule_reproduce_matching_receipts() {
        let config = config();
        let (first_history, first_output) = run_failing_schedule(&config);
        let (second_history, second_output) = run_failing_schedule(&config);
        assert_eq!(first_history, second_history);
        assert_eq!(first_output, second_output);
        assert_eq!(first_output, FAILURE_OUTPUT);

        let first =
            build_protocol_simulation_receipt(config.clone(), &first_history, &first_output)
                .expect("first failing receipt builds");
        let second = build_protocol_simulation_receipt(config, &second_history, &second_output)
            .expect("second failing receipt builds");
        assert_eq!(first, second);
        assert_eq!(
            compare_protocol_simulation_receipts(&first, &second),
            Ok(ProtocolReplayComparison {
                matched: true,
                mismatch: None,
            })
        );
    }

    #[test]
    fn every_receipt_difference_uses_the_stable_mismatch_order() {
        let base_config = config();
        let (history, output) = run_failing_schedule(&base_config);
        let base = build_protocol_simulation_receipt(base_config.clone(), &history, &output)
            .expect("base receipt builds");

        let mut changed_seed = base_config.clone();
        changed_seed.seed = changed_seed
            .seed
            .checked_add(1)
            .expect("seed remains bounded");
        let seed_receipt = build_protocol_simulation_receipt(changed_seed, &history, &output)
            .expect("seed receipt builds");

        let mut changed_schedule = base_config.clone();
        changed_schedule.schedule.digest = format!("blake3:{TEST_ALTERNATE_DIGEST_HEX}");
        let schedule_receipt =
            build_protocol_simulation_receipt(changed_schedule, &history, &output)
                .expect("schedule receipt builds");

        let mut changed_policy = base_config.clone();
        changed_policy.scheduler.policy_id = "deterministic-ready-order-v2".to_string();
        let config_receipt = build_protocol_simulation_receipt(changed_policy, &history, &output)
            .expect("config receipt builds");

        let mut changed_history = history.clone();
        changed_history.extend_from_slice(b"-divergent");
        let history_receipt =
            build_protocol_simulation_receipt(base_config.clone(), &changed_history, &output)
                .expect("history receipt builds");

        let mut changed_output = output.clone();
        changed_output.extend_from_slice(b"-divergent");
        let output_receipt =
            build_protocol_simulation_receipt(base_config, &history, &changed_output)
                .expect("output receipt builds");

        for (receipt, expected_class) in [
            (seed_receipt, ProtocolReplayMismatchClass::Seed),
            (schedule_receipt, ProtocolReplayMismatchClass::FaultSchedule),
            (config_receipt, ProtocolReplayMismatchClass::Config),
            (history_receipt, ProtocolReplayMismatchClass::History),
            (output_receipt, ProtocolReplayMismatchClass::Output),
        ] {
            let comparison = compare_protocol_simulation_receipts(&base, &receipt)
                .expect("valid receipts compare");
            assert_eq!(
                comparison,
                ProtocolReplayComparison {
                    matched: false,
                    mismatch: Some(ProtocolReplayMismatch {
                        class: expected_class,
                    }),
                }
            );
        }
    }

    #[test]
    fn divergent_failing_history_reports_the_first_bounded_class() {
        let config = config();
        let (history, output) = run_failing_schedule(&config);
        let left = build_protocol_simulation_receipt(config.clone(), &history, &output)
            .expect("left receipt builds");
        let mut divergent_history = history;
        divergent_history.extend_from_slice(b"-divergent");
        let right = build_protocol_simulation_receipt(config, &divergent_history, &output)
            .expect("right receipt builds");

        assert_eq!(
            compare_protocol_simulation_receipts(&left, &right),
            Ok(ProtocolReplayComparison {
                matched: false,
                mismatch: Some(ProtocolReplayMismatch {
                    class: ProtocolReplayMismatchClass::History,
                }),
            })
        );
    }
}
