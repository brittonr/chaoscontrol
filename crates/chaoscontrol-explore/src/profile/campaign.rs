use super::{valid_identifier, ArtifactReference, RunProfile};

const CAMPAIGN_SCHEMA: &str = "chaoscontrol.campaign-profile.v1";
const CAMPAIGN_SCOPE: &str = "pre-run campaign intent; not thread start, VM start, completed seed, fault effect, replay, report, receipt, or accepted evidence";
const FAILURE_POLICY: &str = "fail-campaign";
const SEED_TEMPLATE: &str = "seed_{seed}";
const CHECKPOINT_NAME: &str = "campaign_progress.json";
const MAX_SEEDS: usize = 1024;
const MAX_WORKERS: usize = 256;
const PROBABILITY_TOTAL: f64 = 1.0;
const PROBABILITY_EPSILON: f64 = f64::EPSILON * 8.0;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignProfile {
    pub schema: String,
    pub campaign_id: String,
    pub seeds: Vec<u64>,
    pub run: RunProfile,
    pub workers: WorkerProfile,
    pub mutation: MutationProfile,
    pub scenario: Option<ArtifactReference>,
    pub metrics: MetricsProfile,
    pub output: OutputProfile,
    pub bounds: CampaignBounds,
    pub scope: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerProfile {
    pub count: usize,
    pub failure_policy: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct MutationProfile {
    pub maximum_tick: u64,
    pub add_probability: f64,
    pub remove_probability: f64,
    pub shift_probability: f64,
    pub replace_probability: f64,
    pub schedule_mutation_ratio: f64,
    pub havoc_after_stale: u64,
    pub havoc_minimum_mutations: u32,
    pub havoc_maximum_mutations: u32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsProfile {
    pub enabled: bool,
    pub output: Option<ArtifactReference>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputProfile {
    pub root: ArtifactReference,
    pub seed_directory_template: String,
    pub checkpoint_name: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignBounds {
    pub maximum_seeds: usize,
    pub maximum_workers: usize,
    pub maximum_total_memory_mib: u64,
}

#[derive(Debug, Clone)]
pub struct PreparedScenario {
    pub identity: String,
    pub config: ::chaoscontrol_fault::scenario::ScenarioConfig,
}

impl CampaignProfile {
    pub fn try_into_campaign_config(
        self,
        scenario: Option<PreparedScenario>,
    ) -> Result<crate::campaign::CampaignConfig, String> {
        self.validate()?;
        match (&self.scenario, &scenario) {
            (None, None) => {}
            (Some(reference), Some(prepared)) if reference.identity == prepared.identity => {}
            _ => {
                return Err(
                    "campaign scenario reference and prepared scenario identity differ".to_string(),
                )
            }
        }
        let output_dir = self.output.root.path.clone();
        let mut base = self.run.try_into_explorer_config(
            self.seeds[0],
            Some(format!(
                "{output_dir}/{}",
                self.output.seed_directory_template
            )),
        )?;
        base.num_workers = self.workers.count;
        base.mutation.max_tick = self.mutation.maximum_tick;
        base.mutation.add_prob = self.mutation.add_probability;
        base.mutation.remove_prob = self.mutation.remove_probability;
        base.mutation.shift_prob = self.mutation.shift_probability;
        base.mutation.replace_prob = self.mutation.replace_probability;
        base.mutation.schedule_mutation_ratio = self.mutation.schedule_mutation_ratio;
        base.havoc_after_stale = self.mutation.havoc_after_stale;
        base.havoc_mutations = [
            self.mutation.havoc_minimum_mutations,
            self.mutation.havoc_maximum_mutations,
        ];
        base.scenario = scenario.map(|prepared| prepared.config);
        base.emit_metrics = self.metrics.enabled;
        base.metrics_file = self
            .metrics
            .output
            .map(|reference| std::path::PathBuf::from(reference.path));
        Ok(crate::campaign::CampaignConfig {
            seeds: self.seeds,
            base_explorer_config: base,
            output_dir,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != CAMPAIGN_SCHEMA
            || self.scope != CAMPAIGN_SCOPE
            || !valid_identifier(&self.campaign_id)
        {
            return Err("campaign profile schema, identity, or scope is invalid".to_string());
        }
        self.run.validate()?;
        self.output.root.validate()?;
        if let Some(reference) = &self.scenario {
            reference.validate()?;
        }
        if let Some(reference) = &self.metrics.output {
            reference.validate()?;
        }
        if self.bounds.maximum_seeds == 0
            || self.bounds.maximum_seeds > MAX_SEEDS
            || self.bounds.maximum_workers == 0
            || self.bounds.maximum_workers > MAX_WORKERS
            || self.bounds.maximum_total_memory_mib == 0
        {
            return Err("campaign profile declared bounds are invalid".to_string());
        }
        let unique = self
            .seeds
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if self.seeds.is_empty()
            || self.seeds.len() > MAX_SEEDS
            || self.seeds.len() > self.bounds.maximum_seeds
            || unique.len() != self.seeds.len()
            || self.run.seed != self.seeds[0]
        {
            return Err("campaign profile seed set is invalid".to_string());
        }
        if self.workers.count == 0
            || self.workers.count > MAX_WORKERS
            || self.workers.count > self.bounds.maximum_workers
            || self.workers.count > self.seeds.len()
            || self.workers.failure_policy != FAILURE_POLICY
        {
            return Err("campaign profile worker plan is invalid".to_string());
        }
        let probabilities = [
            self.mutation.add_probability,
            self.mutation.remove_probability,
            self.mutation.shift_probability,
            self.mutation.replace_probability,
            self.mutation.schedule_mutation_ratio,
        ];
        if probabilities
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            || (probabilities[..4].iter().sum::<f64>() - PROBABILITY_TOTAL).abs()
                > PROBABILITY_EPSILON
            || self.mutation.maximum_tick == 0
            || self.mutation.havoc_minimum_mutations == 0
            || self.mutation.havoc_minimum_mutations > self.mutation.havoc_maximum_mutations
        {
            return Err("campaign profile mutation bounds are invalid".to_string());
        }
        let memory = self
            .run
            .topology
            .num_vms
            .checked_mul(self.run.topology.memory_mib)
            .ok_or_else(|| "campaign profile memory bound overflow".to_string())?;
        if memory > self.bounds.maximum_total_memory_mib
            || self.output.seed_directory_template != SEED_TEMPLATE
            || self.output.checkpoint_name != CHECKPOINT_NAME
            || self.metrics.enabled != self.metrics.output.is_some()
        {
            return Err("campaign profile output, metrics, or memory bound is invalid".to_string());
        }
        Ok(())
    }
}
