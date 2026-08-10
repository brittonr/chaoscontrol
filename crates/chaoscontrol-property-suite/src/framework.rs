// r[impl chaoscontrol.property_coverage.framework]
// r[impl chaoscontrol.property_coverage.invariants]
// r[impl chaoscontrol.property_coverage.shrink]
// r[impl chaoscontrol.property_coverage.validation]
use serde::{Deserialize, Serialize};

const PROFILE_SCHEMA_VERSION: u32 = 1;
const LCG_MULTIPLIER: u64 = 6_364_136_223_846_793_005;
const LCG_INCREMENT: u64 = 1_442_695_040_888_963_407;
const SEQUENCE_DOMAIN: u64 = 0x5052_4f50_4552_5459;
const REQUIRED_NON_CLAIM_FRAGMENTS: [&str; 5] = [
    "formal proof",
    "whole-system correctness",
    "exhaustive state coverage",
    "KVM behavior",
    "absence of defects",
];
const FORBIDDEN_CLAIM_FRAGMENTS: [&str; 3] =
    ["formal proof", "complete correctness", "absence of defects"];
const MIN_SHRINK_GRANULARITY: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyProfile {
    pub schema_version: u32,
    pub id: String,
    pub target_models: Vec<String>,
    pub seed_policy: String,
    pub seeds: Vec<u64>,
    pub sequence_count: usize,
    pub max_steps: usize,
    pub max_shrink_attempts: usize,
    pub max_retained_counterexamples: usize,
    pub max_runtime_seconds: u64,
    pub claim: String,
    pub non_claims: Vec<String>,
}

impl PropertyProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err("property profile schema version is not supported".to_string());
        }
        let unique_targets = self
            .target_models
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        let unique_seeds = self.seeds.iter().collect::<std::collections::BTreeSet<_>>();
        if self.id.is_empty()
            || self.target_models.is_empty()
            || unique_targets.len() != self.target_models.len()
            || self.seed_policy != "named-explicit"
            || self.seeds.is_empty()
            || unique_seeds.len() != self.seeds.len()
        {
            return Err(
                "property profile identity, target, seed policy, or seed set is invalid"
                    .to_string(),
            );
        }
        if self.sequence_count == 0
            || self.max_steps == 0
            || self.max_shrink_attempts == 0
            || self.max_retained_counterexamples == 0
            || self.max_runtime_seconds == 0
        {
            return Err("property profile bounds must be non-zero".to_string());
        }
        if self.claim.is_empty()
            || FORBIDDEN_CLAIM_FRAGMENTS
                .iter()
                .any(|fragment| self.claim.contains(fragment))
            || REQUIRED_NON_CLAIM_FRAGMENTS.iter().any(|fragment| {
                !self
                    .non_claims
                    .iter()
                    .any(|non_claim| non_claim.contains(fragment))
            })
        {
            return Err("property profile claim boundary is invalid".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Fast,
    Deep,
}

impl Lane {
    pub fn id(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Deep => "deep",
        }
    }
}

pub fn profile(lane: Lane) -> PropertyProfile {
    let profiles: Vec<PropertyProfile> = serde_json::from_str(include_str!(
        "../../../contracts/property-coverage/profiles.json"
    ))
    .expect("the committed property profiles must be valid JSON");
    let selected = profiles
        .into_iter()
        .find(|candidate| candidate.id == lane.id())
        .expect("the selected property lane must exist");
    selected
        .validate()
        .expect("the selected property lane must satisfy its bounds");
    selected
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub invariant: String,
    pub step: usize,
    pub detail: String,
}

