use crate::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey, IdentityError,
    ASSERTION_DESCRIPTOR_DOMAIN, ASSERTION_FINGERPRINT_BYTES, ASSERTION_IDENTITY_VERSION,
    MAX_ASSERTION_CANONICAL_BYTES, MAX_ASSERTION_CATEGORY_BYTES, MAX_ASSERTION_EVENT_DETAILS_BYTES,
    MAX_ASSERTION_GUEST_BYTES, MAX_ASSERTION_KEY_BYTES, MAX_ASSERTION_MESSAGE_BYTES,
    MAX_ASSERTION_NAMESPACE_BYTES, MAX_ASSERTION_SOURCE_BYTES,
};

pub const ASSERTION_WIRE_VERSION: u8 = 1;
pub const CATALOG_BEGIN_PAYLOAD_BYTES: usize = 2;
pub const CATALOG_COMPLETE_PAYLOAD_BYTES: usize = 1 + ASSERTION_FINGERPRINT_BYTES;
const WIRE_VERSION_OFFSET: usize = 0;
const EVENT_CATALOG_TOKEN_OFFSET: usize = WIRE_VERSION_OFFSET + 1;
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
    output[0] = ASSERTION_WIRE_VERSION;
    output[1] = ASSERTION_IDENTITY_VERSION;
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
    let descriptor = decode_canonical_descriptor(canonical_bytes)?;
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
    output[0] = ASSERTION_WIRE_VERSION;
    output[1..CATALOG_COMPLETE_PAYLOAD_BYTES].copy_from_slice(&token.0);
    Ok(CATALOG_COMPLETE_PAYLOAD_BYTES)
}

