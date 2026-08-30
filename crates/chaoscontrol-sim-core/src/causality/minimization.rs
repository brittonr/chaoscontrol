use super::model::{domain_hash, validate_steps, CausalityError, InterleavingStep};
use serde::{Deserialize, Serialize};

const MINIMUM_GRANULARITY: usize = 2;
const CANDIDATE_DOMAIN: &[u8] = b"chaoscontrol.causality.minimization-candidate.v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinimizationCandidate {
    pub candidate_blake3: String,
    pub steps: Vec<InterleavingStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinimizationResult {
    pub minimized_steps: Vec<InterleavingStep>,
    pub executions_spent: u64,
    pub budget: u64,
    pub complete: bool,
    pub budget_exhausted: bool,
}

#[derive(Debug, Clone)]
pub struct DdminState {
    current: Vec<InterleavingStep>,
    granularity: usize,
    chunk_index: usize,
    executions_spent: u64,
    budget: u64,
    tested_empty: bool,
    pending: Option<MinimizationCandidate>,
    complete: bool,
    budget_exhausted: bool,
}

impl DdminState {
    pub fn new(steps: Vec<InterleavingStep>, budget: u64) -> Result<Self, CausalityError> {
        validate_steps(&steps)?;
        if budget == 0 {
            return Err(CausalityError::new(
                "causality-budget",
                "minimization budget must be positive",
            ));
        }
        Ok(Self {
            current: steps,
            granularity: MINIMUM_GRANULARITY,
            chunk_index: 0,
            executions_spent: 0,
            budget,
            tested_empty: false,
            pending: None,
            complete: false,
            budget_exhausted: false,
        })
    }

    pub fn next_candidate(&mut self) -> Result<Option<MinimizationCandidate>, CausalityError> {
        if self.pending.is_some() {
            return Err(CausalityError::new(
                "causality-minimization-state",
                "the prior candidate has no execution outcome",
            ));
        }
        if self.complete {
            return Ok(None);
        }
        if self.executions_spent >= self.budget {
            self.complete = true;
            self.budget_exhausted = true;
            return Ok(None);
        }
        if !self.tested_empty {
            self.tested_empty = true;
            let candidate = candidate(Vec::new())?;
            self.pending = Some(candidate.clone());
            return Ok(Some(candidate));
        }
        loop {
            if self.current.len() < MINIMUM_GRANULARITY {
                self.complete = true;
                return Ok(None);
            }
            let chunks = self.granularity.min(self.current.len());
            if self.chunk_index >= chunks {
                if chunks >= self.current.len() {
                    self.complete = true;
                    return Ok(None);
                }
                self.granularity = self
                    .current
                    .len()
                    .min(self.granularity.saturating_mul(MINIMUM_GRANULARITY));
                self.chunk_index = 0;
                continue;
            }
            let start = self.chunk_index * self.current.len() / chunks;
            let end = (self.chunk_index + 1) * self.current.len() / chunks;
            let steps = self
                .current
                .iter()
                .enumerate()
                .filter(|(index, _)| *index < start || *index >= end)
                .map(|(_, step)| step.clone())
                .collect::<Vec<_>>();
            let candidate = candidate(steps)?;
            self.pending = Some(candidate.clone());
            return Ok(Some(candidate));
        }
    }

    pub fn record_outcome(
        &mut self,
        candidate_blake3: &str,
        reproduced: bool,
    ) -> Result<(), CausalityError> {
        let candidate = self.pending.take().ok_or_else(|| {
            CausalityError::new(
                "causality-minimization-state",
                "execution outcome has no pending candidate",
            )
        })?;
        if candidate.candidate_blake3 != candidate_blake3 {
            self.pending = Some(candidate);
            return Err(CausalityError::new(
                "causality-minimization-identity",
                "execution outcome does not match the pending candidate",
            ));
        }
        let empty_probe = candidate.steps.is_empty() && !self.current.is_empty();
        self.executions_spent = self.executions_spent.checked_add(1).ok_or_else(|| {
            CausalityError::new(
                "causality-budget",
                "minimization execution counter overflow",
            )
        })?;
        if reproduced {
            self.current = candidate.steps;
            self.granularity = self
                .current
                .len()
                .min(self.granularity.saturating_sub(1).max(MINIMUM_GRANULARITY));
            self.chunk_index = 0;
            if self.current.is_empty() {
                self.complete = true;
            }
        } else if !empty_probe {
            self.chunk_index = self.chunk_index.saturating_add(1);
        }
        Ok(())
    }

    pub fn result(&self) -> MinimizationResult {
        MinimizationResult {
            minimized_steps: self.current.clone(),
            executions_spent: self.executions_spent,
            budget: self.budget,
            complete: self.complete && !self.budget_exhausted,
            budget_exhausted: self.budget_exhausted,
        }
    }
}

fn candidate(steps: Vec<InterleavingStep>) -> Result<MinimizationCandidate, CausalityError> {
    let bytes = serde_json::to_vec(&steps).map_err(|error| {
        CausalityError::new("causality-minimization-serialization", error.to_string())
    })?;
    Ok(MinimizationCandidate {
        candidate_blake3: domain_hash(CANDIDATE_DOMAIN, &bytes),
        steps,
    })
}
