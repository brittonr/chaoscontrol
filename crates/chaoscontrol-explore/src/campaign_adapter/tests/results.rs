use campaign_core::IdKind;

use super::{config, digest, Session, EXPLOIT};
use crate::campaign_adapter::{
    candidate_record, finding_ref, result_observation, AdapterError, ResultFacts, RetainedChild,
};

const DEDUP_KEY: u64 = 0xdead;

// r[verify chaoscontrol.campaign_policy.observations.valid]
#[test]
fn child_moments_and_findings_settle_and_extend_the_frontier() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[3.0]);
    let selection = session.executed(EXPLOIT);
    let child = session.admit_child(&selection, 0);
    let finding = finding_ref(DEDUP_KEY, digest(40), selection.parent_moment_id);
    assert_eq!(finding.finding_id.kind, IdKind::Finding);
    assert_eq!(finding.priority_facts_id.kind, IdKind::SourceFacts);
    let facts = ResultFacts {
        children: vec![RetainedChild::from(&child)],
        findings: vec![finding.clone()],
        round_manifest_digest: digest(41),
    };
    let observation = session.settle(&selection, &facts);
    assert_eq!(observation.children.len(), 1);
    assert_eq!(session.state.findings(), vec![finding]);
    assert_eq!(session.state.known_moment_depth(child.moment_id), Some(1));
    let grandchild = candidate_record(
        &session.identity,
        &session.state,
        &session.entry_facts(child.moment_id, 9),
        session.generation_profile_id,
    )
    .unwrap();
    assert_eq!(grandchild.depth, 2);
    assert_ne!(
        finding_ref(DEDUP_KEY + 1, digest(40), selection.parent_moment_id).finding_id,
        observation.findings[0].finding_id
    );
}

// r[verify chaoscontrol.campaign_policy.observations.invalid]
#[test]
fn foreign_unknown_or_duplicate_results_reject() {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    session.admit_entries(root, &[3.0]);
    let selection = session.executed(EXPLOIT);
    let child = session.admit_child(&selection, 0);
    let duplicate = ResultFacts {
        children: vec![RetainedChild::from(&child), RetainedChild::from(&child)],
        ..super::no_children()
    };
    assert_eq!(
        result_observation(&session.identity, &session.state, &selection, &duplicate),
        Err(AdapterError::DuplicateChild)
    );
    let mut other = Session::open(&crate::explorer::ExplorerConfig {
        seed: super::SEED + 1,
        ..config()
    });
    let other_root = other.root.moment_id;
    other.admit_entries(other_root, &[3.0]);
    let foreign = other.executed(EXPLOIT);
    assert_eq!(
        result_observation(
            &session.identity,
            &session.state,
            &foreign,
            &super::no_children()
        ),
        Err(AdapterError::CrossedCampaign)
    );
    let mut unpublished = selection.clone();
    unpublished.selection_id = unpublished.candidate_id;
    unpublished.selection_id.kind = IdKind::Selection;
    assert_eq!(
        result_observation(
            &session.identity,
            &session.state,
            &unpublished,
            &super::no_children()
        ),
        Err(AdapterError::UnknownSelection)
    );
}
