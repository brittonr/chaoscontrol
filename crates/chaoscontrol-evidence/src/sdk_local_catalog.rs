use crate::{EvidenceError, EvidenceResult};
use chaoscontrol_protocol::admission::{
    AcceptedCatalog, CatalogBuilder, ASSERTION_CATALOG_VERSION, MAX_ASSERTION_CATALOG_ENTRIES,
};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionFingerprint, MAX_ASSERTION_CANONICAL_BYTES,
};

const HEX_CHARACTERS_PER_BYTE: usize = 2;
const HEX_HIGH_NIBBLE_SHIFT: u32 = 4;
const HEX_ALPHA_DIGIT_OFFSET: u8 = 10;
const MAX_CANONICAL_HEX_BYTES: usize = MAX_ASSERTION_CANONICAL_BYTES * HEX_CHARACTERS_PER_BYTE;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogEnvelope {
    chaoscontrol_assertion_catalog: CatalogRecord,
}

#[derive(serde::Deserialize)]
#[serde(tag = "record", rename_all = "snake_case", deny_unknown_fields)]
enum CatalogRecord {
    Begin {
        catalog_version: u8,
        expected_descriptors: usize,
        valid: bool,
    },
    Descriptor {
        fingerprint: AssertionFingerprint,
        descriptor: AssertionDescriptor,
        canonical_descriptor: String,
    },
    Complete {
        catalog_version: u8,
        descriptor_count: usize,
        catalog_token: AssertionFingerprint,
    },
    Conflict {
        error: String,
    },
}

pub(crate) fn apply_catalog_line(
    line: &str,
    line_index: usize,
    builder: &mut Option<CatalogBuilder>,
    accepted: &mut Option<AcceptedCatalog>,
) -> EvidenceResult<()> {
    let envelope: CatalogEnvelope = serde_json::from_str(line)
        .map_err(|error| EvidenceError::new(format!("line {}: {error}", line_index + 1)))?;
    match envelope.chaoscontrol_assertion_catalog {
        CatalogRecord::Begin {
            catalog_version,
            expected_descriptors,
            valid,
        } => {
            if catalog_version != ASSERTION_CATALOG_VERSION || !valid {
                return line_error(line_index, "invalid catalog begin record");
            }
            if builder.is_some() || accepted.is_some() {
                return line_error(line_index, "duplicate catalog begin record");
            }
            if expected_descriptors > MAX_ASSERTION_CATALOG_ENTRIES {
                return line_error(line_index, "catalog cardinality exceeds the limit");
            }
            *builder = Some(
                CatalogBuilder::begin(expected_descriptors).map_err(|error| {
                    EvidenceError::new(format!("line {}: {error:?}", line_index + 1))
                })?,
            );
        }
        CatalogRecord::Descriptor {
            fingerprint,
            descriptor,
            canonical_descriptor,
        } => {
            let canonical = decode_hex(&canonical_descriptor, line_index)?;
            let expected = descriptor.canonical_bytes().map_err(|error| {
                EvidenceError::new(format!("invalid assertion descriptor: {error}"))
            })?;
            let expected_fingerprint = descriptor.fingerprint().map_err(|error| {
                EvidenceError::new(format!("invalid assertion fingerprint: {error}"))
            })?;
            if expected_fingerprint != fingerprint {
                return line_error(line_index, "descriptor fingerprint does not match");
            }
            if expected != canonical {
                return line_error(line_index, "canonical descriptor bytes do not match");
            }
            let Some(pending) = builder.as_mut() else {
                return line_error(line_index, "descriptor before catalog begin");
            };
            pending
                .insert_with_fingerprint(descriptor, fingerprint)
                .map_err(|error| {
                    EvidenceError::new(format!("line {}: {error:?}", line_index + 1))
                })?;
        }
        CatalogRecord::Complete {
            catalog_version,
            descriptor_count,
            catalog_token,
        } => {
            if catalog_version != ASSERTION_CATALOG_VERSION
                || descriptor_count > MAX_ASSERTION_CATALOG_ENTRIES
            {
                return line_error(line_index, "invalid catalog complete record");
            }
            let Some(pending) = builder.as_ref() else {
                return line_error(line_index, "catalog completion without begin");
            };
            if descriptor_count != pending.expected_frames()
                || descriptor_count != pending.received_frames()
            {
                return line_error(line_index, "catalog descriptor count mismatch");
            }
            let pending = builder.take().expect("pending catalog was checked");
            *accepted = Some(pending.complete(catalog_token).map_err(|error| {
                EvidenceError::new(format!("line {}: {error:?}", line_index + 1))
            })?);
        }
        CatalogRecord::Conflict { error } => {
            return line_error(
                line_index,
                &format!("SDK reported catalog conflict: {error}"),
            );
        }
    }
    Ok(())
}

fn decode_hex(value: &str, line_index: usize) -> EvidenceResult<Vec<u8>> {
    if value.len() > MAX_CANONICAL_HEX_BYTES {
        return line_error(
            line_index,
            "canonical descriptor hex exceeds the byte limit",
        );
    }
    if !value.len().is_multiple_of(HEX_CHARACTERS_PER_BYTE) {
        return line_error(line_index, "canonical descriptor hex length is invalid");
    }
    let mut output = Vec::with_capacity(value.len() / HEX_CHARACTERS_PER_BYTE);
    for pair in value.as_bytes().as_chunks::<HEX_CHARACTERS_PER_BYTE>().0 {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        output.push((high << HEX_HIGH_NIBBLE_SHIFT) | low);
    }
    Ok(output)
}

fn hex_nibble(value: u8) -> EvidenceResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + HEX_ALPHA_DIGIT_OFFSET),
        _ => Err(EvidenceError::new(
            "canonical descriptor contains invalid hex",
        )),
    }
}

fn line_error<T>(line_index: usize, message: &str) -> EvidenceResult<T> {
    Err(EvidenceError::new(format!(
        "line {}: {message}",
        line_index + 1
    )))
}
