//! Bounded guest emission with explicit transport outcomes.

use chaoscontrol_protocol::protocol_observation::*;
use chaoscontrol_protocol::{encode_payload, CMD_PROTOCOL_OBSERVATION, PAYLOAD_MAX, STATUS_OK};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionPayload {
    CanonicalJson(Vec<u8>),
    ExternalReference(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerContext {
    pub marker_identity: String,
    pub parent_snapshot_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationEmissionInput {
    pub transition_class: String,
    pub logical_boundary_ref: String,
    pub projection: ProjectionPayload,
    pub drain_state: DrainState,
    pub marker: Option<MarkerContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationEmitError {
    CounterOverflow,
    Closed,
    PayloadTooLarge,
    Protocol(ProtocolObservationError),
    Serialization,
    Transport(u8),
    NoHostTransport,
}

/// The shell owns the selected transport. A local log is not a host acknowledgement.
pub trait ObservationTransport {
    fn send(&mut self, payload: &[u8]) -> Result<(), ObservationEmitError>;
}

struct GuestTransport;
impl ObservationTransport for GuestTransport {
    fn send(&mut self, payload: &[u8]) -> Result<(), ObservationEmitError> {
        if !crate::is_in_vm() {
            return Err(ObservationEmitError::NoHostTransport);
        }
        let (_, status) = crate::transport::hypercall_raw(CMD_PROTOCOL_OBSERVATION, 0, 0, payload);
        if status != STATUS_OK {
            return Err(ObservationEmitError::Transport(status));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ProtocolObservationEmitter {
    profile: AdmittedProfile,
    producer: ProducerProfile,
    next_sequence: u64,
    loss_count: u64,
    closed: bool,
}

impl ProtocolObservationEmitter {
    pub fn new(
        profile: AdmittedProfile,
        producer_ref: &str,
        execution_ref: &str,
    ) -> Result<Self, ObservationEmitError> {
        validate_profile_identity(&profile).map_err(ObservationEmitError::Protocol)?;
        if profile.profile.execution_ref != execution_ref {
            return Err(ObservationEmitError::Protocol(
                ProtocolObservationError::IdentityMismatch("execution"),
            ));
        }
        let producer = profile
            .profile
            .producers
            .iter()
            .find(|producer| producer.producer_ref == producer_ref)
            .cloned()
            .ok_or(ObservationEmitError::Protocol(
                ProtocolObservationError::UnknownProducer,
            ))?;
        Ok(Self {
            profile,
            producer,
            next_sequence: 0,
            loss_count: 0,
            closed: false,
        })
    }

    pub fn emit(
        &mut self,
        input: ObservationEmissionInput,
    ) -> Result<ObservationDraft, ObservationEmitError> {
        self.emit_with(input, &mut GuestTransport)
    }

    pub fn emit_with<T: ObservationTransport>(
        &mut self,
        input: ObservationEmissionInput,
        transport: &mut T,
    ) -> Result<ObservationDraft, ObservationEmitError> {
        let (next_sequence, next_loss) = self.reserve_attempt()?;
        let draft = self.prepare(input)?;
        let json = serde_json::to_vec(&draft).map_err(|_| ObservationEmitError::Serialization)?;
        let mut payload = [0_u8; PAYLOAD_MAX];
        let length = encode_payload(&mut payload, PROTOCOL_OBSERVATION_EVENT, &json)
            .ok_or(ObservationEmitError::PayloadTooLarge)?;
        // Reserve counters before the effect. A failed send consumes its sequence.
        self.next_sequence = next_sequence;
        self.closed = draft.drain_state == DrainState::Final;
        if let Err(error) = transport.send(&payload[..length]) {
            self.loss_count = next_loss;
            return Err(error);
        }
        Ok(draft)
    }

    fn reserve_attempt(&self) -> Result<(u64, u64), ObservationEmitError> {
        if self.closed {
            return Err(ObservationEmitError::Closed);
        }
        if self.next_sequence >= u64::from(self.profile.profile.bounds.max_records_per_producer) {
            return Err(ObservationEmitError::Protocol(
                ProtocolObservationError::BoundExceeded("source-records"),
            ));
        }
        Ok((
            self.next_sequence
                .checked_add(1)
                .ok_or(ObservationEmitError::CounterOverflow)?,
            self.loss_count
                .checked_add(1)
                .ok_or(ObservationEmitError::CounterOverflow)?,
        ))
    }

    fn prepare(
        &self,
        input: ObservationEmissionInput,
    ) -> Result<ObservationDraft, ObservationEmitError> {
        let (projection_ref, projection_bytes) = match input.projection {
            ProjectionPayload::CanonicalJson(bytes) => {
                if bytes.len() > self.profile.profile.bounds.max_projection_bytes as usize {
                    return Err(ObservationEmitError::PayloadTooLarge);
                }
                (projection_identity(&bytes), Some(bytes))
            }
            ProjectionPayload::ExternalReference(reference) => (reference, None),
        };
        let novelty_identity = novelty_identity(
            &self.profile,
            &projection_ref,
            &input.logical_boundary_ref,
            &input.transition_class,
        );
        let draft = ObservationDraft {
            schema: DRAFT_SCHEMA.into(),
            profile_ref: self.profile.profile_ref.clone(),
            protocol_ref: self.profile.profile.protocol_ref.clone(),
            cohort_ref: self.profile.profile.cohort_ref.clone(),
            producer_ref: self.producer.producer_ref.clone(),
            participant_ref: self.producer.participant_ref.clone(),
            process_ref: self.producer.process_ref.clone(),
            execution_ref: self.profile.profile.execution_ref.clone(),
            generation: self.producer.admitted_generation,
            source_sequence: self.next_sequence,
            source_loss_count: self.loss_count,
            drain_state: input.drain_state,
            transition_class: input.transition_class,
            logical_boundary_ref: input.logical_boundary_ref,
            projection_schema_ref: self.profile.profile.projection_schema_ref.clone(),
            projection_ref,
            projection_bytes,
            novelty_identity,
            marker_identity: input
                .marker
                .as_ref()
                .map(|marker| marker.marker_identity.clone()),
            parent_snapshot_ref: input.marker.and_then(|marker| marker.parent_snapshot_ref),
        };
        validate_observation_draft(&self.profile, &draft)
            .map_err(ObservationEmitError::Protocol)?;
        Ok(draft)
    }

    pub fn record_source_loss(&mut self) -> Result<(), ObservationEmitError> {
        let (next_sequence, next_loss) = self.reserve_attempt()?;
        self.next_sequence = next_sequence;
        self.loss_count = next_loss;
        Ok(())
    }

    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }
    pub fn loss_count(&self) -> u64 {
        self.loss_count
    }
}