impl Failure {
    pub fn new(invariant: impl Into<String>, step: usize, detail: impl Into<String>) -> Self {
        Self {
            invariant: invariant.into(),
            step,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Counterexample<C> {
    pub suite: String,
    pub seed: u64,
    pub invariant: String,
    pub original_steps: usize,
    pub minimized_steps: usize,
    pub shrink_attempts: usize,
    pub commands: Vec<C>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuiteReport {
    pub suite: String,
    pub sequences: usize,
    pub steps: usize,
    pub rejected_commands: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct StepResult {
    pub rejected: bool,
}

impl StepResult {
    pub const ACCEPTED: Self = Self { rejected: false };
    pub const REJECTED: Self = Self { rejected: true };
}

#[derive(Debug, Clone, Copy)]
pub struct DeterministicRng(u64);

impl DeterministicRng {
    pub fn new(seed: u64, sequence: usize) -> Self {
        let sequence_word = u64::try_from(sequence).expect("sequence count must fit in u64");
        Self(seed ^ sequence_word.wrapping_mul(SEQUENCE_DOMAIN))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(LCG_MULTIPLIER)
            .wrapping_add(LCG_INCREMENT);
        self.0
    }

    pub fn index(&mut self, upper: usize) -> usize {
        assert!(upper > 0, "random index upper bound must be non-zero");
        let upper_u64 = u64::try_from(upper).expect("random index bound must fit in u64");
        usize::try_from(self.next_u64() % upper_u64).expect("bounded index must fit in usize")
    }

    pub fn bounded_u64(&mut self, upper: u64) -> u64 {
        assert!(upper > 0, "random numeric bound must be non-zero");
        self.next_u64() % upper
    }

    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

pub fn run_generated<C, G, F>(
    suite: &str,
    selected: &PropertyProfile,
    mut generate: G,
    check: F,
) -> Result<SuiteReport, Box<Counterexample<C>>>
where
    C: Clone + Serialize,
    G: FnMut(&mut DeterministicRng) -> C,
    F: Fn(&[C]) -> Result<usize, Failure>,
{
    let mut total_steps = 0_usize;
    let mut rejected_commands = 0_usize;
    let mut sequence_total = 0_usize;
    for base_seed in &selected.seeds {
        for sequence in 0..selected.sequence_count {
            let mut rng = DeterministicRng::new(*base_seed, sequence);
            let step_count = rng.index(selected.max_steps) + 1;
            let commands = (0..step_count)
                .map(|_| generate(&mut rng))
                .collect::<Vec<_>>();
            match check(&commands) {
                Ok(rejected) => {
                    total_steps += commands.len();
                    rejected_commands += rejected;
                    sequence_total += 1;
                }
                Err(failure) => {
                    return Err(Box::new(shrink_counterexample(
                        suite,
                        *base_seed,
                        commands,
                        failure,
                        selected.max_shrink_attempts,
                        &check,
                    )));
                }
            }
        }
    }
    Ok(SuiteReport {
        suite: suite.to_string(),
        sequences: sequence_total,
        steps: total_steps,
        rejected_commands,
    })
}

fn shrink_counterexample<C, F>(
    suite: &str,
    seed: u64,
    original: Vec<C>,
    initial_failure: Failure,
    max_attempts: usize,
    check: &F,
) -> Counterexample<C>
where
    C: Clone + Serialize,
    F: Fn(&[C]) -> Result<usize, Failure>,
{
    let original_steps = original.len();
    let mut minimized = original;
    let mut attempts = 0_usize;
    let mut granularity = MIN_SHRINK_GRANULARITY;
    while minimized.len() > 1 && attempts < max_attempts {
        let chunk_size = minimized.len().div_ceil(granularity);
        let mut reduced = false;
        let mut start = 0_usize;
        while start < minimized.len() && attempts < max_attempts {
            let end = usize::min(start + chunk_size, minimized.len());
            let mut candidate = Vec::with_capacity(minimized.len() - (end - start));
            candidate.extend_from_slice(&minimized[..start]);
            candidate.extend_from_slice(&minimized[end..]);
            attempts += 1;
            if !candidate.is_empty()
                && matches!(check(&candidate), Err(ref failure) if failure.invariant == initial_failure.invariant)
            {
                minimized = candidate;
                granularity = MIN_SHRINK_GRANULARITY;
                reduced = true;
                break;
            }
            start = end;
        }
        if !reduced {
            if granularity >= minimized.len() {
                break;
            }
            granularity = usize::min(
                minimized.len(),
                granularity.saturating_mul(MIN_SHRINK_GRANULARITY),
            );
        }
    }
    let final_failure = check(&minimized).expect_err("a minimized counterexample must still fail");
    Counterexample {
        suite: suite.to_string(),
        seed,
        invariant: final_failure.invariant,
        original_steps,
        minimized_steps: minimized.len(),
        shrink_attempts: attempts,
        commands: minimized,
        detail: final_failure.detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SHRINK_ATTEMPTS: usize = 32;
    const FAILURE_COMMAND: u8 = 2;
    const TRAILING_COMMAND: u8 = 3;

    #[test]
    fn shrink_preserves_failure_meaning() {
        let commands = vec![0_u8, 1, FAILURE_COMMAND, TRAILING_COMMAND];
        let failure = Failure::new("contains-two", 2, "two is present");
        let counterexample = shrink_counterexample(
            "framework",
            0,
            commands,
            failure,
            TEST_SHRINK_ATTEMPTS,
            &|candidate| {
                if candidate.contains(&FAILURE_COMMAND) {
                    Err(Failure::new("contains-two", 0, "two is present"))
                } else {
                    Ok(0)
                }
            },
        );
        assert_eq!(counterexample.commands, vec![FAILURE_COMMAND]);
        assert_eq!(counterexample.invariant, "contains-two");
    }

    #[test]
    fn profile_rejects_zero_bounds() {
        let mut selected = profile(Lane::Fast);
        selected.max_steps = 0;
        assert!(selected.validate().is_err());
    }

    #[test]
    fn profile_rejects_overclaims() {
        let mut selected = profile(Lane::Fast);
        selected.claim = "This is a formal proof.".to_string();
        assert!(selected.validate().is_err());
    }
}
