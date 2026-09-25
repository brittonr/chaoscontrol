//! Campaign and Choregraph structural artifacts never count as product evidence.
//!
//! Exploration writes Campaign-owned structural artifacts next to product
//! evidence under its output directory: the durable `campaign-journal/`
//! (`LOCK`, `epoch-NNNN/seg-NNNNNNNN.cbor`) and the `frontier-trace.jsonl`
//! decision trace. Campaign receipts carry `ReceiptClaims::no_claims()`.
//! This pure classifier lets every product evidence admission point refuse
//! those artifacts, and any Campaign/Choregraph receipt schema, as VM, fault,
//! assertion, finding, replay, minimization, or release evidence.
//!
//! The Nickel dogfood receipt contract (`contracts/evidence/dogfood-receipt.ncl`)
//! mirrors these names; keep both in sync.

use std::fmt;

use crate::EvidenceError;

/// Directory holding the durable Campaign journal under an exploration output.
pub const CAMPAIGN_JOURNAL_DIRECTORY: &str = "campaign-journal";
/// Exclusive writer lock inside [`CAMPAIGN_JOURNAL_DIRECTORY`].
pub const CAMPAIGN_JOURNAL_LOCK_FILE: &str = "LOCK";
/// File name prefix of a journal segment (`seg-NNNNNNNN.cbor`).
pub const CAMPAIGN_JOURNAL_SEGMENT_PREFIX: &str = "seg-";
/// File name suffix of a journal segment.
pub const CAMPAIGN_JOURNAL_SEGMENT_SUFFIX: &str = ".cbor";
/// Frontier decision trace written for policy parity replay.
pub const FRONTIER_TRACE_FILE: &str = "frontier-trace.jsonl";

/// Schema bound into every Campaign journal segment header.
pub const CAMPAIGN_JOURNAL_SEGMENT_SCHEMA: &str = "chaoscontrol.campaign-journal-segment.v1";
/// Schema of each frontier decision trace line.
pub const FRONTIER_TRACE_SCHEMA: &str = "chaoscontrol.frontier-trace.v1";
/// Campaign projection schema label owned by the ChaosControl adapter.
pub const CAMPAIGN_PROJECTION_SCHEMA: &str = "chaoscontrol.campaign-projection.v1";
/// Campaign adapter identity label.
pub const CAMPAIGN_ADAPTER_SCHEMA: &str = "chaoscontrol.campaign-adapter.v1";
/// Choregraph exploration moment schema label.
pub const EXPLORATION_MOMENT_SCHEMA: &str = "chaoscontrol.exploration-moment.v1";
/// Namespace of Campaign-native receipt and profile schemas.
pub const CAMPAIGN_SCHEMA_NAMESPACE: &str = "campaign.";
/// Namespace of Choregraph-native receipt schemas.
pub const CHOREGRAPH_SCHEMA_NAMESPACE: &str = "choregraph.";

/// Stable prefix of every refusal message; admission callers and fixture
/// runners match on it to prove the refusal came from this boundary.
pub const STRUCTURAL_EVIDENCE_REFUSAL: &str = "campaign-evidence-boundary";

/// Exact ChaosControl-owned structural schemas and the artifact each names.
const STRUCTURAL_SCHEMAS: [(&str, StructuralArtifactKind); 5] = [
    (
        CAMPAIGN_JOURNAL_SEGMENT_SCHEMA,
        StructuralArtifactKind::CampaignJournalSegment,
    ),
    (FRONTIER_TRACE_SCHEMA, StructuralArtifactKind::FrontierTrace),
    (
        CAMPAIGN_PROJECTION_SCHEMA,
        StructuralArtifactKind::CampaignReceipt,
    ),
    (
        CAMPAIGN_ADAPTER_SCHEMA,
        StructuralArtifactKind::CampaignReceipt,
    ),
    (
        EXPLORATION_MOMENT_SCHEMA,
        StructuralArtifactKind::ChoregraphReceipt,
    ),
];

/// Reference fields of an accepted workload proof and the evidence class each claims.
pub const ACCEPTED_PROOF_REFERENCE_FIELDS: [(&str, ProductEvidenceClass); 6] = [
    ("evidence_dir", ProductEvidenceClass::Release),
    ("summary", ProductEvidenceClass::Replay),
    ("bug", ProductEvidenceClass::Finding),
    ("verdict", ProductEvidenceClass::Replay),
    ("snapshot", ProductEvidenceClass::Replay),
    ("receipt", ProductEvidenceClass::Release),
];

