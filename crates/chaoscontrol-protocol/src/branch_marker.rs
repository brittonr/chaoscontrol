//! Bounded guest-declared branch-marker identity and admission.

pub const BRANCH_MARKER_EVENT: &str = "chaoscontrol_branch_marker";
pub const BRANCH_MARKER_LIMIT_EVENT: &str = "chaoscontrol_branch_marker_limit";
pub const BRANCH_MARKER_SCHEMA: &str = "chaoscontrol.branch-marker.v1";
pub const BRANCH_MARKER_ASSERTION_CATEGORY: &str = "branch-marker";
pub const MARKER_IDENTITY_PREFIX: &str = "b3:";
pub const MAX_MARKERS_PER_RUN: usize = 64;
pub const MAX_MARKER_TEXT_BYTES: usize = 128;
pub const MAX_MARKER_REF_BYTES: usize = 256;
pub const MAX_MARKER_DETAILS_BYTES: usize = 2_048;
const BLAKE3_HEX_BYTES: usize = 64;
const IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.branch-marker.identity.v1\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchMarkerError {
    DetailsTooLarge,
    EmptyText,
    IdentityMismatch,
    InvalidReference,
    InvalidSchema,
    TextTooLarge,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchMarker {
    pub schema: String,
    pub identity: String,
    pub namespace: String,
    pub key: String,
    pub owner: String,
    pub details: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical_position_ref: Option<String>,
}

impl BranchMarker {
    pub fn new(
        namespace: &str,
        key: &str,
        owner: &str,
        details: serde_json::Value,
        state_ref: Option<String>,
        logical_position_ref: Option<String>,
    ) -> Result<Self, BranchMarkerError> {
        validate_text(namespace)?;
        validate_text(key)?;
        validate_text(owner)?;
        validate_details(&details)?;
        validate_optional_ref(state_ref.as_deref(), true)?;
        validate_optional_ref(logical_position_ref.as_deref(), false)?;
        let marker = Self {
            schema: BRANCH_MARKER_SCHEMA.to_string(),
            identity: marker_identity(namespace, key),
            namespace: namespace.to_string(),
            key: key.to_string(),
            owner: owner.to_string(),
            details,
            state_ref,
            logical_position_ref,
        };
        marker.validate()?;
        Ok(marker)
    }

    pub fn validate(&self) -> Result<(), BranchMarkerError> {
        if self.schema != BRANCH_MARKER_SCHEMA {
            return Err(BranchMarkerError::InvalidSchema);
        }
        validate_text(&self.namespace)?;
        validate_text(&self.key)?;
        validate_text(&self.owner)?;
        validate_details(&self.details)?;
        validate_optional_ref(self.state_ref.as_deref(), true)?;
        validate_optional_ref(self.logical_position_ref.as_deref(), false)?;
        if self.identity != marker_identity(&self.namespace, &self.key) {
            return Err(BranchMarkerError::IdentityMismatch);
        }
        Ok(())
    }

    pub fn from_value(value: &serde_json::Value) -> Result<Self, BranchMarkerError> {
        let marker: Self =
            serde_json::from_value(value.clone()).map_err(|_| BranchMarkerError::InvalidSchema)?;
        marker.validate()?;
        Ok(marker)
    }

    pub fn collapse_key(&self) -> (&str, Option<&str>, Option<&str>) {
        (
            &self.identity,
            self.state_ref.as_deref(),
            self.logical_position_ref.as_deref(),
        )
    }
}

pub fn marker_identity(namespace: &str, key: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(IDENTITY_DOMAIN);
    hash_field(&mut hasher, namespace.as_bytes());
    hash_field(&mut hasher, key.as_bytes());
    format!("{MARKER_IDENTITY_PREFIX}{}", hasher.finalize().to_hex())
}

const _: () = assert!(usize::BITS <= u64::BITS);

fn hash_field(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    let length =
        u64::try_from(bytes.len()).expect("supported pointer widths fit the canonical length");
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
}

fn validate_text(value: &str) -> Result<(), BranchMarkerError> {
    if value.is_empty() {
        return Err(BranchMarkerError::EmptyText);
    }
    if value.len() > MAX_MARKER_TEXT_BYTES || value.chars().any(char::is_control) {
        return Err(BranchMarkerError::TextTooLarge);
    }
    Ok(())
}

fn validate_details(details: &serde_json::Value) -> Result<(), BranchMarkerError> {
    let bytes = serde_json::to_vec(details).map_err(|_| BranchMarkerError::DetailsTooLarge)?;
    if bytes.len() > MAX_MARKER_DETAILS_BYTES {
        return Err(BranchMarkerError::DetailsTooLarge);
    }
    Ok(())
}

fn validate_optional_ref(value: Option<&str>, state: bool) -> Result<(), BranchMarkerError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.is_empty() || value.len() > MAX_MARKER_REF_BYTES || value.chars().any(char::is_control)
    {
        return Err(BranchMarkerError::InvalidReference);
    }
    if state && !valid_b3(value) {
        return Err(BranchMarkerError::InvalidReference);
    }
    Ok(())
}

