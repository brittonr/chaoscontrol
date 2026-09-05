use std::io::{self, Write};

pub const MAX_ORACLE_EVENTS: usize = 16_384;
pub const MAX_IDENTITY_CONFLICTS: usize = 64;
const MAX_EVENT_NAME_BYTES: usize = 256;
const MAX_IDENTITY_DIAGNOSTIC_BYTES: usize = 512;

pub(crate) fn validate_bounds(
    events: &[crate::oracle::OracleEvent],
    diagnostics: &[String],
    total_runs: u32,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    if events.len() > MAX_ORACLE_EVENTS || diagnostics.len() > MAX_IDENTITY_CONFLICTS {
        return Err(crate::oracle_validation::OracleValidationError::Cardinality);
    }
    for event in events {
        if event.run_id > total_runs
            || event.name.is_empty()
            || event.name.len() > MAX_EVENT_NAME_BYTES
        {
            return Err(crate::oracle_validation::OracleValidationError::Event);
        }
        let mut writer = BoundedWriter::new(
            ::chaoscontrol_protocol::identity::MAX_ASSERTION_EVENT_DETAILS_BYTES,
        );
        serde_json::to_writer(&mut writer, &event.details)
            .map_err(|_| crate::oracle_validation::OracleValidationError::Event)?;
    }
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.is_empty() || diagnostic.len() > MAX_IDENTITY_DIAGNOSTIC_BYTES)
    {
        return Err(crate::oracle_validation::OracleValidationError::ConflictState);
    }
    Ok(())
}

struct BoundedWriter {
    written: usize,
    maximum: usize,
}

impl BoundedWriter {
    fn new(maximum: usize) -> Self {
        Self {
            written: 0,
            maximum,
        }
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let next = self
            .written
            .checked_add(buffer.len())
            .ok_or_else(|| io::Error::other("JSON byte count overflow"))?;
        if next > self.maximum {
            return Err(io::Error::other("JSON exceeds byte limit"));
        }
        self.written = next;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
