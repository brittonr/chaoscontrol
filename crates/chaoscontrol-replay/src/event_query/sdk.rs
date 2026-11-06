use super::*;
use chaoscontrol_protocol::guest_process::{
    admit_manifest, validate_receipt, LifecycleKind, MultiprocessReceipt, ProcessManifest,
};

/// Adapt retained SDK supervisor lifecycle facts, never VM status projections.
/// The supplied identity is the exact receipt artifact digest; event positions
/// are offsets in its retained events array. Owners must belong to the admitted
/// manifest and ticks must retain supervisor order.
pub fn envelopes_from_sdk_lifecycle(
    source_id: [u8; 32],
    vm_index: u32,
    manifest: &ProcessManifest,
    receipt: &MultiprocessReceipt,
) -> Result<Vec<EventEnvelope>, EventQueryError> {
    if source_id == [0; 32] {
        return Err(EventQueryError::InvalidEvent);
    }
    if receipt.events.len() > MAX_SCANNED_EVENTS {
        return Err(EventQueryError::ScanBound);
    }
    let admitted = admit_manifest(manifest).map_err(|_| EventQueryError::UnknownOwner)?;
    validate_receipt(&admitted, receipt).map_err(|_| EventQueryError::InvalidEvent)?;
    if receipt
        .events
        .windows(2)
        .any(|pair| pair[0].tick > pair[1].tick)
    {
        return Err(EventQueryError::AmbiguousTime);
    }
    receipt
        .events
        .iter()
        .enumerate()
        .map(|(sequence, event)| {
            let kind = match event.kind {
                LifecycleKind::Spawned => "sdk_process_spawned",
                LifecycleKind::Exited => "sdk_process_exited",
                LifecycleKind::Restarted => "sdk_process_restarted",
                LifecycleKind::Killed => "sdk_process_killed",
                LifecycleKind::Paused => "sdk_process_paused",
                LifecycleKind::Resumed => "sdk_process_resumed",
            };
            let payload = serde_json::to_vec(event).map_err(|_| EventQueryError::InvalidEvent)?;
            let envelope = EventEnvelope {
                schema: EVENT_ENVELOPE_SCHEMA.to_string(),
                source_id,
                source_kind: EventSourceKind::SdkLifecycle,
                source_sequence: sequence as u64,
                tick: event.tick,
                vm_index: Some(vm_index),
                process: Some(event.process_identity.clone()),
                thread: None,
                kind: kind.to_string(),
                text: format!("{} {} generation={}", kind, event.role, event.generation),
                payload_digest: *blake3::hash(&payload).as_bytes(),
                details: Some(
                    serde_json::to_value(event).map_err(|_| EventQueryError::InvalidEvent)?,
                ),
                derived: BTreeMap::new(),
            };
            admit_event(&envelope)?;
            Ok(envelope)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_protocol::guest_process::{
        make_receipt, ProcessLifecycleEvent, ProcessSpec, RestartMode, RestartPolicy,
        PROCESS_MANIFEST_SCHEMA,
    };

    #[test]
    fn sdk_lifecycle_keeps_process_owner_tick_generation_and_source_position() {
        let manifest = ProcessManifest {
            schema: PROCESS_MANIFEST_SCHEMA.into(),
            guest: "fixture".into(),
            shared_directories: vec![],
            processes: vec![ProcessSpec {
                role: "worker".into(),
                executable: "/bin/worker".into(),
                arguments: vec![],
                environment: BTreeMap::new(),
                shared_directories: vec![],
                restart: RestartPolicy {
                    mode: RestartMode::Never,
                    max_restarts: 0,
                },
                instrumented: true,
                transport_slot: Some(0),
            }],
        };
        let admitted = admit_manifest(&manifest).unwrap();
        let owner = admitted.processes[0].identity.clone();
        let event = ProcessLifecycleEvent {
            process_identity: owner.clone(),
            role: "worker".into(),
            generation: 0,
            tick: 7,
            kind: LifecycleKind::Spawned,
        };
        let receipt = make_receipt(&admitted, vec![event.clone()]);
        let envelopes = envelopes_from_sdk_lifecycle([3; 32], 2, &manifest, &receipt).unwrap();
        assert_eq!(envelopes[0].process.as_ref(), Some(&owner));
        assert_eq!(envelopes[0].vm_index, Some(2));
        assert_eq!(envelopes[0].tick, 7);
        assert_eq!(envelopes[0].source_sequence, 0);
        assert_eq!(envelopes[0].source_id, [3; 32]);
        assert_eq!(
            envelopes[0].payload_digest,
            *blake3::hash(&serde_json::to_vec(&event).unwrap()).as_bytes()
        );
        // Cross-source fixture uses real retained producer types and adapters,
        // rather than hand-written envelope JSON or source-code wording checks.
        let mut cross_source = envelopes.clone();
        let assertion = crate::recording::RecordedEvent::AssertionHit {
            tick: 7,
            vm_index: 2,
            assertion_id: 9,
            location: "worker ready".into(),
            passed: true,
        };
        let assertion_bytes = serde_json::to_vec(&assertion).unwrap();
        cross_source.push(
            envelope_from_recorded_event(*blake3::hash(&assertion_bytes).as_bytes(), 0, &assertion)
                .unwrap(),
        );
        let serial = crate::recording::RecordedEvent::SerialOutput {
            tick: 7,
            vm_index: 2,
            data: "ready\n".into(),
        };
        cross_source.push(
            envelope_from_recorded_event(
                *blake3::hash(&serde_json::to_vec(&serial).unwrap()).as_bytes(),
                0,
                &serial,
            )
            .unwrap(),
        );
        let marker = MarkerEvidenceContext {
            marker_identity: chaoscontrol_protocol::branch_marker::marker_identity(
                "fixture",
                "worker-ready",
            ),
            owner: owner.clone(),
            state_ref: None,
            logical_position_ref: None,
            run_id: 1,
            observed_tick: 7,
        };
        cross_source.extend(
            envelopes_from_markers(
                *blake3::hash(&serde_json::to_vec(&marker).unwrap()).as_bytes(),
                &[marker],
            )
            .unwrap(),
        );
        use chaoscontrol_vmm::dlog::{DlogRecord, DlogTag};
        let dlog = DlogRecord::new(4, 999, 2, 0x1000, DlogTag::IoOut, 0).to_bytes();
        cross_source.extend(
            envelopes_from_dlog_bytes(
                &dlog,
                1,
                &DlogContext {
                    vm_index: 2,
                    ticks: vec![DlogTickAnchor {
                        source_sequence: 4,
                        tick: 7,
                    }],
                },
            )
            .unwrap(),
        );
        let query = EventQuery::Window {
            input: Box::new(EventQuery::All),
            start_tick: 7,
            end_tick: 7,
        };
        let limits = EventQueryLimits::default();
        let result = evaluate(&query, &cross_source, &limits).unwrap();
        assert_eq!(result.matched, 5);
        assert_eq!(
            result
                .rows
                .iter()
                .map(|e| e.source_kind)
                .collect::<BTreeSet<_>>()
                .len(),
            5
        );
        assert_eq!(evaluate(&query, &cross_source, &limits).unwrap(), result);
        let query_receipt =
            super::super::receipt_for(&query, &cross_source, &result, &limits).unwrap();
        super::super::validate_receipt(&query_receipt, &query, &cross_source, &result, &limits)
            .unwrap();
        let mut unknown = receipt.clone();
        unknown.events[0].process_identity = "unknown".into();
        assert!(envelopes_from_sdk_lifecycle([3; 32], 2, &manifest, &unknown).is_err());
        let mut reversed = receipt.clone();
        let mut earlier = event;
        earlier.tick = 1;
        reversed.events.push(earlier);
        assert_eq!(
            envelopes_from_sdk_lifecycle([3; 32], 2, &manifest, &reversed).unwrap_err(),
            EventQueryError::AmbiguousTime
        );
    }
}
