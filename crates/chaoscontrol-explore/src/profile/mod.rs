mod campaign;
mod run;
mod schedule;
#[cfg(test)]
mod tests;

pub use campaign::{CampaignProfile, PreparedScenario};
pub use run::RunProfile;
pub use schedule::FaultScheduleProfile;

const MAX_PATH_BYTES: usize = 4096;
const MAX_IDENTIFIER_BYTES: usize = 128;
const MAX_PROFILE_JSON_BYTES: u64 = 1024 * 1024;
const BLAKE3_PREFIX: &str = "blake3:";
const BLAKE3_HEX_LENGTH: usize = 64;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub kind: ArtifactReferenceKind,
    pub path: String,
    pub identity: String,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactReferenceKind {
    AbsolutePath,
    RelativeArtifact,
}

impl ArtifactReference {
    pub fn validate(&self) -> Result<(), String> {
        let path = std::path::Path::new(&self.path);
        if self.path.is_empty() || self.path.len() > MAX_PATH_BYTES || self.path.contains('\\') {
            return Err("profile artifact path is empty, oversized, or non-native".to_string());
        }
        for component in path.components() {
            if matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::CurDir
            ) {
                return Err("profile artifact path contains traversal".to_string());
            }
        }
        let absolute = path.is_absolute();
        if (self.kind == ArtifactReferenceKind::AbsolutePath) != absolute {
            return Err("profile artifact path kind does not match its path".to_string());
        }
        let Some(hex) = self.identity.strip_prefix(BLAKE3_PREFIX) else {
            return Err("profile artifact identity is not BLAKE3-bound".to_string());
        };
        if hex.len() != BLAKE3_HEX_LENGTH
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err("profile artifact identity is not lowercase BLAKE3".to_string());
        }
        Ok(())
    }
}

pub(crate) fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

pub(crate) fn checked_usize(field: &str, value: u64) -> Result<usize, String> {
    usize::try_from(value).map_err(|_| format!("profile {field} exceeds usize"))
}

pub fn load_run_profile(path: &std::path::Path) -> std::io::Result<RunProfile> {
    load_profile(path)
}

pub fn load_campaign_profile(path: &std::path::Path) -> std::io::Result<CampaignProfile> {
    load_profile(path)
}

pub fn load_fault_schedule_profile(
    path: &std::path::Path,
) -> std::io::Result<FaultScheduleProfile> {
    load_profile(path)
}

fn load_profile<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> std::io::Result<T> {
    let input = crate::bounded_json::read_bounded_json(path, MAX_PROFILE_JSON_BYTES)?;
    serde_json::from_str(&input).map_err(std::io::Error::other)
}
