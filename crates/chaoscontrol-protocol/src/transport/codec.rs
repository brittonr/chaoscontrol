const FIELD_COUNT: u8 = 10;
const NAMESPACE_FIELD_TAG: u8 = 1;
const LOGICAL_KEY_FIELD_TAG: u8 = 2;
const KIND_FIELD_TAG: u8 = 3;
const MESSAGE_FIELD_TAG: u8 = 4;
const SOURCE_FILE_FIELD_TAG: u8 = 5;
const SOURCE_LINE_FIELD_TAG: u8 = 6;
const SOURCE_COLUMN_FIELD_TAG: u8 = 7;
const GUEST_FIELD_TAG: u8 = 8;
const CATEGORY_FIELD_TAG: u8 = 9;
const COMPATIBILITY_ID_FIELD_TAG: u8 = 10;
const AUTOMATIC_KEY_DISCRIMINANT: u8 = 1;
const STABLE_KEY_DISCRIMINANT: u8 = 2;
const LEGACY_KEY_DISCRIMINANT: u8 = 3;
const KIND_FIELD_BYTES: usize = 1;
const U32_FIELD_BYTES: usize = core::mem::size_of::<u32>();
const FIELD_LENGTH_BYTES: usize = core::mem::size_of::<u16>();
const LOGICAL_KEY_DISCRIMINANT_BYTES: usize = 1;

pub(crate) fn decode_canonical_descriptor(
    input: &[u8],
) -> Result<crate::identity::AssertionDescriptor, crate::identity::AssertionError> {
    if input.len() > crate::identity::MAX_ASSERTION_CANONICAL_BYTES
        || !input.starts_with(crate::identity::ASSERTION_DESCRIPTOR_DOMAIN)
    {
        return Err(crate::identity::AssertionError::MalformedCanonical);
    }
    let mut cursor = crate::identity::ASSERTION_DESCRIPTOR_DOMAIN.len();
    let version = take_byte(input, &mut cursor)?;
    let field_count = take_byte(input, &mut cursor)?;
    if version != crate::identity::ASSERTION_IDENTITY_VERSION || field_count != FIELD_COUNT {
        return Err(crate::identity::AssertionError::InvalidVersion);
    }
    let namespace = take_string(
        input,
        &mut cursor,
        NAMESPACE_FIELD_TAG,
        crate::identity::MAX_ASSERTION_NAMESPACE_BYTES,
    )?;
    let logical_key = decode_logical_key(take_field(
        input,
        &mut cursor,
        LOGICAL_KEY_FIELD_TAG,
        crate::identity::MAX_ASSERTION_KEY_BYTES + LOGICAL_KEY_DISCRIMINANT_BYTES,
    )?)?;
    let kind_field = take_field(input, &mut cursor, KIND_FIELD_TAG, KIND_FIELD_BYTES)?;
    if kind_field.len() != KIND_FIELD_BYTES {
        return Err(crate::identity::AssertionError::InvalidKind);
    }
    let kind = match kind_field[0] {
        crate::identity::ASSERTION_KIND_ALWAYS_DISCRIMINANT => {
            crate::identity::AssertionKind::Always
        }
        crate::identity::ASSERTION_KIND_SOMETIMES_DISCRIMINANT => {
            crate::identity::AssertionKind::Sometimes
        }
        crate::identity::ASSERTION_KIND_REACHABLE_DISCRIMINANT => {
            crate::identity::AssertionKind::Reachable
        }
        crate::identity::ASSERTION_KIND_UNREACHABLE_DISCRIMINANT => {
            crate::identity::AssertionKind::Unreachable
        }
        _ => return Err(crate::identity::AssertionError::InvalidKind),
    };
    let message = take_string(
        input,
        &mut cursor,
        MESSAGE_FIELD_TAG,
        crate::identity::MAX_ASSERTION_MESSAGE_BYTES,
    )?;
    let source_file = take_string(
        input,
        &mut cursor,
        SOURCE_FILE_FIELD_TAG,
        crate::identity::MAX_ASSERTION_SOURCE_BYTES,
    )?;
    let source_line = take_u32(input, &mut cursor, SOURCE_LINE_FIELD_TAG)?;
    let source_column = take_u32(input, &mut cursor, SOURCE_COLUMN_FIELD_TAG)?;
    let guest = take_string(
        input,
        &mut cursor,
        GUEST_FIELD_TAG,
        crate::identity::MAX_ASSERTION_GUEST_BYTES,
    )?;
    let category = take_string(
        input,
        &mut cursor,
        CATEGORY_FIELD_TAG,
        crate::identity::MAX_ASSERTION_CATEGORY_BYTES,
    )?;
    let compatibility_id = take_optional_u32(input, &mut cursor, COMPATIBILITY_ID_FIELD_TAG)?;
    if cursor != input.len() {
        return Err(crate::identity::AssertionError::MalformedCanonical);
    }
    let descriptor = crate::identity::AssertionDescriptor {
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

fn decode_logical_key(
    input: &[u8],
) -> Result<crate::identity::AssertionLogicalKey, crate::identity::AssertionError> {
    let (tag, value) = input
        .split_first()
        .ok_or(crate::identity::AssertionError::MalformedCanonical)?;
    match *tag {
        AUTOMATIC_KEY_DISCRIMINANT => Ok(crate::identity::AssertionLogicalKey::Automatic {
            source_site: decode_string(value, crate::identity::MAX_ASSERTION_KEY_BYTES)?,
        }),
        STABLE_KEY_DISCRIMINANT => Ok(crate::identity::AssertionLogicalKey::Stable {
            key: decode_string(value, crate::identity::MAX_ASSERTION_KEY_BYTES)?,
        }),
        LEGACY_KEY_DISCRIMINANT if value.len() == U32_FIELD_BYTES => {
            Ok(crate::identity::AssertionLogicalKey::LegacyU32 {
                id: u32::from_le_bytes(
                    value
                        .try_into()
                        .map_err(|_| crate::identity::AssertionError::MalformedCanonical)?,
                ),
            })
        }
        _ => Err(crate::identity::AssertionError::MalformedCanonical),
    }
}

fn take_string(
    input: &[u8],
    cursor: &mut usize,
    tag: u8,
    maximum: usize,
) -> Result<String, crate::identity::AssertionError> {
    decode_string(take_field(input, cursor, tag, maximum)?, maximum)
}

fn decode_string(input: &[u8], maximum: usize) -> Result<String, crate::identity::AssertionError> {
    if input.is_empty() || input.len() > maximum {
        return Err(crate::identity::AssertionError::MalformedCanonical);
    }
    core::str::from_utf8(input)
        .map(str::to_string)
        .map_err(|_| crate::identity::AssertionError::MalformedCanonical)
}

fn take_optional_u32(
    input: &[u8],
    cursor: &mut usize,
    tag: u8,
) -> Result<Option<u32>, crate::identity::AssertionError> {
    let bytes = take_field(input, cursor, tag, U32_FIELD_BYTES)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() != U32_FIELD_BYTES {
        return Err(crate::identity::AssertionError::MalformedCanonical);
    }
    Ok(Some(u32::from_le_bytes(bytes.try_into().map_err(
        |_| crate::identity::AssertionError::MalformedCanonical,
    )?)))
}

fn take_u32(
    input: &[u8],
    cursor: &mut usize,
    tag: u8,
) -> Result<u32, crate::identity::AssertionError> {
    let bytes = take_field(input, cursor, tag, U32_FIELD_BYTES)?;
    if bytes.len() != U32_FIELD_BYTES {
        return Err(crate::identity::AssertionError::MalformedCanonical);
    }
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
        crate::identity::AssertionError::MalformedCanonical
    })?))
}

