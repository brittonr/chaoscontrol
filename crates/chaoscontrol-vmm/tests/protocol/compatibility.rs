use super::*;

#[test]
fn old_empty_snapshots_decode_but_malformed_collections_fail() {
    let engine = FaultEngine::new(EngineConfig::default());
    let expected = serde_json::to_value(engine.snapshot()).unwrap();
    let mut legacy = expected.clone();
    for field in ["protocol_observations", "process_fault_queue"] {
        legacy.as_object_mut().unwrap().remove(field);
    }
    let snapshot: chaoscontrol_fault::engine::EngineSnapshot =
        serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(snapshot).unwrap(), expected);
    for field in ["protocol_observations", "process_fault_queue"] {
        let mut malformed = legacy.clone();
        malformed[field] = true.into();
        assert!(
            serde_json::from_value::<chaoscontrol_fault::engine::EngineSnapshot>(malformed)
                .is_err()
        );
    }
}

#[test]
fn scheduler_reference_keeps_every_digest_byte_and_rejects_truncation() {
    let bytes = [0; blake3::OUT_LEN];
    let reference = chaoscontrol_vmm::scheduler::core::ScheduleStateId(bytes).to_reference();
    validate_exact_reference(&reference, "schedule-state").unwrap();
    assert_eq!(
        reference,
        format!("schedule-state:{}", "0".repeat(BLAKE3_HEX_BYTES))
    );
    let mut changed = bytes;
    *changed.last_mut().unwrap() = 1;
    let other = chaoscontrol_vmm::scheduler::core::ScheduleStateId(changed).to_reference();
    assert_ne!(reference, other);
    assert!(other.ends_with("01"));
    let mut truncated = reference;
    truncated.pop();
    assert!(validate_exact_reference(&truncated, "schedule-state").is_err());
}
