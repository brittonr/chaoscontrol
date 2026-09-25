use std::collections::BTreeSet;

use campaign_choregraph::{derive_control_payload_id, EventId};
use campaign_core::CampaignControlEvent;
use ciborium::value::Value;

use super::{config, digest, empty_graph, Session, EXPLOIT};
use crate::campaign_adapter::{
    decode_control_event, decode_moment, encode_control_event, encode_moment,
    ineligible_retirement, readmit_moment, stop_reason, unobserved_closure, AdapterReason,
    IneligibleReason, ProductStop, ResultFacts, RetainedChild, WireError,
};

const ALL_KINDS: usize = 9;
const UNKNOWN_KIND_CODE: u8 = 200;

fn kind(event: &CampaignControlEvent) -> u32 {
    match event {
        CampaignControlEvent::CampaignAdmitted { .. } => 1,
        CampaignControlEvent::CandidatesAdmitted(_) => 2,
        CampaignControlEvent::GuidanceAdmitted(_) => 3,
        CampaignControlEvent::SelectionPublished(_) => 4,
        CampaignControlEvent::ResultObserved(_) => 5,
        CampaignControlEvent::UnobservedClosed(_) => 6,
        CampaignControlEvent::CandidateRetired(_) => 7,
        CampaignControlEvent::CandidatesPruned(_) => 8,
        CampaignControlEvent::StopAccepted(_) => 9,
    }
}

/// Session whose control lineage carries every control-event shape the adapter emits.
fn full_lineage() -> (Session, Vec<(EventId, campaign_choregraph::EventDraft)>) {
    let mut session = Session::open(&config());
    let root = session.root.moment_id;
    let candidates = session.admit_entries(root, &[3.0, 2.0, 1.0]);
    let selection = session.executed(EXPLOIT);
    let child = session.admit_child(&selection, 0);
    let finding = crate::campaign_adapter::finding_ref(1, digest(40), root);
    session.settle(
        &selection,
        &ResultFacts {
            children: vec![RetainedChild::from(&child)],
            findings: vec![finding],
            round_manifest_digest: digest(41),
        },
    );
    let lost = session.executed(EXPLOIT);
    let closure =
        unobserved_closure(&session.identity, &lost, AdapterReason::SnapshotStateLost).unwrap();
    let event = campaign_core::close_unobserved(&session.state, closure).unwrap();
    session.publish(event);
    let retirement = ineligible_retirement(
        &session.identity,
        &session.state,
        candidates[2],
        IneligibleReason::StaleBaseSet,
    );
    let event = campaign_core::retire_candidate(&session.state, retirement).unwrap();
    session.publish(event);
    session.admit_entries(child.moment_id, &[0.5, 0.25]);
    assert!(!session.prune().is_empty());
    let stop = stop_reason(&session.identity, ProductStop::Interrupted);
    let event = campaign_core::admit_stop(&session.state, stop).unwrap();
    session.publish(event);
    let moments = [session.root.event, child.event]
        .into_iter()
        .map(|event| {
            let draft = session.graph.event(event).unwrap().content().clone();
            (event, draft)
        })
        .collect();
    (session, moments)
}

// r[verify chaoscontrol.campaign_policy.history.valid]
#[test]
fn every_control_event_round_trips_with_recomputed_identities() {
    let (session, _) = full_lineage();
    let events = session.control_events();
    let mut kinds = BTreeSet::new();
    for event in &events {
        let decoded = decode_control_event(&encode_control_event(event).unwrap()).unwrap();
        assert_eq!(&decoded, event);
        assert_eq!(decoded.expected_event_id(), Ok(event.event_id()));
        assert_eq!(
            derive_control_payload_id(&decoded),
            derive_control_payload_id(event)
        );
        kinds.insert(kind(event));
    }
    assert_eq!(kinds.len(), ALL_KINDS);
    let replayed: Vec<_> = events
        .iter()
        .map(|event| decode_control_event(&encode_control_event(event).unwrap()).unwrap())
        .collect();
    assert_eq!(campaign_core::reconstruct(&replayed), Ok(session.state));
}

fn field<'a>(value: &'a mut Value, key: &str) -> &'a mut Value {
    let Value::Map(entries) = value else {
        panic!("expected a CBOR map at {key}");
    };
    entries
        .iter_mut()
        .find(|(name, _)| name.as_text() == Some(key))
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("missing field {key}"))
}

