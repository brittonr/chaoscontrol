//! Bounded VMM collection of SDK protocol-observation envelopes.
//!
//! Guest workloads emit one opaque envelope per observation through the
//! lifecycle event transport and leave the scheduler position unset. This
//! module owns two transport facts only: the bounded retention of admitted
//! envelopes and the deterministic scheduler position bound at arrival.
//!
//! Projection bytes stay opaque. Protocol meaning, cohort completeness, and
//! oracle results remain consumer-owned.
// r[impl chaoscontrol.protocol_observation.envelope]

use chaoscontrol_protocol::{decode_payload, HypercallPage, PAYLOAD_MAX};
use chaoscontrol_smr::observation::{ProtocolObservation, MAX_BYTES, MAX_RECORDS};
use serde_json::Value;

/// Lifecycle event name carrying one protocol-observation envelope.
pub const EVENT_NAME: &str = chaoscontrol_smr::observation::SCHEMA;

/// Retained records and bytes for one VM between evidence drains.
///
/// The default mirrors the published contract ceilings so VMM retention can
/// never exceed the aggregate bound one admitted profile can declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolCollectionLimits {
    /// Maximum retained observations for this VM.
    pub records: u64,
    /// Maximum retained canonical envelope bytes for this VM.
    pub bytes: u64,
}

impl Default for ProtocolCollectionLimits {
    fn default() -> Self {
        Self {
            records: MAX_RECORDS,
            bytes: MAX_BYTES,
        }
    }
}

/// Rejections observed while collecting envelopes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolCollectionRejections {
    /// The payload was not a canonical envelope object.
    pub malformed: u64,
    /// The producer supplied a scheduler position the VMM owns.
    pub scheduler_claimed: u64,
    /// The retained record bound was reached.
    pub record_bound: u64,
    /// The retained byte bound was reached.
    pub byte_bound: u64,
}

/// Retained volume and rejection counts for one VM.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolObservationState {
    /// Canonical envelope bytes currently retained.
    pub retained_bytes: u64,
    /// Rejections observed since construction or the last reset.
    pub rejections: ProtocolCollectionRejections,
}

/// Bounded per-VM retention for protocol-observation envelopes.
#[derive(Debug, Default)]
pub struct ProtocolObservationCollector {
    limits: ProtocolCollectionLimits,
    records: Vec<ProtocolObservation>,
    retained_bytes: u64,
    rejections: ProtocolCollectionRejections,
}

impl ProtocolObservationCollector {
    /// Create a collector with explicit retention limits.
    pub fn with_limits(limits: ProtocolCollectionLimits) -> Self {
        Self {
            limits,
            records: Vec::new(),
            retained_bytes: 0,
            rejections: ProtocolCollectionRejections::default(),
        }
    }

    /// Retain one opaque envelope with the VMM-bound scheduler position.
    ///
    /// A producer-supplied non-empty `scheduler_ref` is rejected; the VMM never
    /// accepts a scheduler claim from the guest.
    pub fn collect(&mut self, json_details: &[u8], scheduler_ref: &str) {
        let Some(mut value) = serde_json::from_slice::<Value>(json_details).ok() else {
            self.reject_malformed();
            return;
        };
        let Some(object) = value.as_object_mut() else {
            self.reject_malformed();
            return;
        };
        match object.get("scheduler_ref") {
            None => {}
            Some(Value::String(existing)) if existing.is_empty() => {}
            Some(Value::String(_)) => {
                self.rejections.scheduler_claimed =
                    self.rejections.scheduler_claimed.saturating_add(1);
                return;
            }
            Some(_) => {
                self.reject_malformed();
                return;
            }
        }
        object.insert(
            "scheduler_ref".to_string(),
            Value::String(scheduler_ref.to_string()),
        );
        let Ok(observation) = serde_json::from_value::<ProtocolObservation>(value) else {
            self.reject_malformed();
            return;
        };
        let Ok(encoded) = serde_json::to_vec(&observation) else {
            self.reject_malformed();
            return;
        };
        let record_bytes = encoded.len() as u64;
        if self.records.len() as u64 >= self.limits.records {
            self.rejections.record_bound = self.rejections.record_bound.saturating_add(1);
            return;
        }
        if self.retained_bytes.saturating_add(record_bytes) > self.limits.bytes {
            self.rejections.byte_bound = self.rejections.byte_bound.saturating_add(1);
            return;
        }
        self.retained_bytes += record_bytes;
        self.records.push(observation);
    }

    /// Retained observations in arrival order.
    pub fn records(&self) -> &[ProtocolObservation] {
        &self.records
    }

