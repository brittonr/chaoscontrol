// r[impl chaoscontrol.protocol_observation.envelope]
//! Bounded guest-side protocol-observation producer.
//!
//! Emits opaque envelope events through the existing lifecycle hypercall. The
//! producer owns only sequence and loss accounting; workloads own protocol
//! meaning and the VMM owns scheduler-position binding and cohort assembly.

use serde_json::json;

/// Lifecycle event name carrying one protocol observation envelope.
pub const PROTOCOL_OBSERVATION_EVENT: &str = "chaoscontrol.protocol-observation.v1";

/// Maximum reference length mirrored from the cohort contract.
pub const MAX_REFERENCE_LENGTH: usize = 256;

/// Maximum optional projection bytes accepted as an inline projection.
pub const MAX_PROJECTION_BYTES: usize = 4096;

/// Guest-side producer for one participant stream.
#[derive(Debug, Clone)]
pub struct ProtocolObservationProducer {
    profile_ref: String,
    protocol_ref: String,
    projection_schema_ref: String,
    participant_ref: String,
    producer_ref: String,
    guest_id: u32,
    generation: u64,
    next_sequence: u64,
    dropped: u64,
}

/// Rejection for producer construction or emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolObservationError {
    /// A reference is empty, too long, or not a lowercase BLAKE3 reference.
    InvalidReference,
    /// The projection exceeds the inline bound.
    ProjectionTooLarge,
    /// The source sequence cannot advance.
    SequenceOverflow,
}

fn valid_ref(value: &str) -> bool {
    value.len() <= MAX_REFERENCE_LENGTH
        && value.strip_prefix("blake3:").is_some_and(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

impl ProtocolObservationProducer {
    /// Create a producer for one participant stream.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        profile_ref: &str,
        protocol_ref: &str,
        projection_schema_ref: &str,
        participant_ref: &str,
        producer_ref: &str,
        guest_id: u32,
        generation: u64,
    ) -> Result<Self, ProtocolObservationError> {
        for reference in [
            profile_ref,
            protocol_ref,
            projection_schema_ref,
            participant_ref,
            producer_ref,
        ] {
            if !valid_ref(reference) {
                return Err(ProtocolObservationError::InvalidReference);
            }
        }
        Ok(Self {
            profile_ref: profile_ref.to_string(),
            protocol_ref: protocol_ref.to_string(),
            projection_schema_ref: projection_schema_ref.to_string(),
            participant_ref: participant_ref.to_string(),
            producer_ref: producer_ref.to_string(),
            guest_id,
            generation,
            next_sequence: 1,
            dropped: 0,
        })
    }

    /// Next source sequence that will be assigned.
    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Losses recorded since construction.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Record observations that could not be emitted.
    pub fn record_dropped(&mut self, count: u64) {
        self.dropped = self.dropped.saturating_add(count);
    }

    /// Build one envelope without emitting it.
    ///
    /// The envelope leaves `scheduler_ref` empty; the VMM binds the
    /// deterministic scheduler position at collection time.
    #[allow(clippy::too_many_arguments)]
    pub fn envelope(
        &self,
        transition_class_ref: &str,
        boundary_ref: &str,
        projection: Option<&[u8]>,
        sequence: u64,
    ) -> Result<serde_json::Value, ProtocolObservationError> {
        if !valid_ref(transition_class_ref) || !valid_ref(boundary_ref) {
            return Err(ProtocolObservationError::InvalidReference);
        }
        let projection = projection.unwrap_or_default();
        if projection.len() > MAX_PROJECTION_BYTES {
            return Err(ProtocolObservationError::ProjectionTooLarge);
        }
        Ok(json!({
            "schema": PROTOCOL_OBSERVATION_EVENT,
            "profile_ref": self.profile_ref,
            "protocol_ref": self.protocol_ref,
            "projection_schema_ref": self.projection_schema_ref,
            "participant_ref": self.participant_ref,
            "producer_ref": self.producer_ref,
            "guest_id": self.guest_id,
            "generation": self.generation,
            "source_sequence": sequence,
            "transition_class_ref": transition_class_ref,
            "boundary_ref": boundary_ref,
            "projection_ref": format!("blake3:{}", blake3::hash(projection).to_hex()),
            "projection": serde_json::Value::Null,
            "scheduler_ref": "",
            "dropped": self.dropped,
        }))
    }

    /// Emit one observation and return its assigned source sequence.
    ///
    /// Source sequences begin at 1 and increase by one per emitted envelope;
    /// the cohort contract reserves 0 for "no observation".
    pub fn observe(
        &mut self,
        transition_class_ref: &str,
        boundary_ref: &str,
        projection: Option<&[u8]>,
    ) -> Result<u64, ProtocolObservationError> {
        let sequence = self.next_sequence;
        let envelope = match self.envelope(transition_class_ref, boundary_ref, projection, sequence)
        {
            Ok(envelope) => envelope,
            Err(error) => {
                self.record_dropped(1);
                return Err(error);
            }
        };
        let next = sequence
            .checked_add(1)
            .ok_or(ProtocolObservationError::SequenceOverflow)?;
        crate::lifecycle::send_event(PROTOCOL_OBSERVATION_EVENT, &envelope);
        self.next_sequence = next;
        Ok(sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REF: &str = "blake3:0000000000000000000000000000000000000000000000000000000000000001";

    fn producer() -> ProtocolObservationProducer {
        ProtocolObservationProducer::new(REF, REF, REF, REF, REF, 0, 1).unwrap()
    }

    #[test]
    fn envelopes_assign_sequences_and_account_losses() {
        let mut producer = producer();
        let first = producer.observe(REF, REF, Some(b"projection")).unwrap();
        let second = producer.observe(REF, REF, None).unwrap();
        assert_eq!((first, second), (1, 2));
        assert_eq!(producer.next_sequence(), 3);
        let envelope = producer.envelope(REF, REF, Some(b"x"), 3).unwrap();
        assert_eq!(envelope["source_sequence"], 3);
        assert_eq!(envelope["dropped"], 0);
        assert_eq!(
            envelope["projection_ref"],
            format!("blake3:{}", blake3::hash(b"x").to_hex()).as_str()
        );
        assert!(producer
            .observe(REF, REF, Some(&[0u8; MAX_PROJECTION_BYTES + 1]))
            .is_err());
        assert_eq!(producer.dropped(), 1);
        producer.record_dropped(2);
        assert_eq!(producer.dropped(), 3);
    }

    #[test]
    fn malformed_references_are_rejected() {
        assert!(matches!(
            ProtocolObservationProducer::new("not-a-ref", REF, REF, REF, REF, 0, 0),
            Err(ProtocolObservationError::InvalidReference)
        ));
        assert_eq!(
            producer().envelope("short", REF, None, 0),
            Err(ProtocolObservationError::InvalidReference)
        );
    }
}
