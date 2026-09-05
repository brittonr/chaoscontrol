//! Profile-bound collection separate from free-form oracle events.

use chaoscontrol_protocol::protocol_observation::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collection {
    profile: Option<ProtocolObservationProfile>,
    records: Vec<CollectedObservation>,
    rejected: u64,
}

impl Collection {
    pub fn receive(
        &mut self,
        page: &chaoscontrol_protocol::HypercallPage,
        position: Option<SchedulerPosition>,
    ) -> (u64, u8) {
        use chaoscontrol_protocol::{STATUS_ERROR, STATUS_OK};
        let record = match decode(page, position) {
            Ok(record) => record,
            Err(_) => {
                self.reject();
                return (0, STATUS_ERROR);
            }
        };
        match self.collect(record) {
            Ok(()) => (0, STATUS_OK),
            Err(_) => (0, STATUS_ERROR),
        }
    }

    pub fn configure<O: ProtocolOracle + ?Sized>(
        &mut self,
        profile: AdmittedProfile,
        oracle: &O,
    ) -> Result<(), ProtocolObservationError> {
        validate_oracle_adapter(&profile, oracle)?;
        if self.profile.is_some() || !self.records.is_empty() || self.rejected != 0 {
            return Err(ProtocolObservationError::IdentityMismatch(
                "collection-profile",
            ));
        }
        self.profile = Some(profile.profile);
        Ok(())
    }

    pub fn collect(
        &mut self,
        record: CollectedObservation,
    ) -> Result<(), ProtocolObservationError> {
        if let Err(error) = self.preflight(&record) {
            self.reject();
            return Err(error);
        }
        self.records.push(record);
        Ok(())
    }

    fn preflight(&self, record: &CollectedObservation) -> Result<(), ProtocolObservationError> {
        let profile = self.admitted_profile()?;
        validate_collected_observation(&profile, record)?;
        bounded_record(record)?;
        if self.records.len() >= profile.profile.bounds.max_cohort_backlog as usize {
            return Err(ProtocolObservationError::BoundExceeded(
                "collection-records",
            ));
        }
        let mut bytes = 0_u64;
        let mut producer_count = 0;
        let mut boundaries = std::collections::BTreeSet::new();
        for item in self.records.iter().chain(std::iter::once(record)) {
            if item.draft.producer_ref == record.draft.producer_ref {
                producer_count += 1;
            }
            boundaries.insert(&item.draft.logical_boundary_ref);
            let length = item.draft.projection_bytes.as_ref().map_or(0, Vec::len);
            bytes = bytes
                .checked_add(
                    u64::try_from(length)
                        .map_err(|_| ProtocolObservationError::CardinalityOverflow)?,
                )
                .ok_or(ProtocolObservationError::CardinalityOverflow)?;
        }
        if producer_count > profile.profile.bounds.max_records_per_producer
            || boundaries.len() > profile.profile.bounds.max_logical_boundaries as usize
            || bytes > profile.profile.bounds.max_total_projection_bytes
        {
            return Err(ProtocolObservationError::BoundExceeded("collection-budget"));
        }
        Ok(())
    }

    pub fn admit_snapshot(&self, snapshot: &Self) -> Result<(), ProtocolObservationError> {
        if self.profile != snapshot.profile {
            return Err(ProtocolObservationError::IdentityMismatch(
                "snapshot-profile",
            ));
        }
        snapshot.validate()
    }

    pub fn validate(&self) -> Result<(), ProtocolObservationError> {
        if self.profile.is_none() {
            if self.records.is_empty() {
                return Ok(());
            }
            return Err(ProtocolObservationError::IdentityMismatch(
                "missing-profile",
            ));
        }
        let profile = self.admitted_profile()?;
        if self.records.len() > profile.profile.bounds.max_cohort_backlog as usize {
            return Err(ProtocolObservationError::BoundExceeded(
                "collection-records",
            ));
        }
        let mut rebuilt = Self {
            profile: self.profile.clone(),
            records: Vec::new(),
            rejected: 0,
        };
        for record in &self.records {
            rebuilt.preflight(record)?;
            rebuilt.records.push(record.clone());
        }
        Ok(())
    }

    pub fn cohort(
        &self,
        boundary: &str,
        support: ProjectionSupport,
    ) -> Result<CohortResult, ProtocolObservationError> {
        self.validate()?;
        assemble_with_losses(
            &self.admitted_profile()?,
            boundary,
            &self.records,
            support,
            self.rejected,
        )
    }

    pub fn admitted_profile(&self) -> Result<AdmittedProfile, ProtocolObservationError> {
        let raw = self
            .profile
            .clone()
            .ok_or(ProtocolObservationError::IdentityMismatch(
                "missing-profile",
            ))?;
        admit_profile(raw)
    }

    /// Rejections are sticky. Saturation cannot restore a loss-free state.
    pub fn reject(&mut self) {
        self.rejected = self.rejected.saturating_add(1);
    }
    pub fn is_pristine(&self) -> bool {
        self.profile.is_none() && self.records.is_empty() && self.rejected == 0
    }
    pub fn records(&self) -> &[CollectedObservation] {
        &self.records
    }
    pub fn rejected(&self) -> u64 {
        self.rejected
    }
}

fn decode(
    page: &chaoscontrol_protocol::HypercallPage,
    position: Option<SchedulerPosition>,
) -> Result<CollectedObservation, ProtocolObservationError> {
    use chaoscontrol_protocol::{
        decode_payload, encode_payload, CMD_PROTOCOL_OBSERVATION, PAYLOAD_MAX,
    };
    let position = position.ok_or(ProtocolObservationError::IdentityMismatch(
        "scheduler-position",
    ))?;
    if page.command != CMD_PROTOCOL_OBSERVATION || page.flags != 0 || page.id != 0 {
        return Err(ProtocolObservationError::InvalidSchema);
    }
    let payload = page
        .payload
        .get(..usize::from(page.payload_len))
        .ok_or(ProtocolObservationError::BoundExceeded("payload"))?;
    let decoded = decode_payload(payload).ok_or(ProtocolObservationError::InvalidSchema)?;
    let mut canonical = [0_u8; PAYLOAD_MAX];
    let length = encode_payload(&mut canonical, &decoded.message, &decoded.json_details)
        .ok_or(ProtocolObservationError::InvalidSchema)?;
    if decoded.message != PROTOCOL_OBSERVATION_EVENT || canonical[..length] != *payload {
        return Err(ProtocolObservationError::InvalidSchema);
    }
    let draft = serde_json::from_slice(&decoded.json_details)
        .map_err(|_| ProtocolObservationError::InvalidSchema)?;
    bind_scheduler_position(draft, position)
}