    /// Take retained observations in arrival order.
    pub fn take_records(&mut self) -> Vec<ProtocolObservation> {
        self.retained_bytes = 0;
        std::mem::take(&mut self.records)
    }

    /// Canonical bytes currently retained for this VM.
    pub fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }

    /// Rejections observed since construction or the last reset.
    pub fn rejections(&self) -> ProtocolCollectionRejections {
        self.rejections
    }

    /// Drop retained observations and rejection counters.
    ///
    /// Snapshot and incremental restore call this because observations recorded
    /// on the abandoned branch are not facts about the restored state.
    pub fn reset(&mut self) {
        self.records.clear();
        self.retained_bytes = 0;
        self.rejections = ProtocolCollectionRejections::default();
    }

    fn reject_malformed(&mut self) {
        self.rejections.malformed = self.rejections.malformed.saturating_add(1);
    }
}

/// Collect one lifecycle hypercall payload when it carries an observation.
///
/// Other lifecycle events are ignored. Decoded payloads that are not protocol
/// observations never touch the collector.
pub fn collect_hypercall(
    collector: &mut ProtocolObservationCollector,
    page: &HypercallPage,
    scheduler_ref: &str,
) {
    let payload_len = (page.payload_len as usize).min(PAYLOAD_MAX);
    let Some(decoded) = decode_payload(&page.payload[..payload_len]) else {
        return;
    };
    if decoded.message != EVENT_NAME {
        return;
    }
    collector.collect(&decoded.json_details, scheduler_ref);
}

/// Canonical `blake3:` reference for a deterministic scheduler position.
pub fn scheduler_position_ref(state_id: &crate::scheduler::core::ScheduleStateId) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut reference = String::with_capacity(7 + 64);
    reference.push_str("blake3:");
    for byte in state_id.0 {
        reference.push(HEX[(byte >> 4) as usize] as char);
        reference.push(HEX[(byte & 0x0f) as usize] as char);
    }
    reference
}

#[cfg(test)]
// r[verify chaoscontrol.protocol_observation.validation]
mod tests {
    use super::*;
    use serde_json::json;

    const REFERENCE: &str =
        "blake3:0000000000000000000000000000000000000000000000000000000000000001";
    const SCHEDULER: &str =
        "blake3:0000000000000000000000000000000000000000000000000000000000000002";

    fn envelope() -> Value {
        json!({
            "schema": EVENT_NAME,
            "profile_ref": REFERENCE,
            "protocol_ref": REFERENCE,
            "projection_schema_ref": REFERENCE,
            "participant_ref": REFERENCE,
            "producer_ref": REFERENCE,
            "guest_id": 0,
            "generation": 1,
            "source_sequence": 1,
            "transition_class_ref": REFERENCE,
            "boundary_ref": REFERENCE,
            "projection_ref": REFERENCE,
            "projection": [1, 2, 3],
            "scheduler_ref": "",
            "dropped": 0,
        })
    }

    fn collect(collector: &mut ProtocolObservationCollector, value: &Value) {
        let bytes = serde_json::to_vec(value).unwrap();
        collector.collect(&bytes, SCHEDULER);
    }

    fn hypercall_page(message: &str, details: &[u8]) -> HypercallPage {
        let mut page = HypercallPage::zeroed();
        let length = chaoscontrol_protocol::encode_payload(&mut page.payload, message, details)
            .expect("encoded hypercall payload fits the page");
        page.payload_len = length as u16;
        page
    }

    #[test]
    fn envelope_retains_opaque_projection_with_bound_scheduler_position() {
        let mut collector = ProtocolObservationCollector::default();
        collect(&mut collector, &envelope());
        assert_eq!(collector.records().len(), 1);
        assert!(collector.retained_bytes() > 0);
        let record = &collector.records()[0];
        assert_eq!(record.scheduler_ref, SCHEDULER);
        assert_eq!(record.projection, Some(vec![1, 2, 3]));
        assert_eq!(record.projection_ref, REFERENCE);
        assert_eq!(
            collector.rejections(),
            ProtocolCollectionRejections::default()
        );
    }

    #[test]
    fn missing_scheduler_slot_is_bound_by_the_vmm() {
        let mut collector = ProtocolObservationCollector::default();
        let mut value = envelope();
        value.as_object_mut().unwrap().remove("scheduler_ref");
        collect(&mut collector, &value);
        assert_eq!(collector.records().len(), 1);
        assert_eq!(collector.records()[0].scheduler_ref, SCHEDULER);
    }

