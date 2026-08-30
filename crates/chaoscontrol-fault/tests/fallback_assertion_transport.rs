use chaoscontrol_fault::oracle::{FallbackOracleError, PropertyOracle, Verdict};
use chaoscontrol_protocol::admission::{token_for_descriptors, CatalogBuilder};
use chaoscontrol_protocol::fallback::{
    FallbackProcessIdentity, FallbackRecord, FallbackRecordType, FallbackSink,
    FALLBACK_RECORD_SCHEMA_VERSION,
};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};

const SINK_RECORD_LIMIT: usize = 2;
const ASSERTION_SEQUENCE: u64 = 0;
const LIFECYCLE_SEQUENCE: u64 = 1;
const OVERFLOW_SEQUENCE: u64 = 2;
const BASE_ALIAS: u32 = 41;

fn process() -> FallbackProcessIdentity {
    FallbackProcessIdentity {
        guest: "guest-a".to_string(),
        process: "wal-worker".to_string(),
    }
}

fn fallback_record(
    sequence: u64,
    logical_key: &str,
    record_type: FallbackRecordType,
    condition: Option<bool>,
) -> FallbackRecord {
    FallbackRecord {
        schema_version: FALLBACK_RECORD_SCHEMA_VERSION,
        sequence,
        process: process(),
        namespace: "org.example.store".to_string(),
        logical_key: logical_key.to_string(),
        record_type,
        condition,
        message: format!("fallback event {logical_key}"),
        details: serde_json::json!({"phase": "checkpoint"}),
    }
}

fn base_catalog() -> chaoscontrol_protocol::admission::AcceptedCatalog {
    let descriptor = AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.example.sdk".to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: "sdk-ready".to_string(),
        },
        compatibility_id: Some(BASE_ALIAS),
        kind: AssertionKind::Always,
        message: "SDK assertion remains admitted".to_string(),
        source_file: "src/main.rs".to_string(),
        source_line: 1,
        source_column: 1,
        guest: "guest-a".to_string(),
        category: "invariant".to_string(),
    };
    let descriptors = vec![descriptor];
    let token = token_for_descriptors(&descriptors).expect("base token");
    let mut builder = CatalogBuilder::begin(descriptors.len()).expect("base builder");
    for descriptor in descriptors {
        builder.insert(descriptor).expect("base descriptor");
    }
    builder.complete(token).expect("base catalog")
}

fn evidence_with_overflow() -> chaoscontrol_protocol::fallback::FallbackSinkEvidence {
    let mut sink = FallbackSink::new(SINK_RECORD_LIMIT).expect("sink");
    let assertion = fallback_record(
        ASSERTION_SEQUENCE,
        "wal-reset-safe",
        FallbackRecordType::Always,
        Some(false),
    );
    sink.admit_line(&serde_json::to_string(&assertion).expect("assertion line"))
        .expect("assertion admitted");
    let lifecycle = fallback_record(
        LIFECYCLE_SEQUENCE,
        "checkpoint-entered",
        FallbackRecordType::Lifecycle,
        None,
    );
    sink.admit_line(&serde_json::to_string(&lifecycle).expect("lifecycle line"))
        .expect("lifecycle admitted");
    let overflow = fallback_record(
        OVERFLOW_SEQUENCE,
        "overflowed",
        FallbackRecordType::Lifecycle,
        None,
    );
    sink.admit_line(&serde_json::to_string(&overflow).expect("overflow line"))
        .expect("overflow recorded");
    sink.evidence().expect("sink evidence")
}

#[test]
fn ingests_assertion_lifecycle_and_overflow_in_sink_order() {
    let evidence = evidence_with_overflow();
    let mut oracle = PropertyOracle::new();
    oracle
        .activate_catalog_with_fallback(&base_catalog(), &evidence)
        .expect("fallback catalog activates");
    oracle.begin_run();
    oracle
        .record_fallback_sink(&evidence)
        .expect("fallback sink records");
    oracle.end_run();

    let report = oracle.report();
    chaoscontrol_fault::oracle_validation::validate_oracle_report_claim(&report)
        .expect("fallback report remains admissible");
    let fallback = report
        .structured_assertions
        .values()
        .find(|record| record.fallback_scope.is_some())
        .expect("fallback assertion record");
    assert_eq!(fallback.verdict(), Verdict::Failed);
    assert_eq!(
        fallback
            .fallback_scope
            .as_ref()
            .expect("process scope")
            .process,
        process()
    );
    assert_eq!(report.events.len(), SINK_RECORD_LIMIT);
    assert_eq!(report.events[0].name, "checkpoint-entered");
    assert_eq!(report.events[1].name, "fallback_assertion_sink_overflow");
}

#[test]
fn reordered_evidence_is_rejected_without_partial_oracle_mutation() {
    let evidence = evidence_with_overflow();
    let mut oracle = PropertyOracle::new();
    oracle
        .activate_catalog_with_fallback(&base_catalog(), &evidence)
        .expect("fallback catalog activates");
    oracle.begin_run();

    let mut reordered = evidence.clone();
    reordered.records[0].sequence = LIFECYCLE_SEQUENCE;
    let before = oracle.finalized_report_projection();
    assert!(matches!(
        oracle
            .record_fallback_sink(&reordered)
            .expect_err("reordered evidence"),
        FallbackOracleError::Sink(_)
    ));
    assert_eq!(oracle.finalized_report_projection(), before);
}