/// Product evidence classes that only ChaosControl runtime artifacts may satisfy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProductEvidenceClass {
    Vm,
    Fault,
    Assertion,
    Finding,
    Replay,
    Minimization,
    Release,
}

impl ProductEvidenceClass {
    pub const ALL: [Self; 7] = [
        Self::Vm,
        Self::Fault,
        Self::Assertion,
        Self::Finding,
        Self::Replay,
        Self::Minimization,
        Self::Release,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vm => "vm",
            Self::Fault => "fault",
            Self::Assertion => "assertion",
            Self::Finding => "finding",
            Self::Replay => "replay",
            Self::Minimization => "minimization",
            Self::Release => "release",
        }
    }
}

impl fmt::Display for ProductEvidenceClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Campaign/Choregraph structural artifact kinds recognised at admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructuralArtifactKind {
    /// The journal directory or any entry beneath it other than a lock or segment.
    CampaignJournal,
    CampaignJournalLock,
    CampaignJournalSegment,
    FrontierTrace,
    CampaignReceipt,
    ChoregraphReceipt,
}

impl StructuralArtifactKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CampaignJournal => "campaign journal",
            Self::CampaignJournalLock => "campaign journal lock",
            Self::CampaignJournalSegment => "campaign journal segment",
            Self::FrontierTrace => "frontier trace",
            Self::CampaignReceipt => "campaign receipt",
            Self::ChoregraphReceipt => "choregraph receipt",
        }
    }
}

impl fmt::Display for StructuralArtifactKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Typed refusal: a structural artifact was offered as product evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralEvidenceRefusal {
    pub class: ProductEvidenceClass,
    pub artifact: StructuralArtifactKind,
    pub field: String,
    pub reference: String,
}

impl fmt::Display for StructuralEvidenceRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{STRUCTURAL_EVIDENCE_REFUSAL}: {}: {} `{}` is a Campaign/Choregraph structural artifact and cannot satisfy {} evidence",
            self.field, self.artifact, self.reference, self.class
        )
    }
}

impl StructuralEvidenceRefusal {
    /// Rebind the refusal to the caller's precise field label.
    #[must_use]
    pub fn at(self, field: String) -> Self {
        Self { field, ..self }
    }
}

impl std::error::Error for StructuralEvidenceRefusal {}

impl From<StructuralEvidenceRefusal> for EvidenceError {
    fn from(refusal: StructuralEvidenceRefusal) -> Self {
        Self::new(refusal.to_string())
    }
}

/// Classify an artifact path; `None` means product-owned.
///
/// Components are split on `/` and `\` and compared ASCII case-insensitively
/// so case-folding filesystems cannot smuggle a structural artifact through.
pub fn classify_artifact_path(path: &str) -> Option<StructuralArtifactKind> {
    let components = path
        .split(['/', '\\'])
        .filter(|component| !component.is_empty() && *component != ".")
        .collect::<Vec<_>>();
    if components
        .iter()
        .any(|component| component.eq_ignore_ascii_case(FRONTIER_TRACE_FILE))
    {
        return Some(StructuralArtifactKind::FrontierTrace);
    }
    if components
        .iter()
        .any(|component| is_segment_file_name(component))
    {
        return Some(StructuralArtifactKind::CampaignJournalSegment);
    }
    let last = components.last()?;
    let journal = components
        .iter()
        .position(|component| component.eq_ignore_ascii_case(CAMPAIGN_JOURNAL_DIRECTORY))?;
    if journal + 1 < components.len() && last.eq_ignore_ascii_case(CAMPAIGN_JOURNAL_LOCK_FILE) {
        return Some(StructuralArtifactKind::CampaignJournalLock);
    }
    Some(StructuralArtifactKind::CampaignJournal)
}

/// Classify a declared schema identifier; `None` means product-owned.
pub fn classify_schema(schema: &str) -> Option<StructuralArtifactKind> {
    let schema = schema.trim().to_ascii_lowercase();
    if let Some((_, kind)) = STRUCTURAL_SCHEMAS
        .iter()
        .find(|(structural, _)| schema == *structural)
    {
        return Some(*kind);
    }
    if schema.starts_with(CAMPAIGN_SCHEMA_NAMESPACE) {
        return Some(StructuralArtifactKind::CampaignReceipt);
    }
    if schema.starts_with(CHOREGRAPH_SCHEMA_NAMESPACE) {
        return Some(StructuralArtifactKind::ChoregraphReceipt);
    }
    None
}

