use crate::profile_projection::{
    blake3_identity, bound_file, canonical_pretty_json, ProjectionReceipt, EVALUATOR_IDENTITY,
    MAX_PROFILE_BYTES, NON_CLAIMS, RECEIPT_SCHEMA,
};
use crate::profile_projection_spec::{find_spec, validate_receipt_against_spec, ProjectionSpec};
use crate::{EvidenceError, EvidenceResult};

pub(crate) fn verify_profile_projection(
    root: &std::path::Path,
    projection: &std::path::Path,
    receipt: &std::path::Path,
    expected_profile_id: &str,
) -> EvidenceResult<String> {
    let spec = find_spec(expected_profile_id)?;
    verify_profile_projection_for_spec(root, projection, receipt, spec)
}

pub(crate) fn verify_profile_projection_for_spec(
    root: &std::path::Path,
    projection: &std::path::Path,
    receipt: &std::path::Path,
    spec: &ProjectionSpec,
) -> EvidenceResult<String> {
    validate_requested_paths(projection, receipt, spec)?;
    let receipt_path = rooted(root, receipt);
    let receipt_text =
        crate::bounded_file::read_bounded_regular_file(&receipt_path, MAX_PROFILE_BYTES)?;
    crate::json_preflight::preflight_json(
        &receipt_text,
        crate::json_preflight::QUALITY_REPORT_LIMITS,
    )?;
    let receipt: ProjectionReceipt = serde_json::from_str(&receipt_text).map_err(|error| {
        EvidenceError::new(format!("invalid profile projection receipt: {error}"))
    })?;
    validate_receipt_header(&receipt, spec)?;
    validate_receipt_against_spec(&receipt, spec)?;
    validate_bound_source(root, &receipt.source)?;
    validate_bound_source(root, &receipt.contract)?;
    for import in &receipt.imports {
        validate_bound_source(root, import)?;
    }
    let projection_path = rooted(root, projection);
    let projection_text =
        crate::bounded_file::read_bounded_regular_file(&projection_path, MAX_PROFILE_BYTES)?;
    let canonical = canonical_pretty_json(projection_text.as_bytes())?;
    if blake3_identity(&canonical) != receipt.projection.identity {
        return Err(EvidenceError::new("profile projection identity mismatch"));
    }
    String::from_utf8(canonical)
        .map_err(|error| EvidenceError::new(format!("profile projection is not UTF-8: {error}")))
}

fn validate_requested_paths(
    projection: &std::path::Path,
    receipt: &std::path::Path,
    spec: &ProjectionSpec,
) -> EvidenceResult<()> {
    if projection != std::path::Path::new(spec.projection.path)
        || receipt != std::path::Path::new(spec.receipt)
    {
        return Err(EvidenceError::new(
            "profile projection or receipt path differs from the trusted specification",
        ));
    }
    Ok(())
}

fn validate_receipt_header(
    receipt: &ProjectionReceipt,
    spec: &ProjectionSpec,
) -> EvidenceResult<()> {
    let expected_non_claims = NON_CLAIMS
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    if receipt.schema != RECEIPT_SCHEMA
        || receipt.profile_id != spec.profile_id
        || receipt.evaluator.name != EVALUATOR_IDENTITY
        || receipt.evaluator.identity != blake3_identity(EVALUATOR_IDENTITY.as_bytes())
        || receipt.non_claims != expected_non_claims
    {
        return Err(EvidenceError::new(
            "profile projection receipt header or non-claims mismatch",
        ));
    }
    Ok(())
}

fn validate_bound_source(
    root: &std::path::Path,
    expected: &crate::profile_projection::BoundArtifact,
) -> EvidenceResult<()> {
    let actual = bound_file(root, &expected.path)?;
    if actual != *expected {
        return Err(EvidenceError::new(format!(
            "profile projection source identity mismatch: {}",
            expected.path
        )));
    }
    Ok(())
}

fn rooted(root: &std::path::Path, path: &std::path::Path) -> std::path::PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    root.join(path)
}
