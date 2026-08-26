pub use crate::{
    ASSERTION_KIND_ALWAYS_DISCRIMINANT, ASSERTION_KIND_REACHABLE_DISCRIMINANT,
    ASSERTION_KIND_SOMETIMES_DISCRIMINANT, ASSERTION_KIND_UNREACHABLE_DISCRIMINANT,
};

pub const ASSERTION_IDENTITY_VERSION: u8 = 1;
pub const ASSERTION_FINGERPRINT_BYTES: usize = 32;
const HEX_CHARACTERS_PER_BYTE: usize = 2;
const HEX_ALPHABET_BYTES: usize = 16;
const HEX_HIGH_NIBBLE_SHIFT: u32 = 4;
const HEX_LOW_NIBBLE_MASK: u8 = 0x0f;
const HEX_ALPHA_DIGIT_OFFSET: u8 = 10;
pub const ASSERTION_FINGERPRINT_HEX_BYTES: usize =
    ASSERTION_FINGERPRINT_BYTES * HEX_CHARACTERS_PER_BYTE;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssertionFingerprint(pub [u8; ASSERTION_FINGERPRINT_BYTES]);

impl AssertionFingerprint {
    pub const ZERO: Self = Self([0; ASSERTION_FINGERPRINT_BYTES]);

    pub fn to_hex(self) -> String {
        encode_lower_hex(&self.0)
    }

    pub fn from_hex(value: &str) -> Result<Self, AssertionError> {
        if value.len() != ASSERTION_FINGERPRINT_HEX_BYTES {
            return Err(AssertionError::InvalidFingerprint);
        }
        let mut bytes = [0_u8; ASSERTION_FINGERPRINT_BYTES];
        for (index, pair) in value
            .as_bytes()
            .as_chunks::<HEX_CHARACTERS_PER_BYTE>()
            .0
            .iter()
            .enumerate()
        {
            bytes[index] = (hex_nibble(pair[0])? << HEX_HIGH_NIBBLE_SHIFT) | hex_nibble(pair[1])?;
        }
        Ok(Self(bytes))
    }
}

pub fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX_ALPHABET: &[u8; HEX_ALPHABET_BYTES] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(HEX_CHARACTERS_PER_BYTE));
    for byte in bytes {
        output.push(HEX_ALPHABET[(byte >> HEX_HIGH_NIBBLE_SHIFT) as usize] as char);
        output.push(HEX_ALPHABET[(byte & HEX_LOW_NIBBLE_MASK) as usize] as char);
    }
    output
}

impl core::fmt::Display for AssertionFingerprint {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl serde::Serialize for AssertionFingerprint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for AssertionFingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(serde::de::Error::custom)
    }
}

#[repr(u8)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AssertionKind {
    Always = ASSERTION_KIND_ALWAYS_DISCRIMINANT,
    Sometimes = ASSERTION_KIND_SOMETIMES_DISCRIMINANT,
    Reachable = ASSERTION_KIND_REACHABLE_DISCRIMINANT,
    Unreachable = ASSERTION_KIND_UNREACHABLE_DISCRIMINANT,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssertionLogicalKey {
    Automatic { source_site: String },
    Stable { key: String },
    LegacyU32 { id: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionDescriptor {
    pub identity_version: u8,
    pub namespace: String,
    pub logical_key: AssertionLogicalKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility_id: Option<u32>,
    pub kind: AssertionKind,
    pub message: String,
    pub source_file: String,
    pub source_line: u32,
    pub source_column: u32,
    pub guest: String,
    pub category: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAssertionDescriptor {
    identity_version: u8,
    namespace: String,
    logical_key: AssertionLogicalKey,
    compatibility_id: Option<u32>,
    kind: AssertionKind,
    message: String,
    source_file: String,
    source_line: u32,
    source_column: u32,
    guest: String,
    category: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertionError {
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

impl core::fmt::Display for AssertionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "assertion identity error: {self:?}")
    }
}

impl std::error::Error for AssertionError {}

impl<'de> serde::Deserialize<'de> for AssertionDescriptor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = <RawAssertionDescriptor as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Self {
            identity_version: raw.identity_version,
            namespace: raw.namespace,
            logical_key: raw.logical_key,
            compatibility_id: raw.compatibility_id,
            kind: raw.kind,
            message: raw.message,
            source_file: raw.source_file,
            source_line: raw.source_line,
            source_column: raw.source_column,
            guest: raw.guest,
            category: raw.category,
        })
    }
}

impl AssertionDescriptor {
    pub fn validate(&self) -> Result<(), AssertionError> {
        crate::canonical::validate_descriptor(self)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, AssertionError> {
        crate::canonical::canonical_descriptor(self)
    }

    pub fn fingerprint(&self) -> Result<AssertionFingerprint, AssertionError> {
        fingerprint_canonical(&self.canonical_bytes()?)
    }
}

pub fn fingerprint_canonical(bytes: &[u8]) -> Result<AssertionFingerprint, AssertionError> {
    if bytes.len() > MAX_ASSERTION_CANONICAL_BYTES {
        return Err(AssertionError::FieldTooLong("canonical_descriptor"));
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(FINGERPRINT_DOMAIN);
    hasher.update(bytes);
    Ok(AssertionFingerprint(*hasher.finalize().as_bytes()))
}

fn hex_nibble(byte: u8) -> Result<u8, AssertionError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + HEX_ALPHA_DIGIT_OFFSET),
        _ => Err(AssertionError::InvalidFingerprint),
    }
}