// r[impl chaoscontrol.campaign_policy.evidence]
/// Admit `reference` for `class` only when it is a product-owned artifact path.
pub fn admit_product_artifact(
    class: ProductEvidenceClass,
    field: &str,
    reference: &str,
) -> Result<(), StructuralEvidenceRefusal> {
    refuse(class, field, reference, classify_artifact_path(reference))
}

// r[impl chaoscontrol.campaign_policy.evidence]
/// Admit a declared evidence schema for `class` only when it is product-owned.
pub fn admit_product_schema(
    class: ProductEvidenceClass,
    field: &str,
    schema: &str,
) -> Result<(), StructuralEvidenceRefusal> {
    refuse(class, field, schema, classify_schema(schema))
}

/// Admit every reference of one accepted workload proof.
///
/// `reference` returns the value of a named [`ACCEPTED_PROOF_REFERENCE_FIELDS`]
/// entry, or `None` when the proof omits it.
pub fn admit_accepted_proof_references<'a>(
    reference: impl Fn(&str) -> Option<&'a str>,
) -> Result<(), StructuralEvidenceRefusal> {
    for (field, class) in ACCEPTED_PROOF_REFERENCE_FIELDS {
        if let Some(value) = reference(field) {
            admit_product_artifact(class, field, value)
                .map_err(|refusal| refusal.at(format!("proof.{field}")))?;
        }
    }
    Ok(())
}

fn refuse(
    class: ProductEvidenceClass,
    field: &str,
    reference: &str,
    artifact: Option<StructuralArtifactKind>,
) -> Result<(), StructuralEvidenceRefusal> {
    match artifact {
        None => Ok(()),
        Some(artifact) => Err(StructuralEvidenceRefusal {
            class,
            artifact,
            field: field.to_string(),
            reference: reference.to_string(),
        }),
    }
}