fn tampered(event: &CampaignControlEvent, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let bytes = encode_control_event(event).unwrap();
    let mut value: Value = ciborium::de::from_reader(bytes.as_slice()).unwrap();
    edit(&mut value);
    let mut out = Vec::new();
    ciborium::ser::into_writer(&value, &mut out).unwrap();
    out
}

fn guidance_event(session: &Session) -> CampaignControlEvent {
    session
        .control_events()
        .into_iter()
        .find(|event| matches!(event, CampaignControlEvent::GuidanceAdmitted(_)))
        .unwrap()
}

// r[verify chaoscontrol.campaign_policy.history.invalid]
#[test]
fn codec_drift_and_rewritten_records_fail_identity_checks() {
    let (session, _) = full_lineage();
    let guidance = guidance_event(&session);
    let rank_changed = tampered(&guidance, |value| {
        let body = field(field(value, "event"), "GuidanceAdmitted");
        let Value::Array(ranks) = field(field(body, "guidance"), "ranks") else {
            panic!("ranks are an array");
        };
        ranks[0] = Value::Integer(1.into());
    });
    assert_eq!(
        decode_control_event(&rank_changed),
        Err(WireError::IdentityMismatch)
    );
    let stored_id_changed = tampered(&guidance, |value| {
        let body = field(field(value, "event"), "GuidanceAdmitted");
        let guidance_id = field(field(body, "guidance"), "guidance_id");
        *field(guidance_id, "digest") = Value::Array(vec![Value::Integer(7.into()); 32]);
    });
    assert_eq!(
        decode_control_event(&stored_id_changed),
        Err(WireError::IdentityMismatch)
    );
    let admission = &session.control_events()[0];
    let campaign_changed = tampered(admission, |value| {
        let body = field(field(value, "event"), "CampaignAdmitted");
        let campaign_id = field(field(body, "admission"), "campaign_id");
        *field(campaign_id, "digest") = Value::Array(vec![Value::Integer(9.into()); 32]);
    });
    assert_eq!(
        decode_control_event(&campaign_changed),
        Err(WireError::IdentityMismatch)
    );
    let unknown_kind = tampered(&guidance, |value| {
        *field(field(value, "payload_id"), "kind") = Value::Integer(UNKNOWN_KIND_CODE.into());
    });
    assert_eq!(
        decode_control_event(&unknown_kind),
        Err(WireError::UnknownIdKind(UNKNOWN_KIND_CODE))
    );
    let unknown_field = tampered(&guidance, |value| {
        let Value::Map(entries) = value else {
            panic!("record is a map");
        };
        entries.push((Value::Text("extra".to_owned()), Value::Bool(true)));
    });
    assert_eq!(decode_control_event(&unknown_field), Err(WireError::Decode));
    let mut trailing = encode_control_event(&guidance).unwrap();
    trailing.push(0);
    assert_eq!(decode_control_event(&trailing), Err(WireError::Decode));
    assert_eq!(decode_control_event(&[0xff, 0x00]), Err(WireError::Decode));
}

// r[verify chaoscontrol.campaign_policy.history.valid]
// r[verify chaoscontrol.campaign_policy.history.invalid]
#[test]
fn moment_records_round_trip_and_rebuild_the_same_history() {
    let (session, moments) = full_lineage();
    let mut rebuilt = empty_graph();
    for (event, draft) in &moments {
        let bytes = encode_moment(*event, draft).unwrap();
        let (decoded_event, decoded_draft) = decode_moment(&bytes).unwrap();
        assert_eq!((decoded_event, &decoded_draft), (*event, draft));
        rebuilt = readmit_moment(&rebuilt, decoded_event, decoded_draft).unwrap();
    }
    assert!(rebuilt.contains(session.root.event));
    let (root_event, root_draft) = &moments[0];
    let (child_event, _) = &moments[1];
    assert_eq!(
        encode_moment(*child_event, root_draft),
        Err(WireError::IdentityMismatch)
    );
    let bytes = encode_moment(*root_event, root_draft).unwrap();
    let mut value: Value = ciborium::de::from_reader(bytes.as_slice()).unwrap();
    *field(&mut value, "payload") = Value::Array(vec![Value::Integer(3.into()); 32]);
    let mut rewritten = Vec::new();
    ciborium::ser::into_writer(&value, &mut rewritten).unwrap();
    assert_eq!(decode_moment(&rewritten), Err(WireError::IdentityMismatch));
}
