use std::fmt;

pub const MAX_FINDABILITY_SUBTREES: usize = 16_384;
pub const MAX_BUG_INSTANCES_PER_SUBTREE: usize = 4_096;
const MAX_IDENTIFIER_BYTES: usize = 256;
pub(crate) const MAX_EXACT_F64_INTEGER: u64 = 9_007_199_254_740_992;
const BLAKE3_HEX_BYTES: usize = 64;
const BLAKE3_PREFIX: &str = "blake3:";
const OBSERVATION_DOMAIN: &[u8] = b"chaoscontrol.findability.observation.v1\0";
const OBSERVATION_SET_DOMAIN: &[u8] = b"chaoscontrol.findability.observation-set.v1\0";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubtreeObservation {
    pub generation_id: String,
    pub subtree_id: String,
    pub independence_group: String,
    pub observed_time: u64,
    pub source_blake3: String,
    pub bugs: Vec<BugInstance>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BugInstance {
    pub found_at: u64,
    pub bug_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssembledObservation {
    pub generation_id: String,
    pub subtree_id: String,
    pub independence_group: String,
    pub exposure: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_bug_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_bug_blake3: Option<String>,
    pub discarded_bug_instances: usize,
    pub source_blake3: String,
    pub observation_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindabilityError {
    pub class: &'static str,
    pub subtree_id: Option<String>,
    pub detail: String,
}

impl FindabilityError {
    pub(crate) fn new(class: &'static str, detail: impl Into<String>) -> Self {
        Self {
            class,
            subtree_id: None,
            detail: detail.into(),
        }
    }

    fn subtree(class: &'static str, subtree_id: &str, detail: impl Into<String>) -> Self {
        Self {
            class,
            subtree_id: Some(subtree_id.to_string()),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for FindabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.subtree_id.as_deref() {
            Some(subtree_id) => write!(
                formatter,
                "{} at subtree {}: {}",
                self.class, subtree_id, self.detail
            ),
            None => write!(formatter, "{}: {}", self.class, self.detail),
        }
    }
}

impl std::error::Error for FindabilityError {}

pub fn assemble_observations(
    subtrees: &[SubtreeObservation],
) -> Result<Vec<AssembledObservation>, FindabilityError> {
    if subtrees.is_empty() {
        return Err(FindabilityError::new(
            "observation-empty",
            "at least one subtree observation is required",
        ));
    }
    if subtrees.len() > MAX_FINDABILITY_SUBTREES {
        return Err(FindabilityError::new(
            "observation-bound",
            "subtree count exceeds the supported bound",
        ));
    }
    let mut subtree_ids = std::collections::BTreeSet::new();
    let mut generation = None;
    let mut assembled = Vec::with_capacity(subtrees.len());
    for subtree in subtrees {
        validate_subtree(subtree)?;
        if !subtree_ids.insert(subtree.subtree_id.as_str()) {
            return Err(FindabilityError::subtree(
                "observation-subtree",
                &subtree.subtree_id,
                "duplicate subtree observation",
            ));
        }
        if generation
            .replace(subtree.generation_id.as_str())
            .is_some_and(|prior| prior != subtree.generation_id)
        {
            return Err(FindabilityError::new(
                "observation-generation",
                "one report cannot mix run generations",
            ));
        }
        let mut bugs = subtree.bugs.clone();
        bugs.sort();
        let first = bugs.first();
        let exposure = first.map_or(subtree.observed_time, |bug| bug.found_at);
        let discarded_bug_instances = bugs.len().saturating_sub(usize::from(first.is_some()));
        let mut observation = AssembledObservation {
            generation_id: subtree.generation_id.clone(),
            subtree_id: subtree.subtree_id.clone(),
            independence_group: subtree.independence_group.clone(),
            exposure,
            first_bug_at: first.map(|bug| bug.found_at),
            first_bug_blake3: first.map(|bug| bug.bug_blake3.clone()),
            discarded_bug_instances,
            source_blake3: subtree.source_blake3.clone(),
            observation_blake3: String::new(),
        };
        observation.observation_blake3 = observation_identity(&observation)?;
        assembled.push(observation);
    }
    assembled.sort_by(|left, right| left.subtree_id.cmp(&right.subtree_id));
    Ok(assembled)
}

pub fn observation_set_identity(
    observations: &[AssembledObservation],
) -> Result<String, FindabilityError> {
    validate_assembled(observations)?;
    let mut canonical = observations.to_vec();
    canonical.sort_by(|left, right| left.subtree_id.cmp(&right.subtree_id));
    let bytes = serde_json::to_vec(&canonical).map_err(|error| {
        FindabilityError::new("observation-set-serialization", error.to_string())
    })?;
    Ok(domain_hash(OBSERVATION_SET_DOMAIN, &bytes))
}

pub(crate) fn validate_assembled(
    observations: &[AssembledObservation],
) -> Result<(), FindabilityError> {
    if observations.is_empty() || observations.len() > MAX_FINDABILITY_SUBTREES {
        return Err(FindabilityError::new(
            "observation-bound",
            "assembled observations are empty or exceed the supported bound",
        ));
    }
    let mut subtrees = std::collections::BTreeSet::new();
    let mut generation = None;
    let mut previous = None;
    for observation in observations {
        validate_identifier("generation_id", &observation.generation_id)?;
        validate_identifier("subtree_id", &observation.subtree_id)?;
        validate_identifier("independence_group", &observation.independence_group)?;
        validate_digest("source_blake3", &observation.source_blake3)?;
        validate_digest("observation_blake3", &observation.observation_blake3)?;
        if observation.exposure == 0 || observation.exposure > MAX_EXACT_F64_INTEGER {
            return Err(FindabilityError::subtree(
                "observation-exposure",
                &observation.subtree_id,
                "exposure must be positive and exactly representable by the model",
            ));
        }
        match (
            observation.first_bug_at,
            observation.first_bug_blake3.as_deref(),
        ) {
            (None, None) => {}
            (Some(found_at), Some(bug_blake3)) => {
                if found_at == 0 || found_at != observation.exposure {
                    return Err(FindabilityError::subtree(
                        "observation-bug-time",
                        &observation.subtree_id,
                        "first bug time must be positive and equal the censored exposure",
                    ));
                }
                validate_digest("first_bug_blake3", bug_blake3)?;
            }
            _ => {
                return Err(FindabilityError::subtree(
                    "observation-bug-binding",
                    &observation.subtree_id,
                    "first bug time and identity must be present together",
                ));
            }
        }
        if !subtrees.insert(observation.subtree_id.as_str()) {
            return Err(FindabilityError::subtree(
                "observation-subtree",
                &observation.subtree_id,
                "duplicate subtree observation",
            ));
        }
        if generation
            .replace(observation.generation_id.as_str())
            .is_some_and(|prior| prior != observation.generation_id)
        {
            return Err(FindabilityError::new(
                "observation-generation",
                "one report cannot mix run generations",
            ));
        }
        if previous.is_some_and(|prior: &str| prior >= observation.subtree_id.as_str()) {
            return Err(FindabilityError::new(
                "observation-order",
                "assembled observations are not in canonical subtree order",
            ));
        }
        previous = Some(observation.subtree_id.as_str());
        let expected = observation_identity(observation)?;
        if expected != observation.observation_blake3 {
            return Err(FindabilityError::subtree(
                "observation-identity",
                &observation.subtree_id,
                "observation BLAKE3 identity drifted",
            ));
        }
    }
    Ok(())
}

fn validate_subtree(subtree: &SubtreeObservation) -> Result<(), FindabilityError> {
    validate_identifier("generation_id", &subtree.generation_id)?;
    validate_identifier("subtree_id", &subtree.subtree_id)?;
    validate_identifier("independence_group", &subtree.independence_group)?;
    validate_digest("source_blake3", &subtree.source_blake3)?;
    if subtree.observed_time == 0 || subtree.observed_time > MAX_EXACT_F64_INTEGER {
        return Err(FindabilityError::subtree(
            "observation-time",
            &subtree.subtree_id,
            "observed time must be positive and exactly representable by the model",
        ));
    }
    if subtree.bugs.len() > MAX_BUG_INSTANCES_PER_SUBTREE {
        return Err(FindabilityError::subtree(
            "observation-bug-bound",
            &subtree.subtree_id,
            "bug instance count exceeds the supported bound",
        ));
    }
    for bug in &subtree.bugs {
        if bug.found_at == 0 || bug.found_at > subtree.observed_time {
            return Err(FindabilityError::subtree(
                "observation-bug-time",
                &subtree.subtree_id,
                "bug time must be positive and within the subtree horizon",
            ));
        }
        validate_digest("bug_blake3", &bug.bug_blake3)?;
    }
    Ok(())
}

fn observation_identity(observation: &AssembledObservation) -> Result<String, FindabilityError> {
    #[derive(serde::Serialize)]
    struct Material<'a> {
        generation_id: &'a str,
        subtree_id: &'a str,
        independence_group: &'a str,
        exposure: u64,
        first_bug_at: Option<u64>,
        first_bug_blake3: Option<&'a str>,
        discarded_bug_instances: usize,
        source_blake3: &'a str,
    }
    let material = Material {
        generation_id: &observation.generation_id,
        subtree_id: &observation.subtree_id,
        independence_group: &observation.independence_group,
        exposure: observation.exposure,
        first_bug_at: observation.first_bug_at,
        first_bug_blake3: observation.first_bug_blake3.as_deref(),
        discarded_bug_instances: observation.discarded_bug_instances,
        source_blake3: &observation.source_blake3,
    };
    let bytes = serde_json::to_vec(&material)
        .map_err(|error| FindabilityError::new("observation-serialization", error.to_string()))?;
    Ok(domain_hash(OBSERVATION_DOMAIN, &bytes))
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), FindabilityError> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(FindabilityError::new(
            "identifier",
            format!("{field} is empty or exceeds the supported bound"),
        ));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
    }) {
        return Err(FindabilityError::new(
            "identifier",
            format!("{field} contains a non-canonical byte"),
        ));
    }
    Ok(())
}

fn validate_digest(field: &'static str, value: &str) -> Result<(), FindabilityError> {
    let Some(hex) = value.strip_prefix(BLAKE3_PREFIX) else {
        return Err(FindabilityError::new(
            "digest",
            format!("{field} must use a BLAKE3 identity"),
        ));
    };
    if hex.len() != BLAKE3_HEX_BYTES
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(FindabilityError::new(
            "digest",
            format!("{field} has malformed lowercase BLAKE3 hex"),
        ));
    }
    Ok(())
}

fn domain_hash(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
    format!("{BLAKE3_PREFIX}{}", hasher.finalize().to_hex())
}