pub fn decode_catalog_complete(input: &[u8]) -> Result<AssertionFingerprint, IdentityError> {
    if input.len() != CATALOG_COMPLETE_PAYLOAD_BYTES || input[0] != ASSERTION_WIRE_VERSION {
        return Err(IdentityError::MalformedCanonical);
    }
    let mut token = [0_u8; ASSERTION_FINGERPRINT_BYTES];
    token.copy_from_slice(&input[1..]);
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

fn decode_canonical_descriptor(input: &[u8]) -> Result<AssertionDescriptor, IdentityError> {
    const FIELD_COUNT: u8 = 10;
    if input.len() > MAX_ASSERTION_CANONICAL_BYTES
        || !input.starts_with(ASSERTION_DESCRIPTOR_DOMAIN)
    {
        return Err(IdentityError::MalformedCanonical);
    }
    let mut cursor = ASSERTION_DESCRIPTOR_DOMAIN.len();
    let version = take_byte(input, &mut cursor)?;
    let field_count = take_byte(input, &mut cursor)?;
    if version != ASSERTION_IDENTITY_VERSION || field_count != FIELD_COUNT {
        return Err(IdentityError::InvalidVersion);
    }
    let namespace = take_string(input, &mut cursor, 1, MAX_ASSERTION_NAMESPACE_BYTES)?;
    let logical_key = decode_logical_key(take_field(
        input,
        &mut cursor,
        2,
        MAX_ASSERTION_KEY_BYTES + 1,
    )?)?;
    let kind_field = take_field(input, &mut cursor, 3, 1)?;
    if kind_field.len() != 1 {
        return Err(IdentityError::InvalidKind);
    }
    let kind = match kind_field[0] {
        0 => AssertionKind::Always,
        1 => AssertionKind::Sometimes,
        2 => AssertionKind::Reachable,
        3 => AssertionKind::Unreachable,
        _ => return Err(IdentityError::InvalidKind),
    };
    let message = take_string(input, &mut cursor, 4, MAX_ASSERTION_MESSAGE_BYTES)?;
    let source_file = take_string(input, &mut cursor, 5, MAX_ASSERTION_SOURCE_BYTES)?;
    let source_line = take_u32(input, &mut cursor, 6)?;
    let source_column = take_u32(input, &mut cursor, 7)?;
    let guest = take_string(input, &mut cursor, 8, MAX_ASSERTION_GUEST_BYTES)?;
    let category = take_string(input, &mut cursor, 9, MAX_ASSERTION_CATEGORY_BYTES)?;
    let compatibility_id = take_optional_u32(input, &mut cursor, 10)?;
    if cursor != input.len() {
        return Err(IdentityError::MalformedCanonical);
    }
    let descriptor = AssertionDescriptor {
        identity_version: version,
        namespace,
        logical_key,
        compatibility_id,
        kind,
        message,
        source_file,
        source_line,
        source_column,
        guest,
        category,
    };
    descriptor.validate()?;
    Ok(descriptor)
}

fn decode_logical_key(input: &[u8]) -> Result<AssertionLogicalKey, IdentityError> {
    let (tag, value) = input
        .split_first()
        .ok_or(IdentityError::MalformedCanonical)?;
    match *tag {
        1 => Ok(AssertionLogicalKey::Automatic {
            source_site: decode_string(value, MAX_ASSERTION_KEY_BYTES)?,
        }),
        2 => Ok(AssertionLogicalKey::Stable {
            key: decode_string(value, MAX_ASSERTION_KEY_BYTES)?,
        }),
        3 if value.len() == 4 => Ok(AssertionLogicalKey::LegacyU32 {
            id: u32::from_le_bytes(
                value
                    .try_into()
                    .map_err(|_| IdentityError::MalformedCanonical)?,
            ),
        }),
        _ => Err(IdentityError::MalformedCanonical),
    }
}

fn take_string(
    input: &[u8],
    cursor: &mut usize,
    tag: u8,
    maximum: usize,
) -> Result<String, IdentityError> {
    decode_string(take_field(input, cursor, tag, maximum)?, maximum)
}

fn decode_string(input: &[u8], maximum: usize) -> Result<String, IdentityError> {
    if input.len() > maximum {
        return Err(IdentityError::MalformedCanonical);
    }
    String::from_utf8(input.to_vec()).map_err(|_| IdentityError::MalformedCanonical)
}

fn take_optional_u32(
    input: &[u8],
    cursor: &mut usize,
    tag: u8,
) -> Result<Option<u32>, IdentityError> {
    let bytes = take_field(input, cursor, tag, 4)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() != 4 {
        return Err(IdentityError::MalformedCanonical);
    }
    Ok(Some(u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| IdentityError::MalformedCanonical)?,
    )))
}

fn take_u32(input: &[u8], cursor: &mut usize, tag: u8) -> Result<u32, IdentityError> {
    let bytes = take_field(input, cursor, tag, 4)?;
    if bytes.len() != 4 {
        return Err(IdentityError::MalformedCanonical);
    }
    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| IdentityError::MalformedCanonical)?,
    ))
}

fn take_field<'a>(
    input: &'a [u8],
    cursor: &mut usize,
    expected_tag: u8,
    maximum: usize,
) -> Result<&'a [u8], IdentityError> {
    let tag = take_byte(input, cursor)?;
    if tag != expected_tag || input.len().saturating_sub(*cursor) < 2 {
        return Err(IdentityError::MalformedCanonical);
    }
    let length = u16::from_le_bytes([input[*cursor], input[*cursor + 1]]) as usize;
    *cursor += 2;
    if length > maximum || input.len().saturating_sub(*cursor) < length {
        return Err(IdentityError::MalformedCanonical);
    }
    let value = &input[*cursor..*cursor + length];
    *cursor += length;
    Ok(value)
}

fn take_byte(input: &[u8], cursor: &mut usize) -> Result<u8, IdentityError> {
    let value = input
        .get(*cursor)
        .copied()
        .ok_or(IdentityError::MalformedCanonical)?;
    *cursor += 1;
    Ok(value)
}
