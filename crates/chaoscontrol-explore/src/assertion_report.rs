use crate::explorer::{AssertionDetail, AssertionIdentityDetail, AssertionStats};
use chaoscontrol_fault::oracle::{OracleReport, Verdict};
use chaoscontrol_fault::oracle_validation::OracleValidationError;
use chaoscontrol_protocol::admission::CatalogValidationStatus;

const FAILED_SORT_RANK: u8 = 0;
const UNEXERCISED_SORT_RANK: u8 = 1;
const PASSED_SORT_RANK: u8 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct AssertionReportProjection {
    pub stats: AssertionStats,
    pub details: Vec<AssertionDetail>,
    pub catalog_status: CatalogValidationStatus,
    pub collision_safe_evidence: bool,
}

pub fn strict_projection(
    report: &OracleReport,
) -> Result<AssertionReportProjection, OracleValidationError> {
    chaoscontrol_fault::oracle_validation::validate_oracle_report_claim(report)?;
    let mut passed = 0_usize;
    let mut failed = 0_usize;
    let mut unexercised = 0_usize;
    let mut details = Vec::with_capacity(report.structured_assertions.len());
    for record in report.structured_assertions.values() {
        let verdict = record.verdict();
        match verdict {
            Verdict::Passed => passed += 1,
            Verdict::Failed => failed += 1,
            Verdict::Unexercised => unexercised += 1,
        }
        let admitted = record
            .identity
            .as_ref()
            .ok_or(OracleValidationError::Record)?;
        let failure_details = record
            .last_failure_details
            .as_ref()
            .and_then(|bytes| std::str::from_utf8(bytes).ok().map(str::to_string));
        details.push(AssertionDetail {
            id: record.compatibility_id.unwrap_or_default(),
            identity: Some(AssertionIdentityDetail {
                descriptor: admitted.descriptor.clone(),
                fingerprint: admitted.fingerprint,
                canonical_descriptor: chaoscontrol_protocol::identity::encode_lower_hex(
                    &admitted.canonical_bytes,
                ),
                catalog_tokens: record.catalog_tokens.iter().copied().collect(),
            }),
            message: record.message.clone(),
            kind: format!("{:?}", record.kind).to_lowercase(),
            guest: record.guest.clone(),
            category: record.category.clone(),
            verdict: format!("{verdict:?}").to_lowercase(),
            hit_count: record.hit_count,
            true_count: record.true_count,
            false_count: record.false_count,
            last_failure_details: failure_details,
        });
    }
    details.sort_by(|left, right| {
        verdict_rank(&left.verdict)
            .cmp(&verdict_rank(&right.verdict))
            .then(
                left.identity
                    .as_ref()
                    .map(|identity| identity.fingerprint)
                    .cmp(&right.identity.as_ref().map(|identity| identity.fingerprint)),
            )
            .then(left.id.cmp(&right.id))
    });
    Ok(AssertionReportProjection {
        stats: AssertionStats {
            catalog_size: details.len(),
            passed,
            failed,
            unexercised,
        },
        details,
        catalog_status: CatalogValidationStatus::Accepted,
        collision_safe_evidence: true,
    })
}

fn verdict_rank(verdict: &str) -> u8 {
    match verdict {
        "failed" => FAILED_SORT_RANK,
        "unexercised" => UNEXERCISED_SORT_RANK,
        _ => PASSED_SORT_RANK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_fault::oracle::PropertyOracle;
    use chaoscontrol_protocol::admission::{token_for_descriptors, CatalogBuilder};
    use chaoscontrol_protocol::identity::{
        AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
    };

    const TEST_ALIAS: u32 = 7;

    fn accepted_report() -> OracleReport {
        let descriptor = AssertionDescriptor {
            identity_version: ASSERTION_IDENTITY_VERSION,
            namespace: "org.example.report".to_string(),
            logical_key: AssertionLogicalKey::Stable {
                key: "report-assertion".to_string(),
            },
            compatibility_id: Some(TEST_ALIAS),
            kind: AssertionKind::Always,
            message: "report assertion".to_string(),
            source_file: "src/report.rs".to_string(),
            source_line: 1,
            source_column: 1,
            guest: "guest".to_string(),
            category: "invariant".to_string(),
        };
        let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
        let mut builder = CatalogBuilder::begin(1).expect("catalog begins");
        builder.insert(descriptor).expect("descriptor inserts");
        let catalog = builder.complete(token).expect("catalog completes");
        let mut oracle = PropertyOracle::new();
        oracle.activate_catalog(catalog).expect("catalog activates");
        oracle.report()
    }

    #[test]
    fn projects_a_validated_accepted_report() {
        let projection = strict_projection(&accepted_report()).expect("report projects");

        assert_eq!(projection.catalog_status, CatalogValidationStatus::Accepted);
        assert!(projection.collision_safe_evidence);
        assert_eq!(projection.details.len(), 1);
        assert_eq!(projection.details[0].id, TEST_ALIAS);
    }

    #[test]
    fn rejects_forged_report_metadata() {
        let mut report = accepted_report();
        let record = report
            .structured_assertions
            .values_mut()
            .next()
            .expect("record");
        record.message = "forged".to_string();

        assert!(strict_projection(&report).is_err());
    }
}
