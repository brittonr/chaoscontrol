use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlExecution {
    pub control_id: String,
    pub profile_ref: String,
    pub build_ref: String,
    pub seed: u64,
    pub commands_ref: String,
    pub run_ref: String,
    pub run: SemanticRun,
    pub fault_outcomes: Vec<FaultOutcome>,
}

pub fn campaign_commands_ref(commands: &[GeneratedCommand]) -> SmrResult<String> {
    canonical_value_ref(b"chaoscontrol.smr-chain.campaign-commands.v1\0", &commands)
}
pub fn semantic_run_ref(run: &SemanticRun) -> SmrResult<String> {
    canonical_value_ref(b"chaoscontrol.smr-chain.semantic-run.v1\0", run)
}

pub fn validate_control(
    plan: &SmrWorkloadPlan,
    campaign: &FaultCampaignProfile,
    seed: u64,
    build_ref: &str,
    control: &ControlExecution,
) -> SmrResult<()> {
    let generated = plan_campaign(plan, campaign, seed)?;
    if control.control_id != NO_FAULT_CONTROL_ID
        || control.profile_ref != plan.profile_ref
        || control.build_ref != build_ref
        || control.seed != seed
        || !control.fault_outcomes.is_empty()
        || control.commands_ref != campaign_commands_ref(&generated.commands)?
        || control.run_ref != semantic_run_ref(&control.run)?
    {
        return Err(SmrError::new(
            "fault-control-binding",
            "executed no-fault control identities or inputs differ",
        ));
    }
    retained::validate_semantic_run(plan, &control.run)?;
    let expected_operations: Vec<_> = generated
        .commands
        .iter()
        .map(|command| command.operation_id.clone())
        .collect();
    if control.run.operation_ids != expected_operations
        || control.run.terminal_status != TerminalStatus::Completed
        || control.run.observations.is_empty()
        || control
            .run
            .safety_prefixes
            .iter()
            .any(|verdict| *verdict != SafetyVerdict::Pass)
    {
        return Err(SmrError::new(
            "fault-control-execution",
            "no-fault control did not execute the planned operations safely",
        ));
    }
    for command in &generated.commands {
        if !control.run.proposal_outcomes.iter().any(|attempt| {
            attempt.operation_id == command.operation_id
                && attempt.command_ref == command.command_ref
                && attempt.outcome == ProposalOutcome::Acknowledged
        }) || !control
            .run
            .observations
            .iter()
            .any(|observation| observation.command_index == command.command_index)
        {
            return Err(SmrError::new(
                "fault-control-execution",
                "no-fault control lacks an acknowledged committed command",
            ));
        }
    }
    for observation in &control.run.observations {
        let command = generated
            .commands
            .get(observation.command_index.saturating_sub(1) as usize)
            .ok_or_else(|| {
                SmrError::new(
                    "fault-control-execution",
                    "control observation is outside planned commands",
                )
            })?;
        validate_observation_transition(plan, observation, &command.bytes)?;
    }
    Ok(())
}
