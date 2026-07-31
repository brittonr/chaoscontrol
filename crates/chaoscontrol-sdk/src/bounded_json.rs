use chaoscontrol_protocol::assertion_identity::MAX_ASSERTION_EVENT_DETAILS_BYTES;
use std::io::{self, Write};

pub(crate) fn assertion_details(value: &serde_json::Value) -> Result<Vec<u8>, io::Error> {
    let mut writer = BoundedWriter::new(MAX_ASSERTION_EVENT_DETAILS_BYTES);
    serde_json::to_writer(&mut writer, value).map_err(io::Error::other)?;
    Ok(writer.into_bytes())
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}

impl BoundedWriter {
    fn new(maximum: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(maximum),
            maximum,
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let next_length = self
            .bytes
            .len()
            .checked_add(input.len())
            .ok_or_else(|| io::Error::other("assertion details length overflow"))?;
        if next_length > self.maximum {
            return Err(io::Error::other("assertion details exceed protocol limit"));
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_details_serialize() {
        let value = serde_json::json!({"term": 3});
        assert_eq!(
            assertion_details(&value).expect("details"),
            br#"{"term":3}"#
        );
    }

    #[test]
    fn oversized_details_fail_before_output_growth() {
        let value = serde_json::Value::String("x".repeat(MAX_ASSERTION_EVENT_DETAILS_BYTES));
        assert!(assertion_details(&value).is_err());
    }
}
