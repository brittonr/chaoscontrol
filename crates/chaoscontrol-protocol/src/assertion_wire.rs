use crate::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, IdentityError,
    ASSERTION_FINGERPRINT_BYTES, ASSERTION_IDENTITY_VERSION, MAX_ASSERTION_CANONICAL_BYTES,
    MAX_ASSERTION_EVENT_DETAILS_BYTES,
};

pub const ASSERTION_WIRE_VERSION: u8 = 1;
const VERSION_FIELD_BYTES: usize = 1;
const WIRE_VERSION_OFFSET: usize = 0;
const IDENTITY_VERSION_OFFSET: usize = WIRE_VERSION_OFFSET + VERSION_FIELD_BYTES;
const CATALOG_TOKEN_OFFSET: usize = WIRE_VERSION_OFFSET + VERSION_FIELD_BYTES;
pub const CATALOG_BEGIN_PAYLOAD_BYTES: usize = IDENTITY_VERSION_OFFSET + VERSION_FIELD_BYTES;
pub const CATALOG_COMPLETE_PAYLOAD_BYTES: usize =
    CATALOG_TOKEN_OFFSET + ASSERTION_FINGERPRINT_BYTES;
const EVENT_CATALOG_TOKEN_OFFSET: usize = WIRE_VERSION_OFFSET + VERSION_FIELD_BYTES;
const EVENT_FINGERPRINT_OFFSET: usize = EVENT_CATALOG_TOKEN_OFFSET + ASSERTION_FINGERPRINT_BYTES;
pub const EVENT_KIND_OFFSET: usize = EVENT_FINGERPRINT_OFFSET + ASSERTION_FINGERPRINT_BYTES;
pub const EVENT_BINDING_BYTES: usize = EVENT_KIND_OFFSET + 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptorFrame {
    pub fingerprint: AssertionFingerprint,
    pub descriptor: AssertionDescriptor,
    pub canonical_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventFrame {
    pub catalog_token: AssertionFingerprint,
    pub fingerprint: AssertionFingerprint,
    pub kind: AssertionKind,
    pub details: Vec<u8>,
}

pub fn encode_catalog_begin(output: &mut [u8]) -> Result<usize, IdentityError> {
    if output.len() < CATALOG_BEGIN_PAYLOAD_BYTES {
        return Err(IdentityError::MalformedCanonical);
    }
    output[WIRE_VERSION_OFFSET] = ASSERTION_WIRE_VERSION;
    output[IDENTITY_VERSION_OFFSET] = ASSERTION_IDENTITY_VERSION;
    Ok(CATALOG_BEGIN_PAYLOAD_BYTES)
}

pub fn decode_catalog_begin(input: &[u8]) -> Result<(), IdentityError> {
    if input != [ASSERTION_WIRE_VERSION, ASSERTION_IDENTITY_VERSION] {
        return Err(IdentityError::InvalidVersion);
    }
    Ok(())
}

pub fn encode_descriptor_frame(
    descriptor: &AssertionDescriptor,
    output: &mut [u8],
) -> Result<usize, IdentityError> {
    let canonical = descriptor.canonical_bytes()?;
    let fingerprint = descriptor.fingerprint()?;
    let required = ASSERTION_FINGERPRINT_BYTES.saturating_add(canonical.len());
    if required > output.len() {
        return Err(IdentityError::FieldTooLong("descriptor_frame"));
    }
    output[..ASSERTION_FINGERPRINT_BYTES].copy_from_slice(&fingerprint.0);
    output[ASSERTION_FINGERPRINT_BYTES..required].copy_from_slice(&canonical);
    Ok(required)
}

pub fn decode_descriptor_frame(input: &[u8]) -> Result<DescriptorFrame, IdentityError> {
    if input.len() < ASSERTION_FINGERPRINT_BYTES
        || input.len() > ASSERTION_FINGERPRINT_BYTES + MAX_ASSERTION_CANONICAL_BYTES
    {
        return Err(IdentityError::MalformedCanonical);
    }
    let mut claimed = [0_u8; ASSERTION_FINGERPRINT_BYTES];
    claimed.copy_from_slice(&input[..ASSERTION_FINGERPRINT_BYTES]);
    let canonical_bytes = &input[ASSERTION_FINGERPRINT_BYTES..];
    let descriptor = crate::assertion_wire_decode::decode_canonical_descriptor(canonical_bytes)?;
    let fingerprint = descriptor.fingerprint()?;
    if fingerprint.0 != claimed {
        return Err(IdentityError::InvalidFingerprint);
    }
    Ok(DescriptorFrame {
        fingerprint,
        descriptor,
        canonical_bytes: canonical_bytes.to_vec(),
    })
}

pub fn encode_catalog_complete(
    token: AssertionFingerprint,
    output: &mut [u8],
) -> Result<usize, IdentityError> {
    if output.len() < CATALOG_COMPLETE_PAYLOAD_BYTES {
        return Err(IdentityError::MalformedCanonical);
    }
    output[WIRE_VERSION_OFFSET] = ASSERTION_WIRE_VERSION;
    output[CATALOG_TOKEN_OFFSET..CATALOG_COMPLETE_PAYLOAD_BYTES].copy_from_slice(&token.0);
    Ok(CATALOG_COMPLETE_PAYLOAD_BYTES)
}

pub fn decode_catalog_complete(input: &[u8]) -> Result<AssertionFingerprint, IdentityError> {
    if input.len() != CATALOG_COMPLETE_PAYLOAD_BYTES
        || input[WIRE_VERSION_OFFSET] != ASSERTION_WIRE_VERSION
    {
        return Err(IdentityError::MalformedCanonical);
    }
    let mut token = [0_u8; ASSERTION_FINGERPRINT_BYTES];
    token.copy_from_slice(&input[CATALOG_TOKEN_OFFSET..]);
    Ok(AssertionFingerprint(token))
}

pub fn encode_event_frame(frame: &EventFrame, output: &mut [u8]) -> Result<usize, IdentityError> {
    if frame.details.len() > MAX_ASSERTION_EVENT_DETAILS_BYTES {
        return Err(IdentityError::FieldTooLong("event_details"));
    }
    let required = EVENT_BINDING_BYTES.saturating_add(frame.details.len());
    if required > output.len() {
        return Err(IdentityError::FieldTooLong("event_frame"));
    }
    output[WIRE_VERSION_OFFSET] = ASSERTION_WIRE_VERSION;
    output[EVENT_CATALOG_TOKEN_OFFSET..EVENT_FINGERPRINT_OFFSET]
        .copy_from_slice(&frame.catalog_token.0);
    output[EVENT_FINGERPRINT_OFFSET..EVENT_KIND_OFFSET].copy_from_slice(&frame.fingerprint.0);
    output[EVENT_KIND_OFFSET] = frame.kind as u8;
    output[EVENT_BINDING_BYTES..required].copy_from_slice(&frame.details);
    Ok(required)
}

pub fn decode_event_frame(input: &[u8], kind: AssertionKind) -> Result<EventFrame, IdentityError> {
    if input.len() < EVENT_BINDING_BYTES
        || input.len() > EVENT_BINDING_BYTES + MAX_ASSERTION_EVENT_DETAILS_BYTES
        || input[WIRE_VERSION_OFFSET] != ASSERTION_WIRE_VERSION
    {
        return Err(IdentityError::MalformedCanonical);
    }
    let mut catalog_token = [0_u8; ASSERTION_FINGERPRINT_BYTES];
    let mut fingerprint = [0_u8; ASSERTION_FINGERPRINT_BYTES];
    catalog_token.copy_from_slice(&input[EVENT_CATALOG_TOKEN_OFFSET..EVENT_FINGERPRINT_OFFSET]);
    fingerprint.copy_from_slice(&input[EVENT_FINGERPRINT_OFFSET..EVENT_KIND_OFFSET]);
    if input[EVENT_KIND_OFFSET] != kind as u8 {
        return Err(IdentityError::InvalidKind);
    }
    Ok(EventFrame {
        catalog_token: AssertionFingerprint(catalog_token),
        fingerprint: AssertionFingerprint(fingerprint),
        kind,
        details: input[EVENT_BINDING_BYTES..].to_vec(),
    })
}
