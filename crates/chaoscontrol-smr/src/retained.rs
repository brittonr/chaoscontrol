//! Retained-parent semantic replay and independently supplied observer readback.
//!
//! Shells retain bytes and execute the consumer. The core checks their identities
//! before handing admitted inputs to the consumer; it never invents execution.
use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedReplay {
    pub profile_ref: String,
    pub parent_state_ref: String,
    pub parent_state: Vec<u8>,
    pub commands: Vec<GeneratedCommand>,
    pub expected: SemanticRun,
}

pub fn parent_state_ref(bytes: &[u8]) -> String {
    domain_hash(b"chaoscontrol.smr-chain.parent-state.v1\0", &[bytes])
}

/// Restore a retained application parent and actually rerun the consumer.
/// `execute` must restore the provided bytes, not the host's current state.
pub fn replay_retained<F>(
    plan: &SmrWorkloadPlan,
    artifact: &RetainedReplay,
    execute: F,
) -> SmrResult<ReplayComparison>
where
    F: FnOnce(&[u8], &[GeneratedCommand]) -> SmrResult<SemanticRun>,
{
    if artifact.profile_ref != plan.profile_ref
        || artifact.parent_state_ref != parent_state_ref(&artifact.parent_state)
    {
        return Err(SmrError::new(
            "replay-parent",
            "retained parent bytes or profile identity differ",
        ));
    }
    let bounds = &plan.profile.bounds;
    let encoded = serde_json::to_vec(artifact)
        .map_err(|error| SmrError::new("replay-encoding", error.to_string()))?;
    if encoded.len() as u64 > bounds.maximum_evidence_bytes
        || artifact.commands.len() as u64 > bounds.maximum_commands
    {
        return Err(SmrError::new(
            "replay-bound",
            "retained replay inputs exceed admitted bounds",
        ));
    }
    let mut operations = BTreeMap::new();
    let mut clients = BTreeSet::new();
    for (offset, command) in artifact.commands.iter().enumerate() {
        validate_identifier("operation_id", &command.operation_id)?;
        validate_identifier("client_id", &command.client_id)?;
        if command.command_index != offset as u64 + MIN_COMMAND_INDEX
            || command.bytes.len() as u64 > bounds.maximum_command_bytes
            || operations
                .insert(command.operation_id.as_str(), command)
                .is_some()
            || command.command_ref
                != domain_hash(
                    b"chaoscontrol.smr-chain.command-ref.v1\0",
                    &[&command.bytes],
                )
        {
            return Err(SmrError::new(
                "replay-command",
                "retained command identity, position or bytes differ",
            ));
        }
        clients.insert(command.client_id.as_str());
    }
    if clients.len() as u64 > u64::from(bounds.maximum_clients) {
        return Err(SmrError::new(
            "replay-bound",
            "retained commands exceed the admitted client bound",
        ));
    }
    validate_semantic_run(plan, &artifact.expected)?;
    for attempt in &artifact.expected.proposal_outcomes {
        if !operations
            .get(attempt.operation_id.as_str())
            .is_some_and(|command| command.command_ref == attempt.command_ref)
        {
            return Err(SmrError::new(
                "replay-command",
                "retained proposal is not bound to the supplied command",
            ));
        }
    }
    for observation in &artifact.expected.observations {
        if observation
            .command_index
            .checked_sub(MIN_COMMAND_INDEX)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| artifact.commands.get(index))
            .is_none()
        {
            return Err(SmrError::new(
                "replay-command",
                "retained observation has no supplied command",
            ));
        }
    }
    let actual = execute(&artifact.parent_state, &artifact.commands)?;
    validate_semantic_run(plan, &actual)?;
    compare_replay_bounded(&artifact.expected, &actual, bounds)
}

pub fn validate_semantic_run(plan: &SmrWorkloadPlan, run: &SemanticRun) -> SmrResult<()> {
    validate_replay_event_bound("semantic", run, &plan.profile.bounds)?;
    validate_proposal_attempts(&run.proposal_outcomes, &plan.profile.bounds)?;
    let history = validate_history(plan, &run.observations)?;
    if evaluate_liveness(plan, run.stabilization_facts.as_ref())? != run.liveness {
        return Err(SmrError::new(
            "replay-liveness",
            "semantic liveness differs from retained preconditions",
        ));
    }
    let mut operations: BTreeSet<_> = run
        .proposal_outcomes
        .iter()
        .map(|attempt| attempt.operation_id.as_str())
        .collect();
    if run.operation_ids.len() != operations.len()
        || run
            .operation_ids
            .iter()
            .any(|id| !operations.remove(id.as_str()))
    {
        return Err(SmrError::new(
            "replay-operation",
            "semantic operation identities do not match the recorded proposals",
        ));
    }
    if run.safety_prefixes != history.safety_prefixes {
        return Err(SmrError::new(
            "replay-safety",
            "semantic safety prefixes differ from raw observations",
        ));
    }
    Ok(())
}

/// Compare observer output against a separate read of the consumer's committed
/// application path. Supplying expected chain digests is not readback evidence.
/// Shell conformance tests must supply the independent committed readback, even
/// when the adapter under test returns a perfectly framed fabricated digest.
pub fn validate_observer_readback(
    plan: &SmrWorkloadPlan,
    observed: &[ChainObservation],
    committed_readback: &[ChainObservation],
) -> SmrResult<()> {
    validate_history(plan, observed)?;
    validate_history(plan, committed_readback)?;
    if observed != committed_readback {
        return Err(SmrError::new(
            "observer-path",
            "observer differs from independent committed application readback",
        ));
    }
    Ok(())
}
