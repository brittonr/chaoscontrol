use serde::{de::Deserialize, Deserializer};

pub(crate) use crate::assertion_summary_validation::{
    derive_detail_verdict, validate_assertion_details,
};

pub const ASSERTION_SUMMARY_SCHEMA: &str = "chaoscontrol.assertion-summary.v2";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionSummaryV2 {
    schema: String,
    catalog_status: ::chaoscontrol_protocol::admission::CatalogValidationStatus,
    collision_safe_evidence: bool,
    assertions: Vec<crate::explorer::AssertionDetail>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAssertionSummaryV2 {
    schema: String,
    catalog_status: ::chaoscontrol_protocol::admission::CatalogValidationStatus,
    collision_safe_evidence: bool,
    assertions: Vec<crate::explorer::AssertionDetail>,
}

impl AssertionSummaryV2 {
    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn catalog_status(&self) -> ::chaoscontrol_protocol::admission::CatalogValidationStatus {
        self.catalog_status
    }

    pub fn collision_safe_evidence(&self) -> bool {
        self.collision_safe_evidence
    }

    pub fn assertions(&self) -> &[crate::explorer::AssertionDetail] {
        &self.assertions
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        Self::from_claims(
            self.schema.clone(),
            self.catalog_status,
            self.collision_safe_evidence,
            self.assertions.clone(),
        )?;
        Ok(())
    }

    pub fn from_exploration(report: &crate::explorer::ExplorationReport) -> Result<Self, String> {
        Self::new(
            report.assertion_catalog_status,
            report.collision_safe_assertion_evidence,
            &report.assertion_identity_conflicts,
            &report.assertion_details,
        )
    }

    pub fn from_campaign(report: &crate::campaign::CampaignReport) -> Result<Self, String> {
        Self::new(
            report.assertion_catalog_status,
            report.collision_safe_assertion_evidence,
            &report.assertion_identity_conflicts,
            &report.assertion_details,
        )
    }

    pub fn fatal(assertions: &[crate::explorer::AssertionDetail]) -> Result<Self, String> {
        crate::assertion_summary_validation::validate_fatal_details(assertions)?;
        Ok(Self {
            schema: ASSERTION_SUMMARY_SCHEMA.to_string(),
            catalog_status:
                ::chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict,
            collision_safe_evidence: false,
            assertions: assertions.to_vec(),
        })
    }

    fn from_claims(
        schema: String,
        source_status: ::chaoscontrol_protocol::admission::CatalogValidationStatus,
        source_collision_safe: bool,
        assertions: Vec<crate::explorer::AssertionDetail>,
    ) -> Result<Self, String> {
        if schema != ASSERTION_SUMMARY_SCHEMA {
            return Err("unsupported assertion summary schema".to_string());
        }
        let validated = Self::new(source_status, source_collision_safe, &[], &assertions)?;
        if validated.catalog_status != source_status
            || validated.collision_safe_evidence != source_collision_safe
        {
            return Err("assertion summary authority claim is not exact".to_string());
        }
        Ok(validated)
    }

    fn new(
        source_status: ::chaoscontrol_protocol::admission::CatalogValidationStatus,
        source_collision_safe: bool,
        conflicts: &[String],
        assertions: &[crate::explorer::AssertionDetail],
    ) -> Result<Self, String> {
        if assertions.is_empty() {
            return Err("empty assertion summaries are not emitted".to_string());
        }
        if source_status
            == ::chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict
            || !conflicts.is_empty()
        {
            return Self::fatal(assertions);
        }
        let recomputed_status = validate_assertion_details(assertions)?;
        let (catalog_status, collision_safe_evidence) = match recomputed_status {
            ::chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted
                if source_status
                    == ::chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted
                    && source_collision_safe =>
            {
                (
                    ::chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted,
                    true,
                )
            }
            ::chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted => (
                ::chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict,
                false,
            ),
            ::chaoscontrol_protocol::admission::CatalogValidationStatus::LegacyAmbiguous
                if source_status
                    != ::chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted
                    && !source_collision_safe =>
            {
                (
                    ::chaoscontrol_protocol::admission::CatalogValidationStatus::LegacyAmbiguous,
                    false,
                )
            }
            ::chaoscontrol_protocol::admission::CatalogValidationStatus::LegacyAmbiguous => (
                ::chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict,
                false,
            ),
            _ => return Err("unsupported assertion summary classification".to_string()),
        };
        Ok(Self {
            schema: ASSERTION_SUMMARY_SCHEMA.to_string(),
            catalog_status,
            collision_safe_evidence,
            assertions: assertions.to_vec(),
        })
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