fn take_field<'a>(
    input: &'a [u8],
    cursor: &mut usize,
    expected_tag: u8,
    maximum: usize,
) -> Result<&'a [u8], crate::identity::AssertionError> {
    let tag = take_byte(input, cursor)?;
    if tag != expected_tag || input.len().saturating_sub(*cursor) < FIELD_LENGTH_BYTES {
        return Err(crate::identity::AssertionError::MalformedCanonical);
    }
    let length_end = cursor
        .checked_add(FIELD_LENGTH_BYTES)
        .ok_or(crate::identity::AssertionError::MalformedCanonical)?;
    let length = u16::from_le_bytes(
        input[*cursor..length_end]
            .try_into()
            .map_err(|_| crate::identity::AssertionError::MalformedCanonical)?,
    ) as usize;
    *cursor = length_end;
    if length > maximum || input.len().saturating_sub(*cursor) < length {
        return Err(crate::identity::AssertionError::MalformedCanonical);
    }
    let end = cursor
        .checked_add(length)
        .ok_or(crate::identity::AssertionError::MalformedCanonical)?;
    let value = &input[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn take_byte(input: &[u8], cursor: &mut usize) -> Result<u8, crate::identity::AssertionError> {
    let value = *input
        .get(*cursor)
        .ok_or(crate::identity::AssertionError::MalformedCanonical)?;
    *cursor = cursor
        .checked_add(1)
        .ok_or(crate::identity::AssertionError::MalformedCanonical)?;
    Ok(value)
}