fn is_segment_file_name(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    lowered
        .strip_prefix(CAMPAIGN_JOURNAL_SEGMENT_PREFIX)
        .and_then(|rest| rest.strip_suffix(CAMPAIGN_JOURNAL_SEGMENT_SUFFIX))
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRODUCT_PATHS: [&str; 9] = [
        "dogfood-results/raft-20260506-095025/report.txt",
        "dogfood-results/raft-20260506-095025/bug_0.json",
        "dogfood-results/raft-20260506-095025/assertions.json",
        "dogfood-results/raft-20260506-095025/checkpoint.json",
        "dogfood-results/raft-20260506-095025/replay-verdict.json",
        "snapshots/0000000000000000000000000000000000000000000000000000000000000000.snapshot.bin",
        "/tmp/output/run-config.json",
        "docs/campaign-journal.md",
        "dogfood-results/run/frontier-trace.jsonl.md",
    ];

    const STRUCTURAL_PATHS: [(&str, StructuralArtifactKind); 9] = [
        (
            "dogfood-results/run/campaign-journal",
            StructuralArtifactKind::CampaignJournal,
        ),
        (
            "dogfood-results/run/campaign-journal/epoch-0000",
            StructuralArtifactKind::CampaignJournal,
        ),
        (
            "dogfood-results/run/campaign-journal/LOCK",
            StructuralArtifactKind::CampaignJournalLock,
        ),
        (
            "dogfood-results/run/campaign-journal/epoch-0000/seg-00000000.cbor",
            StructuralArtifactKind::CampaignJournalSegment,
        ),
        (
            "/tmp/output/Campaign-Journal/epoch-0001/SEG-00000042.CBOR",
            StructuralArtifactKind::CampaignJournalSegment,
        ),
        (
            "copied/seg-00000007.cbor",
            StructuralArtifactKind::CampaignJournalSegment,
        ),
        (
            "dogfood-results\\run\\campaign-journal\\LOCK",
            StructuralArtifactKind::CampaignJournalLock,
        ),
        (
            "dogfood-results/run/frontier-trace.jsonl",
            StructuralArtifactKind::FrontierTrace,
        ),
        (
            "./FRONTIER-TRACE.JSONL",
            StructuralArtifactKind::FrontierTrace,
        ),
    ];

    const PRODUCT_SCHEMAS: [&str; 5] = [
        "1",
        "chaoscontrol.campaign-profile.v1",
        "chaoscontrol.run-profile.v2",
        "chaoscontrol.sdk.local_report.v2",
        "campaign-profile",
    ];

    const STRUCTURAL_SCHEMA_CASES: [(&str, StructuralArtifactKind); 8] = [
        (
            CAMPAIGN_JOURNAL_SEGMENT_SCHEMA,
            StructuralArtifactKind::CampaignJournalSegment,
        ),
        (FRONTIER_TRACE_SCHEMA, StructuralArtifactKind::FrontierTrace),
        (
            CAMPAIGN_PROJECTION_SCHEMA,
            StructuralArtifactKind::CampaignReceipt,
        ),
        (
            CAMPAIGN_ADAPTER_SCHEMA,
            StructuralArtifactKind::CampaignReceipt,
        ),
        (
            EXPLORATION_MOMENT_SCHEMA,
            StructuralArtifactKind::ChoregraphReceipt,
        ),
        (
            "campaign.provenance.v1",
            StructuralArtifactKind::CampaignReceipt,
        ),
        (
            " Choregraph.Nickel-Source-Receipt.v1 ",
            StructuralArtifactKind::ChoregraphReceipt,
        ),
        (
            "CHAOSCONTROL.FRONTIER-TRACE.V1",
            StructuralArtifactKind::FrontierTrace,
        ),
    ];

    // r[verify chaoscontrol.campaign_policy.evidence.valid]
    #[test]
    fn product_artifacts_and_schemas_satisfy_every_evidence_class() {
        for class in ProductEvidenceClass::ALL {
            for path in PRODUCT_PATHS {
                assert_eq!(classify_artifact_path(path), None, "{path}");
                assert_eq!(admit_product_artifact(class, "field", path), Ok(()));
            }
            for schema in PRODUCT_SCHEMAS {
                assert_eq!(classify_schema(schema), None, "{schema}");
                assert_eq!(admit_product_schema(class, "field", schema), Ok(()));
            }
        }
    }

    // r[verify chaoscontrol.campaign_policy.evidence.invalid]
    #[test]
    fn structural_artifacts_are_refused_for_every_evidence_class() {
        for class in ProductEvidenceClass::ALL {
            for (path, kind) in STRUCTURAL_PATHS {
                let refusal = admit_product_artifact(class, "receipt.path", path)
                    .expect_err("structural artifact path must be refused");
                assert_eq!(refusal.class, class, "{path}");
                assert_eq!(refusal.artifact, kind, "{path}");
                assert_eq!(refusal.reference, path);
                let error = EvidenceError::from(refusal);
                assert!(error.message().starts_with(STRUCTURAL_EVIDENCE_REFUSAL));
                assert!(error.message().contains(class.as_str()));
            }
        }
    }

    // r[verify chaoscontrol.campaign_policy.evidence.invalid]
    #[test]
    fn structural_schemas_are_refused_for_every_evidence_class() {
        for class in ProductEvidenceClass::ALL {
            for (schema, kind) in STRUCTURAL_SCHEMA_CASES {
                let refusal = admit_product_schema(class, "receipt.schema_version", schema)
                    .expect_err("structural schema must be refused");
                assert_eq!(refusal.class, class, "{schema}");
                assert_eq!(refusal.artifact, kind, "{schema}");
            }
        }
    }

    #[test]
    fn segment_names_require_a_digit_sequence() {
        for name in [
            "seg-.cbor",
            "seg-0000000x.cbor",
            "seg-00000001.json",
            "seg-00000001",
            "segment-00000001.cbor",
        ] {
            assert_eq!(classify_artifact_path(name), None, "{name}");
        }
        assert_eq!(classify_artifact_path(""), None);
    }

    // r[verify chaoscontrol.campaign_policy.evidence.valid]
    // r[verify chaoscontrol.campaign_policy.evidence.invalid]
    #[test]
    fn accepted_proof_references_claim_their_declared_classes() {
        let product = |field: &str| match field {
            "receipt" => None,
            _ => Some("dogfood-results/raft/bug_0.json"),
        };
        assert_eq!(admit_accepted_proof_references(product), Ok(()));

        for (field, class) in ACCEPTED_PROOF_REFERENCE_FIELDS {
            let structural = |candidate: &str| {
                if candidate == field {
                    Some("dogfood-results/raft/frontier-trace.jsonl")
                } else {
                    Some("dogfood-results/raft/bug_0.json")
                }
            };
            let refusal = admit_accepted_proof_references(structural)
                .expect_err("structural proof reference must be refused");
            assert_eq!(refusal.class, class, "{field}");
            assert_eq!(refusal.field, format!("proof.{field}"));
            assert_eq!(refusal.artifact, StructuralArtifactKind::FrontierTrace);
        }
    }
}
