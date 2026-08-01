use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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

    pub fn from_hex(value: &str) -> Result<Self, IdentityError> {
        if value.len() != ASSERTION_FINGERPRINT_HEX_BYTES {
            return Err(IdentityError::InvalidFingerprint);
        }
        let mut bytes = [0_u8; ASSERTION_FINGERPRINT_BYTES];
        for (index, pair) in value
            .as_bytes()
            .chunks_exact(HEX_CHARACTERS_PER_BYTE)
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
    Always = ASSERTION_KIND_ALWAYS_DISCRIMINANT,
    Sometimes = ASSERTION_KIND_SOMETIMES_DISCRIMINANT,
    Reachable = ASSERTION_KIND_REACHABLE_DISCRIMINANT,
    Unreachable = ASSERTION_KIND_UNREACHABLE_DISCRIMINANT,
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_compatibility_id"
    )]
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

fn deserialize_compatibility_id<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    u32::deserialize(deserializer).map(Some)
}

impl AssertionDescriptor {
    pub fn validate(&self) -> Result<(), IdentityError> {
        crate::assertion_identity_core::validate_descriptor(self)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, IdentityError> {
        crate::assertion_identity_core::canonical_descriptor(self)
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

fn hex_nibble(byte: u8) -> Result<u8, IdentityError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + HEX_ALPHA_DIGIT_OFFSET),
        _ => Err(IdentityError::InvalidFingerprint),
    }
}
