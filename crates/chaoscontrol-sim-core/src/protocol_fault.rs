//! Pure protocol-specific fault hook planning.
//!
//! This module validates scheduled protocol faults and returns typed effects.
//! It does not apply process, network, clock, or transport effects.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// r[protocol-fault-sim.faults]
/// One explicit protocol fault hook from the admitted schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolFaultHook {
    NodeLoss {
        node_id: String,
    },
    MessageLoss {
        message_id: String,
    },
    MessageReorder {
        message_id: String,
        release_tick: u64,
    },
    MessageDuplication {
        message_id: String,
        additional_copies: u32,
    },
    Partition {
        side_a: Vec<String>,
        side_b: Vec<String>,
    },
}

/// One fault hook bound to its exact schedule position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledProtocolFault {
    pub sequence: u64,
    pub at_tick: u64,
    pub hook: ProtocolFaultHook,
}

/// Supplied protocol facts used for pure fault admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolFaultContext {
    pub current_tick: u64,
    pub expected_sequence: u64,
    pub known_nodes: BTreeSet<String>,
    pub known_messages: BTreeSet<String>,
    pub maximum_partition_nodes: usize,
    pub maximum_additional_copies: u32,
}

/// Exact effect that a shell or adapter can apply after pure admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolFaultEffect {
    MarkNodeLost {
        node_id: String,
    },
    DropMessage {
        message_id: String,
    },
    DelayMessage {
        message_id: String,
        release_tick: u64,
    },
    DuplicateMessage {
        message_id: String,
        additional_copies: u32,
    },
    PartitionLinks {
        side_a: Vec<String>,
        side_b: Vec<String>,
    },
}

/// Pure result of one admitted protocol fault hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolFaultDecision {
    pub effect: ProtocolFaultEffect,
    pub next_expected_sequence: u64,
}

/// Fail-closed protocol fault admission error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolFaultError {
    InvalidPartitionNodeLimit,
    InvalidAdditionalCopyLimit,
    SequenceMismatch {
        expected: u64,
        found: u64,
    },
    TickMismatch {
        expected: u64,
        found: u64,
    },
    UnknownNode {
        node_id: String,
    },
    UnknownMessage {
        message_id: String,
    },
    ReorderDoesNotAdvanceTime {
        current_tick: u64,
        release_tick: u64,
    },
    InvalidAdditionalCopies {
        found: u32,
        maximum: u32,
    },
    EmptyPartitionSide {
        side: &'static str,
    },
    DuplicatePartitionNode {
        side: &'static str,
        node_id: String,
    },
    PartitionOverlap {
        node_id: String,
    },
    PartitionNodeLimitExceeded {
        found: usize,
        maximum: usize,
    },
    SequenceExhausted,
}

impl fmt::Display for ProtocolFaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ProtocolFaultError {}

/// Validate one scheduled hook and return its normalized protocol effect.
pub fn plan_protocol_fault(
    context: &ProtocolFaultContext,
    scheduled: &ScheduledProtocolFault,
) -> Result<ProtocolFaultDecision, ProtocolFaultError> {
    validate_fault_context(context)?;
    if scheduled.sequence != context.expected_sequence {
        return Err(ProtocolFaultError::SequenceMismatch {
            expected: context.expected_sequence,
            found: scheduled.sequence,
        });
    }
    if scheduled.at_tick != context.current_tick {
        return Err(ProtocolFaultError::TickMismatch {
            expected: context.current_tick,
            found: scheduled.at_tick,
        });
    }

    let effect = plan_fault_effect(context, &scheduled.hook)?;
    let next_expected_sequence = context
        .expected_sequence
        .checked_add(1)
        .ok_or(ProtocolFaultError::SequenceExhausted)?;
    Ok(ProtocolFaultDecision {
        effect,
        next_expected_sequence,
    })
}

fn validate_fault_context(context: &ProtocolFaultContext) -> Result<(), ProtocolFaultError> {
    if context.maximum_partition_nodes == 0 {
        return Err(ProtocolFaultError::InvalidPartitionNodeLimit);
    }
    if context.maximum_additional_copies == 0 {
        return Err(ProtocolFaultError::InvalidAdditionalCopyLimit);
    }
    Ok(())
}