fn valid_b3(value: &str) -> bool {
    let Some(hex) = value.strip_prefix(MARKER_IDENTITY_PREFIX) else {
        return false;
    };
    hex.len() == BLAKE3_HEX_BYTES
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_framing_preserves_lengths_and_rejects_ambiguous_boundaries() {
        const FIELD: &[u8] = b"ab";
        const FIELD_LENGTH: u64 = 2;
        let mut expected = blake3::Hasher::new();
        expected.update(&FIELD_LENGTH.to_le_bytes());
        expected.update(FIELD);
        let mut actual = blake3::Hasher::new();
        hash_field(&mut actual, FIELD);
        assert_eq!(actual.finalize(), expected.finalize());

        assert_ne!(marker_identity("ab", "c"), marker_identity("a", "bc"));
        let empty = blake3::Hasher::new();
        let mut framed_empty = empty.clone();
        hash_field(&mut framed_empty, &[]);
        assert_ne!(empty.finalize(), framed_empty.finalize());
    }

    #[test]
    fn stable_identity_ignores_instance_details_and_refs() {
        let first = BranchMarker::new(
            "raft",
            "leader-elected",
            "guest-0",
            serde_json::json!({"term": 1}),
            None,
            None,
        )
        .unwrap();
        let second = BranchMarker::new(
            "raft",
            "leader-elected",
            "guest-1",
            serde_json::json!({"term": 2}),
            Some(format!("b3:{}", "a".repeat(BLAKE3_HEX_BYTES))),
            Some("term:2".to_string()),
        )
        .unwrap();
        assert_eq!(first.identity, second.identity);
        assert_ne!(first.collapse_key(), second.collapse_key());
    }

    #[test]
    fn omitted_optional_refs_round_trip_but_wrong_types_fail() {
        let marker = BranchMarker::new(
            "raft",
            "declared",
            "fixture",
            serde_json::json!({}),
            None,
            None,
        )
        .unwrap();
        let value = serde_json::to_value(&marker).unwrap();
        assert!(value.get("state_ref").is_none());
        assert!(value.get("logical_position_ref").is_none());
        assert_eq!(BranchMarker::from_value(&value).unwrap(), marker);
        for field in ["state_ref", "logical_position_ref"] {
            let mut bad = value.clone();
            bad[field] = true.into();
            assert_eq!(
                BranchMarker::from_value(&bad),
                Err(BranchMarkerError::InvalidSchema)
            );
        }
        let mut missing = value;
        missing.as_object_mut().unwrap().remove("identity");
        assert_eq!(
            BranchMarker::from_value(&missing),
            Err(BranchMarkerError::InvalidSchema)
        );
    }

    #[test]
    fn malformed_identity_refs_and_bounds_fail_closed() {
        let mut marker = BranchMarker::new(
            "raft",
            "leader-elected",
            "guest-0",
            serde_json::json!({}),
            None,
            None,
        )
        .unwrap();
        marker.identity = format!("b3:{}", "f".repeat(BLAKE3_HEX_BYTES));
        assert_eq!(marker.validate(), Err(BranchMarkerError::IdentityMismatch));
        assert!(BranchMarker::new("", "key", "owner", serde_json::json!({}), None, None).is_err());
        assert!(BranchMarker::new(
            "ns",
            "key",
            "owner",
            serde_json::json!({}),
            Some("sha256:bad".to_string()),
            None,
        )
        .is_err());
        assert!(BranchMarker::new(
            "ns",
            "key",
            "owner",
            serde_json::Value::String("x".repeat(MAX_MARKER_DETAILS_BYTES)),
            None,
            None,
        )
        .is_err());
    }
}
