use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const ASSERTION_IDENTITY_VERSION: u8 = 1;
pub const ASSERTION_FINGERPRINT_BYTES: usize = 32;
pub const ASSERTION_FINGERPRINT_HEX_BYTES: usize = ASSERTION_FINGERPRINT_BYTES * 2;
pub const MAX_ASSERTION_NAMESPACE_BYTES: usize = 128;
pub const MAX_ASSERTION_KEY_BYTES: usize = 256;
pub const MAX_ASSERTION_MESSAGE_BYTES: usize = 1024;
pub const MAX_ASSERTION_SOURCE_BYTES: usize = 512;
pub const MAX_ASSERTION_GUEST_BYTES: usize = 128;
pub const MAX_ASSERTION_CATEGORY_BYTES: usize = 64;
pub const MAX_ASSERTION_CANONICAL_BYTES: usize = 2304;
pub const MAX_ASSERTION_EVENT_DETAILS_BYTES: usize = 2048;

pub const ASSERTION_DESCRIPTOR_DOMAIN: &[u8] = b"chaoscontrol.assertion-descriptor.v1\0";
const FINGERPRINT_DOMAIN: &[u8] = b"chaoscontrol.assertion-fingerprint.v1\0";
const FIELD_COUNT: u8 = 10;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssertionFingerprint(pub [u8; ASSERTION_FINGERPRINT_BYTES]);

impl AssertionFingerprint {
    pub const ZERO: Self = Self([0; ASSERTION_FINGERPRINT_BYTES]);

    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(ASSERTION_FINGERPRINT_HEX_BYTES);
        for byte in self.0 {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        output
    }