    #[test]
    fn producer_scheduler_claims_and_malformed_envelopes_are_rejected() {
        let mut collector = ProtocolObservationCollector::default();
        let mut claimed = envelope();
        claimed["scheduler_ref"] = json!(SCHEDULER);
        collect(&mut collector, &claimed);

        let mut unknown = envelope();
        unknown["unexpected"] = json!(true);
        collect(&mut collector, &unknown);

        let mut wrong_type = envelope();
        wrong_type["scheduler_ref"] = json!(7);
        collect(&mut collector, &wrong_type);

        collector.collect(b"not-json", SCHEDULER);
        collector.collect(b"[]", SCHEDULER);

        assert!(collector.records().is_empty());
        assert_eq!(collector.retained_bytes(), 0);
        assert_eq!(collector.rejections().scheduler_claimed, 1);
        assert_eq!(collector.rejections().malformed, 4);
    }

    #[test]
    fn retention_bounds_fail_closed_and_count_rejections() {
        let mut collector = ProtocolObservationCollector::with_limits(ProtocolCollectionLimits {
            records: 1,
            bytes: u64::MAX,
        });
        collect(&mut collector, &envelope());
        collect(&mut collector, &envelope());
        assert_eq!(collector.records().len(), 1);
        assert_eq!(collector.rejections().record_bound, 1);

        let mut collector = ProtocolObservationCollector::with_limits(ProtocolCollectionLimits {
            records: u64::MAX,
            bytes: 1,
        });
        collect(&mut collector, &envelope());
        assert!(collector.records().is_empty());
        assert_eq!(collector.retained_bytes(), 0);
        assert_eq!(collector.rejections().byte_bound, 1);
    }

    #[test]
    fn take_and_reset_clear_retained_state() {
        let mut collector = ProtocolObservationCollector::default();
        collect(&mut collector, &envelope());
        let taken = collector.take_records();
        assert_eq!(taken.len(), 1);
        assert!(collector.records().is_empty());
        assert_eq!(collector.retained_bytes(), 0);

        collect(&mut collector, &envelope());
        collector.collect(b"not-json", SCHEDULER);
        collector.reset();
        assert!(collector.records().is_empty());
        assert_eq!(
            collector.rejections(),
            ProtocolCollectionRejections::default()
        );
    }

    #[test]
    fn hypercall_payload_round_trip_binds_scheduler_position() {
        let mut collector = ProtocolObservationCollector::default();
        let details = serde_json::to_vec(&envelope()).unwrap();
        let page = hypercall_page(EVENT_NAME, &details);
        collect_hypercall(&mut collector, &page, SCHEDULER);
        assert_eq!(collector.records().len(), 1);
        assert_eq!(collector.records()[0].scheduler_ref, SCHEDULER);
        assert_eq!(
            collector.rejections(),
            ProtocolCollectionRejections::default()
        );
    }

    #[test]
    fn unrelated_or_empty_lifecycle_events_are_ignored() {
        let mut collector = ProtocolObservationCollector::default();
        let setup = hypercall_page("setup_complete", b"{}");
        collect_hypercall(&mut collector, &setup, SCHEDULER);
        let empty = HypercallPage::zeroed();
        collect_hypercall(&mut collector, &empty, SCHEDULER);
        assert!(collector.records().is_empty());
        assert_eq!(
            collector.rejections(),
            ProtocolCollectionRejections::default()
        );
    }

    #[test]
    fn sdk_producer_envelopes_collect_with_one_based_sequences() {
        let producer = chaoscontrol_sdk::protocol_observation::ProtocolObservationProducer::new(
            REFERENCE, REFERENCE, REFERENCE, REFERENCE, REFERENCE, 0, 1,
        )
        .expect("valid producer references");
        let value = producer
            .envelope(REFERENCE, REFERENCE, Some(b"opaque"), 1)
            .expect("bounded envelope");
        let bytes = serde_json::to_vec(&value).unwrap();
        let mut collector = ProtocolObservationCollector::default();
        collector.collect(&bytes, SCHEDULER);
        assert_eq!(collector.records().len(), 1);
        assert_eq!(collector.records()[0].source_sequence, 1);
        assert_eq!(collector.records()[0].scheduler_ref, SCHEDULER);
    }

    #[test]
    fn scheduler_position_references_are_canonical_and_distinct() {
        let first = crate::scheduler::core::ScheduleStateId([1; 32]);
        let second = crate::scheduler::core::ScheduleStateId([2; 32]);
        let reference = scheduler_position_ref(&first);
        assert_eq!(reference.len(), 71);
        assert_eq!(reference, scheduler_position_ref(&first));
        assert!(reference.starts_with("blake3:"));
        assert_ne!(reference, scheduler_position_ref(&second));
    }
}
