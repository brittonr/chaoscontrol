const FIELD_COUNT: u8 = 10;
const CANONICAL_FIELD_HEADER_BYTES: usize = 3;
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

pub(crate) fn validate_descriptor(
    descriptor: &crate::identity::AssertionDescriptor,
) -> Result<(), crate::identity::AssertionError> {
    if descriptor.identity_version != crate::identity::ASSERTION_IDENTITY_VERSION {
        return Err(crate::identity::AssertionError::InvalidVersion);
    }
    validate_text(
        "namespace",
        &descriptor.namespace,
        crate::identity::MAX_ASSERTION_NAMESPACE_BYTES,
    )?;
    match &descriptor.logical_key {
        crate::identity::AssertionLogicalKey::Automatic { source_site } => {
            validate_text(
                "source_site",
                source_site,
                crate::identity::MAX_ASSERTION_KEY_BYTES,
            )?;
        }
        crate::identity::AssertionLogicalKey::Stable { key } => {
            validate_text("key", key, crate::identity::MAX_ASSERTION_KEY_BYTES)?;
        }
        crate::identity::AssertionLogicalKey::LegacyU32 { id } => {
            if descriptor.compatibility_id != Some(*id) {
                return Err(crate::identity::AssertionError::InvalidLegacyAlias);
            }
        }
    }
    validate_text(
        "message",
        &descriptor.message,
        crate::identity::MAX_ASSERTION_MESSAGE_BYTES,
    )?;
    validate_text(
        "source_file",
        &descriptor.source_file,
        crate::identity::MAX_ASSERTION_SOURCE_BYTES,
    )?;
    validate_source_path(&descriptor.source_file)?;
    if descriptor.source_line == 0 || descriptor.source_column == 0 {
        return Err(crate::identity::AssertionError::InvalidSourcePosition);
    }
    if let crate::identity::AssertionLogicalKey::Automatic { source_site } = &descriptor.logical_key
    {
        let expected = format!(
            "{}:{}:{}",
            descriptor.source_file, descriptor.source_line, descriptor.source_column
        );
        if source_site != &expected {
            return Err(crate::identity::AssertionError::InvalidAutomaticSourceSite);
        }
    }
    validate_text(
        "guest",
        &descriptor.guest,
        crate::identity::MAX_ASSERTION_GUEST_BYTES,
    )?;
    validate_category(&descriptor.category)?;
    Ok(())
}

pub(crate) fn canonical_descriptor(
    descriptor: &crate::identity::AssertionDescriptor,
) -> Result<Vec<u8>, crate::identity::AssertionError> {
    validate_descriptor(descriptor)?;
    let mut output = Vec::with_capacity(crate::identity::MAX_ASSERTION_CANONICAL_BYTES);
    output.extend_from_slice(crate::identity::ASSERTION_DESCRIPTOR_DOMAIN);
    output.push(crate::identity::ASSERTION_IDENTITY_VERSION);
    output.push(FIELD_COUNT);
    write_field(
        &mut output,
        NAMESPACE_FIELD_TAG,
        descriptor.namespace.as_bytes(),
    )?;
    write_field(
        &mut output,
        LOGICAL_KEY_FIELD_TAG,
        &logical_key_bytes(&descriptor.logical_key)?,
    )?;
    write_field(&mut output, KIND_FIELD_TAG, &[descriptor.kind as u8])?;
    write_field(
        &mut output,
        MESSAGE_FIELD_TAG,
        descriptor.message.as_bytes(),
    )?;
    write_field(
        &mut output,
        SOURCE_FILE_FIELD_TAG,
        descriptor.source_file.as_bytes(),
    )?;
    write_field(
        &mut output,
        SOURCE_LINE_FIELD_TAG,
        &descriptor.source_line.to_le_bytes(),
    )?;
    write_field(
        &mut output,
        SOURCE_COLUMN_FIELD_TAG,
        &descriptor.source_column.to_le_bytes(),
    )?;
    write_field(&mut output, GUEST_FIELD_TAG, descriptor.guest.as_bytes())?;
    write_field(
        &mut output,
        CATEGORY_FIELD_TAG,
        descriptor.category.as_bytes(),
    )?;
    let compatibility_id = descriptor.compatibility_id.map(u32::to_le_bytes);
    write_field(
        &mut output,
        COMPATIBILITY_ID_FIELD_TAG,
        compatibility_id
            .as_ref()
            .map_or(&[], |bytes| bytes.as_slice()),
    )?;
    if output.len() > crate::identity::MAX_ASSERTION_CANONICAL_BYTES {
        return Err(crate::identity::AssertionError::FieldTooLong(
            "canonical_descriptor",
        ));
    }
    Ok(output)
}

fn logical_key_bytes(
    key: &crate::identity::AssertionLogicalKey,
) -> Result<Vec<u8>, crate::identity::AssertionError> {
    let mut output = Vec::with_capacity(crate::identity::MAX_ASSERTION_KEY_BYTES + 1);
    match key {
        crate::identity::AssertionLogicalKey::Automatic { source_site } => {
            output.push(AUTOMATIC_KEY_DISCRIMINANT);
            output.extend_from_slice(source_site.as_bytes());
        }
        crate::identity::AssertionLogicalKey::Stable { key } => {
            output.push(STABLE_KEY_DISCRIMINANT);
            output.extend_from_slice(key.as_bytes());
        }
        crate::identity::AssertionLogicalKey::LegacyU32 { id } => {
            output.push(LEGACY_KEY_DISCRIMINANT);
            output.extend_from_slice(&id.to_le_bytes());
        }
    }
    Ok(output)
}

fn write_field(
    output: &mut Vec<u8>,
    tag: u8,
    value: &[u8],
) -> Result<(), crate::identity::AssertionError> {
    let length = u16::try_from(value.len())
        .map_err(|_| crate::identity::AssertionError::MalformedCanonical)?;
    let required = output
        .len()
        .saturating_add(CANONICAL_FIELD_HEADER_BYTES)
        .saturating_add(value.len());
    if required > crate::identity::MAX_ASSERTION_CANONICAL_BYTES {
        return Err(crate::identity::AssertionError::FieldTooLong(
            "canonical_descriptor",
        ));
    }
    output.push(tag);
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(value);
    Ok(())
}

fn validate_category(value: &str) -> Result<(), crate::identity::AssertionError> {
    validate_text(
        "category",
        value,
        crate::identity::MAX_ASSERTION_CATEGORY_BYTES,
    )?;
    let normalized = value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !normalized || value.starts_with('-') || value.ends_with('-') {
        return Err(crate::identity::AssertionError::InvalidCategory);
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), crate::identity::AssertionError> {
    if value.is_empty() {
        return Err(crate::identity::AssertionError::EmptyField(field));
    }
    if value.len() > maximum {
        return Err(crate::identity::AssertionError::FieldTooLong(field));
    }
    if value
        .bytes()
        .any(|byte| byte == 0 || byte.is_ascii_control())
    {
        return Err(crate::identity::AssertionError::InvalidCharacter(field));
    }
    Ok(())
}

fn validate_source_path(value: &str) -> Result<(), crate::identity::AssertionError> {
    if value.starts_with('/') || value.starts_with('\\') || value.contains("\\") {
        return Err(crate::identity::AssertionError::InvalidPath);
    }
    if value
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(crate::identity::AssertionError::InvalidPath);
    }
    Ok(())
}
