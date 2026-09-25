use campaign_core::IdKind;

use super::SEED;
use crate::campaign_adapter::{rank_profile_id, AdapterIdentity, AdapterReason};

const OTHER_SEED: u64 = SEED + 1;

// r[verify chaoscontrol.campaign_policy.adapter.valid]
#[test]
fn identities_have_declared_kinds_and_bind_both_conversion_profiles() {
    let identity = AdapterIdentity::for_seed(SEED);
    assert_eq!(identity, AdapterIdentity::for_seed(SEED));
    let expected = [
        (identity.adapter_id, IdKind::Adapter),
        (identity.projection_schema_id, IdKind::Projection),
        (identity.control_domain_id, IdKind::ControlDomain),
        (identity.guidance_profile_id, IdKind::GuidanceProfile),
        (identity.entropy_rule_id, IdKind::EntropyProfile),
        (identity.entropy_source_id, IdKind::EntropyProfile),
        (identity.operation_schema_id, IdKind::OperationSchema),
        (identity.resource_dimension_id, IdKind::ResourceDimension),
        (identity.cost_profile_id, IdKind::CostProfile),
        (identity.policy_id, IdKind::Policy),
        (identity.production_policy_id, IdKind::Policy),
        (identity.scheduling_policy_id, IdKind::SchedulingProfile),
        (identity.coalescing_policy_id, IdKind::CoalescingPolicy),
        (identity.retention_profile_id, IdKind::RetentionProfile),
    ];
    for (id, kind) in expected {
        assert_eq!(id.require_kind(kind), Ok(id));
    }
    assert_eq!(identity.guidance_profile_id, rank_profile_id());
    assert_ne!(identity.policy_id, identity.production_policy_id);
    assert_ne!(identity.entropy_rule_id, identity.entropy_source_id);
}

// r[verify chaoscontrol.campaign_policy.entropy.invalid]
#[test]
fn only_the_entropy_source_binds_the_seed() {
    let identity = AdapterIdentity::for_seed(SEED);
    let other = AdapterIdentity::for_seed(OTHER_SEED);
    assert_ne!(identity.entropy_source_id, other.entropy_source_id);
    assert_eq!(identity.adapter_id, other.adapter_id);
    assert_eq!(identity.entropy_rule_id, other.entropy_rule_id);
    assert_eq!(
        AdapterIdentity {
            entropy_source_id: other.entropy_source_id,
            ..identity.clone()
        },
        other
    );
}

#[test]
fn reasons_are_distinct_bounded_and_round_trip() {
    let identity = AdapterIdentity::for_seed(SEED);
    let mut ids = Vec::new();
    for reason in AdapterReason::ALL {
        let id = identity.reason_id(reason);
        assert_eq!(id.kind, IdKind::Reason);
        assert_eq!(AdapterReason::from_id(id), Some(reason));
        ids.push(id);
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), AdapterReason::ALL.len());
    assert_eq!(AdapterReason::from_id(identity.policy_id), None);
}