fn plan_fault_effect(
    context: &ProtocolFaultContext,
    hook: &ProtocolFaultHook,
) -> Result<ProtocolFaultEffect, ProtocolFaultError> {
    match hook {
        ProtocolFaultHook::NodeLoss { node_id } => {
            require_known_node(context, node_id)?;
            Ok(ProtocolFaultEffect::MarkNodeLost {
                node_id: node_id.clone(),
            })
        }
        ProtocolFaultHook::MessageLoss { message_id } => {
            require_known_message(context, message_id)?;
            Ok(ProtocolFaultEffect::DropMessage {
                message_id: message_id.clone(),
            })
        }
        ProtocolFaultHook::MessageReorder {
            message_id,
            release_tick,
        } => {
            require_known_message(context, message_id)?;
            if *release_tick <= context.current_tick {
                return Err(ProtocolFaultError::ReorderDoesNotAdvanceTime {
                    current_tick: context.current_tick,
                    release_tick: *release_tick,
                });
            }
            Ok(ProtocolFaultEffect::DelayMessage {
                message_id: message_id.clone(),
                release_tick: *release_tick,
            })
        }
        ProtocolFaultHook::MessageDuplication {
            message_id,
            additional_copies,
        } => {
            require_known_message(context, message_id)?;
            if *additional_copies == 0 || *additional_copies > context.maximum_additional_copies {
                return Err(ProtocolFaultError::InvalidAdditionalCopies {
                    found: *additional_copies,
                    maximum: context.maximum_additional_copies,
                });
            }
            Ok(ProtocolFaultEffect::DuplicateMessage {
                message_id: message_id.clone(),
                additional_copies: *additional_copies,
            })
        }
        ProtocolFaultHook::Partition { side_a, side_b } => plan_partition(context, side_a, side_b),
    }
}

fn require_known_node(
    context: &ProtocolFaultContext,
    node_id: &str,
) -> Result<(), ProtocolFaultError> {
    if !context.known_nodes.contains(node_id) {
        return Err(ProtocolFaultError::UnknownNode {
            node_id: node_id.to_string(),
        });
    }
    Ok(())
}

fn require_known_message(
    context: &ProtocolFaultContext,
    message_id: &str,
) -> Result<(), ProtocolFaultError> {
    if !context.known_messages.contains(message_id) {
        return Err(ProtocolFaultError::UnknownMessage {
            message_id: message_id.to_string(),
        });
    }
    Ok(())
}

fn plan_partition(
    context: &ProtocolFaultContext,
    side_a: &[String],
    side_b: &[String],
) -> Result<ProtocolFaultEffect, ProtocolFaultError> {
    if side_a.is_empty() {
        return Err(ProtocolFaultError::EmptyPartitionSide { side: "side_a" });
    }
    if side_b.is_empty() {
        return Err(ProtocolFaultError::EmptyPartitionSide { side: "side_b" });
    }
    let found = side_a.len().checked_add(side_b.len()).ok_or(
        ProtocolFaultError::PartitionNodeLimitExceeded {
            found: usize::MAX,
            maximum: context.maximum_partition_nodes,
        },
    )?;
    if found > context.maximum_partition_nodes {
        return Err(ProtocolFaultError::PartitionNodeLimitExceeded {
            found,
            maximum: context.maximum_partition_nodes,
        });
    }

    let normalized_a = normalized_partition_side(context, "side_a", side_a)?;
    let normalized_b = normalized_partition_side(context, "side_b", side_b)?;
    if let Some(node_id) = normalized_a.intersection(&normalized_b).next() {
        return Err(ProtocolFaultError::PartitionOverlap {
            node_id: node_id.clone(),
        });
    }
    Ok(ProtocolFaultEffect::PartitionLinks {
        side_a: normalized_a.into_iter().collect(),
        side_b: normalized_b.into_iter().collect(),
    })
}