    pub fn from_hex(value: &str) -> Result<Self, IdentityError> {
        if value.len() != ASSERTION_FINGERPRINT_HEX_BYTES {
            return Err(IdentityError::InvalidFingerprint);
        }
        let mut bytes = [0_u8; ASSERTION_FINGERPRINT_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for AssertionFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for AssertionFingerprint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for AssertionFingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(serde::de::Error::custom)
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionKind {
    Always,
    Sometimes,
    Reachable,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssertionLogicalKey {
    Automatic { source_site: String },
    Stable { key: String },
    LegacyU32 { id: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionDescriptor {
    pub identity_version: u8,
    pub namespace: String,
    pub logical_key: AssertionLogicalKey,
    pub compatibility_id: Option<u32>,
    pub kind: AssertionKind,
    pub message: String,
    pub source_file: String,
    pub source_line: u32,
    pub source_column: u32,
    pub guest: String,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    EmptyField(&'static str),
    FieldTooLong(&'static str),
    InvalidAutomaticSourceSite,
    InvalidCharacter(&'static str),
    InvalidCategory,
    InvalidFingerprint,
    InvalidKind,
    InvalidLegacyAlias,
    InvalidPath,
    InvalidSourcePosition,
    InvalidVersion,
    MalformedCanonical,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "assertion identity error: {self:?}")
    }
}

impl std::error::Error for IdentityError {}

impl AssertionDescriptor {
    pub fn validate(&self) -> Result<(), IdentityError> {
        if self.identity_version != ASSERTION_IDENTITY_VERSION {
            return Err(IdentityError::InvalidVersion);
        }
        validate_text("namespace", &self.namespace, MAX_ASSERTION_NAMESPACE_BYTES)?;
        match &self.logical_key {
            AssertionLogicalKey::Automatic { source_site } => {
                validate_text("source_site", source_site, MAX_ASSERTION_KEY_BYTES)?;
            }
            AssertionLogicalKey::Stable { key } => {
                validate_text("key", key, MAX_ASSERTION_KEY_BYTES)?;
            }
            AssertionLogicalKey::LegacyU32 { id } => {
                if self.compatibility_id != Some(*id) {
                    return Err(IdentityError::InvalidLegacyAlias);
                }
            }
        }
        validate_text("message", &self.message, MAX_ASSERTION_MESSAGE_BYTES)?;
        validate_text("source_file", &self.source_file, MAX_ASSERTION_SOURCE_BYTES)?;
        validate_source_path(&self.source_file)?;
        if self.source_line == 0 || self.source_column == 0 {
            return Err(IdentityError::InvalidSourcePosition);
        }
        if let AssertionLogicalKey::Automatic { source_site } = &self.logical_key {
            let expected = format!(
                "{}:{}:{}",
                self.source_file, self.source_line, self.source_column
            );
            if source_site != &expected {
                return Err(IdentityError::InvalidAutomaticSourceSite);
            }
        }
        validate_text("guest", &self.guest, MAX_ASSERTION_GUEST_BYTES)?;
        validate_category(&self.category)?;
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, IdentityError> {
        self.validate()?;
        let mut output = Vec::with_capacity(MAX_ASSERTION_CANONICAL_BYTES);
        output.extend_from_slice(ASSERTION_DESCRIPTOR_DOMAIN);
        output.push(ASSERTION_IDENTITY_VERSION);
        output.push(FIELD_COUNT);
        write_field(&mut output, 1, self.namespace.as_bytes())?;
        write_field(&mut output, 2, &logical_key_bytes(&self.logical_key)?)?;
        write_field(&mut output, 3, &[self.kind as u8])?;
        write_field(&mut output, 4, self.message.as_bytes())?;
        write_field(&mut output, 5, self.source_file.as_bytes())?;
        write_field(&mut output, 6, &self.source_line.to_le_bytes())?;
        write_field(&mut output, 7, &self.source_column.to_le_bytes())?;
        write_field(&mut output, 8, self.guest.as_bytes())?;
        write_field(&mut output, 9, self.category.as_bytes())?;
        let compatibility_id = self.compatibility_id.map(u32::to_le_bytes);
        write_field(
            &mut output,
            10,
            compatibility_id
                .as_ref()
                .map_or(&[], |bytes| bytes.as_slice()),
        )?;
        if output.len() > MAX_ASSERTION_CANONICAL_BYTES {
            return Err(IdentityError::FieldTooLong("canonical_descriptor"));
        }
        Ok(output)
    }

    pub fn fingerprint(&self) -> Result<AssertionFingerprint, IdentityError> {
        fingerprint_canonical(&self.canonical_bytes()?)
    }
}

pub fn fingerprint_canonical(bytes: &[u8]) -> Result<AssertionFingerprint, IdentityError> {
    if bytes.len() > MAX_ASSERTION_CANONICAL_BYTES {
        return Err(IdentityError::FieldTooLong("canonical_descriptor"));
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(FINGERPRINT_DOMAIN);
    hasher.update(bytes);
    Ok(AssertionFingerprint(*hasher.finalize().as_bytes()))
}

fn logical_key_bytes(key: &AssertionLogicalKey) -> Result<Vec<u8>, IdentityError> {
    let mut output = Vec::with_capacity(MAX_ASSERTION_KEY_BYTES + 1);
    match key {
        AssertionLogicalKey::Automatic { source_site } => {
            output.push(1);
            output.extend_from_slice(source_site.as_bytes());
        }
        AssertionLogicalKey::Stable { key } => {
            output.push(2);
            output.extend_from_slice(key.as_bytes());
        }
        AssertionLogicalKey::LegacyU32 { id } => {
            output.push(3);
            output.extend_from_slice(&id.to_le_bytes());
        }
    }
    Ok(output)
}

fn write_field(output: &mut Vec<u8>, tag: u8, value: &[u8]) -> Result<(), IdentityError> {
    let length = u16::try_from(value.len()).map_err(|_| IdentityError::MalformedCanonical)?;
    let required = output.len().saturating_add(3).saturating_add(value.len());
    if required > MAX_ASSERTION_CANONICAL_BYTES {
        return Err(IdentityError::FieldTooLong("canonical_descriptor"));
    }
    output.push(tag);
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(value);
    Ok(())
}

fn validate_category(value: &str) -> Result<(), IdentityError> {
    validate_text("category", value, MAX_ASSERTION_CATEGORY_BYTES)?;
    let normalized = value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !normalized || value.starts_with('-') || value.ends_with('-') {
        return Err(IdentityError::InvalidCategory);
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, maximum: usize) -> Result<(), IdentityError> {
    if value.is_empty() {
        return Err(IdentityError::EmptyField(field));
    }
    if value.len() > maximum {
        return Err(IdentityError::FieldTooLong(field));
    }
    if value
        .bytes()
        .any(|byte| byte == 0 || byte.is_ascii_control())
    {
        return Err(IdentityError::InvalidCharacter(field));
    }
    Ok(())
}

fn validate_source_path(value: &str) -> Result<(), IdentityError> {
    if value.starts_with('/') || value.starts_with('\\') || value.contains("\\") {
        return Err(IdentityError::InvalidPath);
    }
    if value
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(IdentityError::InvalidPath);
    }
    Ok(())
}

fn hex_nibble(byte: u8) -> Result<u8, IdentityError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(IdentityError::InvalidFingerprint),
    }
}
