use crate::assertion_summary_validation::validate_fatal_details;
use crate::campaign::CampaignReport;
use crate::explorer::{AssertionDetail, ExplorationReport};
use chaoscontrol_protocol::admission::CatalogValidationStatus;
use serde::{Deserialize, Deserializer, Serialize};

pub(crate) use crate::assertion_summary_validation::{
    derive_detail_verdict, validate_assertion_details,
};

pub const ASSERTION_SUMMARY_SCHEMA: &str = "chaoscontrol.assertion-summary.v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionSummaryV2 {
    schema: String,
    catalog_status: CatalogValidationStatus,
    collision_safe_evidence: bool,
    assertions: Vec<AssertionDetail>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAssertionSummaryV2 {
    schema: String,
    catalog_status: CatalogValidationStatus,
    collision_safe_evidence: bool,
    assertions: Vec<AssertionDetail>,
}

impl AssertionSummaryV2 {
    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn catalog_status(&self) -> CatalogValidationStatus {
        self.catalog_status
    }

    pub fn collision_safe_evidence(&self) -> bool {
        self.collision_safe_evidence
    }

    pub fn assertions(&self) -> &[AssertionDetail] {
        &self.assertions
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.schema != ASSERTION_SUMMARY_SCHEMA {
            return Err("unsupported assertion summary schema".to_string());
        }
        Self::recomputed_claims(
            self.catalog_status,
            self.collision_safe_evidence,
            &[],
            &self.assertions,
        )?;
        Ok(())
    }

    pub fn from_exploration(report: &ExplorationReport) -> Result<Self, String> {
        Self::new(
            report.assertion_catalog_status,
            report.collision_safe_assertion_evidence,
            &report.assertion_identity_conflicts,
            &report.assertion_details,
        )
    }

    pub fn from_campaign(report: &CampaignReport) -> Result<Self, String> {
        Self::new(
            report.assertion_catalog_status,
            report.collision_safe_assertion_evidence,
            &report.assertion_identity_conflicts,
            &report.assertion_details,
        )
    }

    pub fn fatal(assertions: &[AssertionDetail]) -> Result<Self, String> {
        validate_fatal_details(assertions)?;
        Ok(Self {
            schema: ASSERTION_SUMMARY_SCHEMA.to_string(),
            catalog_status: CatalogValidationStatus::FatalConflict,
            collision_safe_evidence: false,
            assertions: assertions.to_vec(),
        })
    }

    fn from_claims(
        schema: String,
        source_status: CatalogValidationStatus,
        source_collision_safe: bool,
        assertions: Vec<AssertionDetail>,
    ) -> Result<Self, String> {
        if schema != ASSERTION_SUMMARY_SCHEMA {
            return Err("unsupported assertion summary schema".to_string());
        }
        let (catalog_status, collision_safe_evidence) =
            Self::recomputed_claims(source_status, source_collision_safe, &[], &assertions)?;
        if catalog_status != source_status || collision_safe_evidence != source_collision_safe {
            return Err("assertion summary authority claim is not exact".to_string());
        }
        Ok(Self {
            schema,
            catalog_status,
            collision_safe_evidence,
            assertions,
        })
    }

    fn new(
        source_status: CatalogValidationStatus,
        source_collision_safe: bool,
        conflicts: &[String],
        assertions: &[AssertionDetail],
    ) -> Result<Self, String> {
        let (catalog_status, collision_safe_evidence) =
            Self::recomputed_claims(source_status, source_collision_safe, conflicts, assertions)?;
        Ok(Self {
            schema: ASSERTION_SUMMARY_SCHEMA.to_string(),
            catalog_status,
            collision_safe_evidence,
            assertions: assertions.to_vec(),
        })
    }

    fn recomputed_claims(
        source_status: CatalogValidationStatus,
        source_collision_safe: bool,
        conflicts: &[String],
        assertions: &[AssertionDetail],
    ) -> Result<(CatalogValidationStatus, bool), String> {
        if assertions.is_empty() {
            return Err("empty assertion summaries are not emitted".to_string());
        }
        if source_status == CatalogValidationStatus::FatalConflict || !conflicts.is_empty() {
            validate_fatal_details(assertions)?;
            return Ok((CatalogValidationStatus::FatalConflict, false));
        }
        let recomputed_status = validate_assertion_details(assertions)?;
        match recomputed_status {
            CatalogValidationStatus::Accepted
                if source_status == CatalogValidationStatus::Accepted && source_collision_safe =>
            {
                Ok((CatalogValidationStatus::Accepted, true))
            }
            CatalogValidationStatus::Accepted => {
                Ok((CatalogValidationStatus::FatalConflict, false))
            }
            CatalogValidationStatus::LegacyAmbiguous
                if source_status != CatalogValidationStatus::Accepted && !source_collision_safe =>
            {
                Ok((CatalogValidationStatus::LegacyAmbiguous, false))
            }
            CatalogValidationStatus::LegacyAmbiguous => {
                Ok((CatalogValidationStatus::FatalConflict, false))
            }
            _ => Err("unsupported assertion summary classification".to_string()),
        }
    }
}

impl<'de> Deserialize<'de> for AssertionSummaryV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawAssertionSummaryV2::deserialize(deserializer)?;
        Self::from_claims(
            raw.schema,
            raw.catalog_status,
            raw.collision_safe_evidence,
            raw.assertions,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(id: u32) -> AssertionDetail {
        AssertionDetail {
            process_instances: Default::default(),
            id,
            identity: None,
            message: format!("assertion_{id}"),
            kind: "always".to_string(),
            guest: "uncategorized".to_string(),
            category: "uncategorized".to_string(),
            verdict: "passed".to_string(),
            hit_count: 1,
            true_count: 1,
            false_count: 0,
            last_failure_details: None,
        }
    }

    #[test]
    fn legacy_summary_round_trips_and_validates() {
        let summary = AssertionSummaryV2::new(
            CatalogValidationStatus::LegacyAmbiguous,
            false,
            &[],
            &[detail(1)],
        )
        .expect("legacy summary");
        assert_eq!(
            summary.catalog_status(),
            CatalogValidationStatus::LegacyAmbiguous
        );
        summary.validate().expect("summary validates");
    }

    #[test]
    fn duplicate_legacy_ids_are_rejected() {
        assert!(AssertionSummaryV2::new(
            CatalogValidationStatus::LegacyAmbiguous,
            false,
            &[],
            &[detail(1), detail(1)],
        )
        .is_err());
    }
}
