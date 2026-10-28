//! Versioned descriptor profile for native-process replay diagnostics.
//!
//! A descriptor is detached diagnostic evidence. It never becomes a semantic
//! observation, logical world root, authority grant, or equivalence proof.
//!
//! r[impl chaoscontrol.native_process_profile.identity]
//! r[impl chaoscontrol.native_process_profile.admission]
//! r[impl chaoscontrol.native_process_profile.records]
//! r[impl chaoscontrol.native_process_profile.detached_nonclaims]
//! r[impl chaoscontrol.native_process_profile.validation]

use serde::{Deserialize, Serialize};

pub const NATIVE_PROCESS_PROFILE_SCHEMA: &str = "chaoscontrol.native-process-replay-profile.v1";
pub const NATIVE_PROCESS_DESCRIPTOR_SCHEMA: &str = "chaoscontrol.native-process-descriptor.v1";
pub const NATIVE_PROCESS_RECORD_SCHEMA: &str = "chaoscontrol.native-process-record.v1";
pub const NATIVE_PROCESS_PROFILE_VERSION: u32 = 1;
pub const MAX_NATIVE_PROCESS_RECORDS: usize = 64;
pub const MAX_NATIVE_PROCESS_TEXT_BYTES: usize = 256;
pub const MAX_NATIVE_PROCESS_EVIDENCE_REFS: usize = 4;
pub const NATIVE_PROCESS_NON_CLAIMS: [&str; 4] = [
    "descriptor does not establish semantic equivalence",
    "descriptor is not a logical world root",
    "descriptor grants no authority, repair permission, or voting permission",
    "descriptor does not prove guest, host, or workload behaviour",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReplayClass {
    RecordedOnly,
    RecordReplayMatched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum ProcessExit {
    Exited { code: i32 },
    Killed,
    Restarted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProcessProfile {
    pub schema: String,
    pub version: u32,
    pub producer_cohort: String,
    pub record_schema: String,
    pub replay_class: ReplayClass,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProcessRecord {
    pub process_key: String,
    pub generation: u64,
    pub sequence: u64,
    pub executable_blake3: String,
    pub start_tick: u64,
    pub end_tick: u64,
    pub exit: ProcessExit,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProcessDescriptor {
    pub schema: String,
    pub profile: NativeProcessProfile,
    pub records: Vec<NativeProcessRecord>,
}

fn is_hex_ref(value: &str) -> bool {
    value
        .strip_prefix("blake3:")
        .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

fn valid_text(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_NATIVE_PROCESS_TEXT_BYTES {
        return Err(format!(
            "{label}: text length is outside 1..={MAX_NATIVE_PROCESS_TEXT_BYTES}"
        ));
    }
    if value.contains('/') || value.contains('\\') {
        return Err(format!("{label}: host path fragment is not permitted"));
    }
    let lowered = value.to_ascii_lowercase();
    if ["secret", "token", "password", "private_key"]
        .iter()
        .any(|needle| lowered.contains(needle))
    {
        return Err(format!("{label}: secret-bearing text is not permitted"));
    }
    if !value.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        return Err(format!("{label}: text is not printable ASCII"));
    }
    Ok(())
}

impl NativeProcessProfile {
    /// Validate the declared profile fields.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != NATIVE_PROCESS_PROFILE_SCHEMA {
            return Err(String::from("profile: schema is not current"));
        }
        if self.version != NATIVE_PROCESS_PROFILE_VERSION {
            return Err(String::from("profile: version is not current"));
        }
        if self.record_schema != NATIVE_PROCESS_RECORD_SCHEMA {
            return Err(String::from("profile: record schema is not current"));
        }
        valid_text("profile producer cohort", &self.producer_cohort)?;
        if self.non_claims.len() != NATIVE_PROCESS_NON_CLAIMS.len()
            || self
                .non_claims
                .iter()
                .map(String::as_str)
                .ne(NATIVE_PROCESS_NON_CLAIMS)
        {
            return Err(String::from("profile: non-claims are not the current set"));
        }
        Ok(())
    }

    /// Canonical profile identity.
    pub fn identity(&self) -> Result<String, String> {
        self.validate()?;
        let encoded = serde_json::to_vec(self).map_err(|error| format!("profile: {error}"))?;
        Ok(format!("blake3:{}", blake3::hash(&encoded).to_hex()))
    }
}

impl NativeProcessDescriptor {
    /// Validate the descriptor without trusting its summary.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != NATIVE_PROCESS_DESCRIPTOR_SCHEMA {
            return Err(String::from("descriptor: schema is not current"));
        }
        self.profile.validate()?;
        if self.records.is_empty() || self.records.len() > MAX_NATIVE_PROCESS_RECORDS {
            return Err(format!(
                "descriptor: record count is outside 1..={MAX_NATIVE_PROCESS_RECORDS}"
            ));
        }
        let mut previous: Option<(&str, u64, u64)> = None;
        for record in &self.records {
            valid_text("record process key", &record.process_key)?;
            if !is_hex_ref(&record.executable_blake3) {
                return Err(String::from(
                    "record: executable identity is not a BLAKE3 reference",
                ));
            }
            if record.end_tick < record.start_tick {
                return Err(String::from("record: end tick precedes start tick"));
            }
            if record.evidence_refs.len() > MAX_NATIVE_PROCESS_EVIDENCE_REFS {
                return Err(String::from("record: evidence ref count exceeds its bound"));
            }
            for reference in &record.evidence_refs {
                if !is_hex_ref(reference) {
                    return Err(String::from(
                        "record: evidence identity is not a BLAKE3 reference",
                    ));
                }
            }
            let key = (
                record.process_key.as_str(),
                record.generation,
                record.sequence,
            );
            if let Some(previous) = previous {
                let is_same_process = previous.0 == key.0 && previous.1 == key.1;
                if is_same_process && key.2 <= previous.2 {
                    return Err(String::from("record: sequence is not strictly increasing"));
                }
                if !is_same_process && key <= previous {
                    return Err(String::from("record: process order drifted"));
                }
            }
            previous = Some(key);
        }
        Ok(())
    }

    /// Canonical descriptor identity.
    pub fn identity(&self) -> Result<String, String> {
        self.validate()?;
        let encoded = serde_json::to_vec(self).map_err(|error| format!("descriptor: {error}"))?;
        Ok(format!("blake3:{}", blake3::hash(&encoded).to_hex()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> NativeProcessProfile {
        NativeProcessProfile {
            schema: NATIVE_PROCESS_PROFILE_SCHEMA.to_string(),
            version: NATIVE_PROCESS_PROFILE_VERSION,
            producer_cohort: "chaoscontrol-native-process-v1".to_string(),
            record_schema: NATIVE_PROCESS_RECORD_SCHEMA.to_string(),
            replay_class: ReplayClass::RecordReplayMatched,
            non_claims: NATIVE_PROCESS_NON_CLAIMS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    fn record(
        key: &str,
        generation: u64,
        sequence: u64,
        start: u64,
        end: u64,
    ) -> NativeProcessRecord {
        NativeProcessRecord {
            process_key: key.to_string(),
            generation,
            sequence,
            executable_blake3: format!("blake3:{}", "a".repeat(64)),
            start_tick: start,
            end_tick: end,
            exit: ProcessExit::Exited { code: 0 },
            evidence_refs: vec![format!("blake3:{}", "b".repeat(64))],
        }
    }

    fn sample() -> NativeProcessDescriptor {
        NativeProcessDescriptor {
            schema: NATIVE_PROCESS_DESCRIPTOR_SCHEMA.to_string(),
            profile: profile(),
            records: vec![
                record("vmm", 1, 1, 0, 10),
                record("vmm", 1, 2, 10, 20),
                record("vmm", 2, 1, 20, 30),
                record("worker", 1, 1, 0, 30),
            ],
        }
    }

    #[test]
    fn complete_descriptor_and_profile_identify() {
        let descriptor = sample();
        assert!(descriptor.validate().is_ok());
        assert!(descriptor
            .identity()
            .expect("identity")
            .starts_with("blake3:"));
        assert!(descriptor
            .profile
            .identity()
            .expect("profile")
            .starts_with("blake3:"));
    }

    #[test]
    fn stale_schema_and_version_are_denied() {
        let mut descriptor = sample();
        descriptor.schema = "chaoscontrol.native-process-descriptor.v0".to_string();
        assert!(descriptor.validate().unwrap_err().contains("schema"));
        let mut descriptor = sample();
        descriptor.profile.version = 0;
        assert!(descriptor.validate().unwrap_err().contains("version"));
    }

    #[test]
    fn order_drift_and_duplicate_sequence_are_denied() {
        let mut descriptor = sample();
        descriptor.records.swap(1, 2);
        assert!(descriptor.validate().unwrap_err().contains("order"));
        let mut descriptor = sample();
        descriptor.records[1].sequence = 1;
        assert!(descriptor.validate().unwrap_err().contains("sequence"));
    }

    #[test]
    fn over_bound_and_empty_record_sets_are_denied() {
        let mut descriptor = sample();
        descriptor.records.clear();
        assert!(descriptor.validate().unwrap_err().contains("record count"));
        let mut descriptor = sample();
        descriptor.records = (0..=MAX_NATIVE_PROCESS_RECORDS)
            .map(|index| record("vmm", 1, index as u64 + 1, 0, 1))
            .collect();
        assert!(descriptor.validate().unwrap_err().contains("record count"));
    }

    #[test]
    fn path_secret_and_nonclaim_leaks_are_denied() {
        let mut descriptor = sample();
        descriptor.records[0].process_key = "/usr/bin/vmm".to_string();
        assert!(descriptor.validate().unwrap_err().contains("host path"));
        let mut descriptor = sample();
        descriptor.records[0].process_key = "secret-worker".to_string();
        assert!(descriptor.validate().unwrap_err().contains("secret"));
        let mut descriptor = sample();
        descriptor.profile.non_claims.pop();
        assert!(descriptor.validate().unwrap_err().contains("non-claims"));
    }

    #[test]
    fn identity_refs_and_ticks_are_checked() {
        let mut descriptor = sample();
        descriptor.records[0].executable_blake3 = "sha256:00".to_string();
        assert!(descriptor.validate().unwrap_err().contains("BLAKE3"));
        let mut descriptor = sample();
        descriptor.records[0].end_tick = 0;
        descriptor.records[0].start_tick = 1;
        assert!(descriptor.validate().unwrap_err().contains("end tick"));
        let mut descriptor = sample();
        descriptor.records[0].evidence_refs =
            vec![format!("blake3:{}", "b".repeat(64)); MAX_NATIVE_PROCESS_EVIDENCE_REFS + 1];
        assert!(descriptor.validate().unwrap_err().contains("evidence"));
    }
}