fn normalized_partition_side(
    context: &ProtocolFaultContext,
    side_name: &'static str,
    nodes: &[String],
) -> Result<BTreeSet<String>, ProtocolFaultError> {
    let mut normalized = BTreeSet::new();
    for node_id in nodes {
        require_known_node(context, node_id)?;
        if !normalized.insert(node_id.clone()) {
            return Err(ProtocolFaultError::DuplicatePartitionNode {
                side: side_name,
                node_id: node_id.clone(),
            });
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TICK: u64 = 13;
    const TEST_SEQUENCE: u64 = 7;
    const TEST_RELEASE_TICK: u64 = 17;
    const TEST_MAXIMUM_PARTITION_NODES: usize = 3;
    const TEST_MAXIMUM_ADDITIONAL_COPIES: u32 = 2;
    const TEST_ADDITIONAL_COPIES: u32 = 1;
    const TEST_HOOK_COUNT: usize = 5;

    fn context() -> ProtocolFaultContext {
        ProtocolFaultContext {
            current_tick: TEST_TICK,
            expected_sequence: TEST_SEQUENCE,
            known_nodes: BTreeSet::from([
                "node-a".to_string(),
                "node-b".to_string(),
                "node-c".to_string(),
            ]),
            known_messages: BTreeSet::from(["message-a".to_string(), "message-b".to_string()]),
            maximum_partition_nodes: TEST_MAXIMUM_PARTITION_NODES,
            maximum_additional_copies: TEST_MAXIMUM_ADDITIONAL_COPIES,
        }
    }

    fn scheduled(hook: ProtocolFaultHook) -> ScheduledProtocolFault {
        ScheduledProtocolFault {
            sequence: TEST_SEQUENCE,
            at_tick: TEST_TICK,
            hook,
        }
    }

    #[test]
    fn all_protocol_fault_hooks_plan_exact_normalized_effects() {
        let cases = [
            (
                ProtocolFaultHook::NodeLoss {
                    node_id: "node-a".to_string(),
                },
                ProtocolFaultEffect::MarkNodeLost {
                    node_id: "node-a".to_string(),
                },
            ),
            (
                ProtocolFaultHook::MessageLoss {
                    message_id: "message-a".to_string(),
                },
                ProtocolFaultEffect::DropMessage {
                    message_id: "message-a".to_string(),
                },
            ),
            (
                ProtocolFaultHook::MessageReorder {
                    message_id: "message-a".to_string(),
                    release_tick: TEST_RELEASE_TICK,
                },
                ProtocolFaultEffect::DelayMessage {
                    message_id: "message-a".to_string(),
                    release_tick: TEST_RELEASE_TICK,
                },
            ),
            (
                ProtocolFaultHook::MessageDuplication {
                    message_id: "message-b".to_string(),
                    additional_copies: TEST_ADDITIONAL_COPIES,
                },
                ProtocolFaultEffect::DuplicateMessage {
                    message_id: "message-b".to_string(),
                    additional_copies: TEST_ADDITIONAL_COPIES,
                },
            ),
            (
                ProtocolFaultHook::Partition {
                    side_a: vec!["node-b".to_string(), "node-a".to_string()],
                    side_b: vec!["node-c".to_string()],
                },
                ProtocolFaultEffect::PartitionLinks {
                    side_a: vec!["node-a".to_string(), "node-b".to_string()],
                    side_b: vec!["node-c".to_string()],
                },
            ),
        ];
        assert_eq!(cases.len(), TEST_HOOK_COUNT);
        for (hook, expected) in cases {
            let first = plan_protocol_fault(&context(), &scheduled(hook.clone()))
                .expect("protocol fault plans");
            let second = plan_protocol_fault(&context(), &scheduled(hook))
                .expect("same protocol fault plans again");
            assert_eq!(first, second);
            assert_eq!(first.effect, expected);
            assert_eq!(first.next_expected_sequence, TEST_SEQUENCE + 1);
        }
    }

    #[test]
    fn unknown_and_malformed_fault_hooks_fail_closed() {
        let unknown_node = scheduled(ProtocolFaultHook::NodeLoss {
            node_id: "node-missing".to_string(),
        });
        assert!(matches!(
            plan_protocol_fault(&context(), &unknown_node),
            Err(ProtocolFaultError::UnknownNode { .. })
        ));

        let unknown_message = scheduled(ProtocolFaultHook::MessageLoss {
            message_id: "message-missing".to_string(),
        });
        assert!(matches!(
            plan_protocol_fault(&context(), &unknown_message),
            Err(ProtocolFaultError::UnknownMessage { .. })
        ));

        let overlap = scheduled(ProtocolFaultHook::Partition {
            side_a: vec!["node-a".to_string()],
            side_b: vec!["node-a".to_string()],
        });
        assert_eq!(
            plan_protocol_fault(&context(), &overlap),
            Err(ProtocolFaultError::PartitionOverlap {
                node_id: "node-a".to_string()
            })
        );

        let invalid_copies = scheduled(ProtocolFaultHook::MessageDuplication {
            message_id: "message-a".to_string(),
            additional_copies: 0,
        });
        assert_eq!(
            plan_protocol_fault(&context(), &invalid_copies),
            Err(ProtocolFaultError::InvalidAdditionalCopies {
                found: 0,
                maximum: TEST_MAXIMUM_ADDITIONAL_COPIES
            })
        );

        let mut wrong_tick = scheduled(ProtocolFaultHook::MessageLoss {
            message_id: "message-a".to_string(),
        });
        wrong_tick.at_tick = TEST_TICK + 1;
        assert_eq!(
            plan_protocol_fault(&context(), &wrong_tick),
            Err(ProtocolFaultError::TickMismatch {
                expected: TEST_TICK,
                found: TEST_TICK + 1
            })
        );
    }
}
