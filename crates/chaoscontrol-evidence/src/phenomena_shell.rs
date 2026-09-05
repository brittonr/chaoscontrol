use std::fs::{self};
use std::io::Write;

pub const MAX_PHENOMENA_HISTORY_BYTES: u64 = 4 * 1_024 * 1_024;
const REGISTER_KEY: &str = "register";
const ROUND_ARTIFACT_DOMAIN: &[u8] = b"chaoscontrol.phenomena.round-artifact.v1\0";

pub fn read_phenomena_history_path(
    path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<::chaoscontrol_smr::phenomena::PhenomenaHistory> {
    let path = path.as_ref();
    let bytes = crate::bounded_file::read_bounded_regular_bytes(path, MAX_PHENOMENA_HISTORY_BYTES)?;
    let history = serde_json::from_slice::<::chaoscontrol_smr::phenomena::PhenomenaHistory>(&bytes)
        .map_err(|error| {
            crate::EvidenceError::new(format!(
                "{}: history JSON is not a closed typed artifact: {error}",
                path.display()
            ))
        })?;
    ::chaoscontrol_smr::phenomena::validate_history(&history)
        .map_err(|error| crate::EvidenceError::new(format!("{}: {error}", path.display())))?;
    Ok(history)
}

pub fn adapt_consistency_history(
    source: &crate::consistency_checker::OperationHistory,
) -> crate::EvidenceResult<::chaoscontrol_smr::phenomena::PhenomenaHistory> {
    crate::consistency_checker::validate_history(source)?;
    let mut source_operations = source.operations.clone();
    source_operations.sort_by_key(|operation| {
        (
            operation.completed_at,
            operation.invoked_at,
            operation.operation_id.clone(),
        )
    });
    let mut operations = Vec::with_capacity(source_operations.len());
    let mut prior_writes: Vec<(&crate::consistency_checker::HistoryOperation, u64)> = Vec::new();
    for (index, source_operation) in source_operations.iter().enumerate() {
        let sequence = u64::try_from(index)
            .map_err(|_| crate::EvidenceError::new("round history sequence exceeds u64"))?;
        let status = match &source_operation.completion {
            crate::consistency_checker::OperationCompletion::Ok { .. } => {
                ::chaoscontrol_smr::phenomena::OperationStatus::Committed
            }
            crate::consistency_checker::OperationCompletion::Failed { .. } => {
                ::chaoscontrol_smr::phenomena::OperationStatus::Aborted
            }
        };
        let mut dependencies = Vec::new();
        let kind = match (&source_operation.invocation, &source_operation.completion) {
            (crate::consistency_checker::OperationInvocation::Write { value }, _) => {
                if let Some((previous, _)) = prior_writes
                    .iter()
                    .rev()
                    .find(|(previous, _)| previous.completed_at <= source_operation.invoked_at)
                {
                    dependencies.push(::chaoscontrol_smr::phenomena::Dependency {
                        predecessor: previous.operation_id.clone(),
                        kind: ::chaoscontrol_smr::phenomena::DependencyKind::WriteWrite,
                    });
                }
                let version = sequence.checked_add(1).ok_or_else(|| {
                    crate::EvidenceError::new("round history version exceeds u64")
                })?;
                prior_writes.push((source_operation, version));
                ::chaoscontrol_smr::phenomena::OperationKind::Write {
                    key: REGISTER_KEY.to_string(),
                    version,
                    value: value.to_string(),
                }
            }
            (
                crate::consistency_checker::OperationInvocation::Read,
                crate::consistency_checker::OperationCompletion::Ok { value },
            ) => {
                let observation = match value {
                    None => ::chaoscontrol_smr::phenomena::ReadObservation::Initial,
                    Some(value) => match prior_writes.iter().rev().find(|(write, _)| {
                        matches!(
                            write.invocation,
                            crate::consistency_checker::OperationInvocation::Write {
                                value: written
                            } if written == *value
                        )
                    }) {
                        Some((write, version)) => {
                            dependencies.push(::chaoscontrol_smr::phenomena::Dependency {
                                predecessor: write.operation_id.clone(),
                                kind: ::chaoscontrol_smr::phenomena::DependencyKind::WriteRead,
                            });
                            ::chaoscontrol_smr::phenomena::ReadObservation::Write {
                                operation_id: write.operation_id.clone(),
                                version: *version,
                                value: value.to_string(),
                            }
                        }
                        None => ::chaoscontrol_smr::phenomena::ReadObservation::Unattributed {
                            value: value.to_string(),
                        },
                    },
                };
                ::chaoscontrol_smr::phenomena::OperationKind::Read {
                    key: REGISTER_KEY.to_string(),
                    observation,
                }
            }
            (
                crate::consistency_checker::OperationInvocation::Read,
                crate::consistency_checker::OperationCompletion::Failed { .. },
            ) => ::chaoscontrol_smr::phenomena::OperationKind::Read {
                key: REGISTER_KEY.to_string(),
                observation: ::chaoscontrol_smr::phenomena::ReadObservation::Initial,
            },
        };
        operations.push(::chaoscontrol_smr::phenomena::HistoryOperation {
            operation_id: source_operation.operation_id.clone(),
            process: source_operation.process.clone(),
            sequence,
            status,
            kind,
            dependencies,
        });
    }
    let bytes = serde_json::to_vec(source)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(ROUND_ARTIFACT_DOMAIN);
    let length = u64::try_from(bytes.len())
        .map_err(|_| crate::EvidenceError::new("round history byte length exceeds u64"))?;
    hasher.update(&length.to_le_bytes());
    hasher.update(&bytes);
    let source_blake3 = format!("blake3:{}", hasher.finalize().to_hex());
    ::chaoscontrol_smr::phenomena::bind_history(
        &source.workload,
        source_blake3,
        operations,
        Vec::new(),
    )
    .map_err(|error| crate::EvidenceError::new(error.to_string()))
}

pub fn check_consistency_phenomena_path(
    path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<::chaoscontrol_smr::phenomena::PhenomenaReport> {
    let source = crate::consistency_checker::read_history_path(path)?;
    let history = adapt_consistency_history(&source)?;
    let report = ::chaoscontrol_smr::phenomena::check_history(&history)
        .map_err(|error| crate::EvidenceError::new(error.to_string()))?;
    ::chaoscontrol_smr::phenomena::validate_report_for_history(&report, &history)
        .map_err(|error| crate::EvidenceError::new(error.to_string()))?;
    Ok(report)
}

pub fn validate_phenomena_history_path(
    path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<String> {
    let history = read_phenomena_history_path(path)?;
    Ok(format!(
        "history={} workload={} operations={} gaps={} source={}",
        history.history_id,
        history.workload,
        history.operations.len(),
        history.gaps.len(),
        history.source_blake3
    ))
}

pub fn check_phenomena_history_path(
    path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<::chaoscontrol_smr::phenomena::PhenomenaReport> {
    let history = read_phenomena_history_path(path)?;
    let report = ::chaoscontrol_smr::phenomena::check_history(&history)
        .map_err(|error| crate::EvidenceError::new(error.to_string()))?;
    ::chaoscontrol_smr::phenomena::validate_report_for_history(&report, &history)
        .map_err(|error| crate::EvidenceError::new(error.to_string()))?;
    Ok(report)
}

pub fn write_phenomena_report_path(
    history_path: impl AsRef<std::path::Path>,
    report_path: impl AsRef<std::path::Path>,
) -> crate::EvidenceResult<()> {
    let report = check_phenomena_history_path(history_path)?;
    let mut bytes = serde_json::to_vec_pretty(&report)?;
    bytes.push(b'\n');
    let report_path = report_path.as_ref();
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report_path)
        .map_err(|error| {
            crate::EvidenceError::new(format!("{}: {error}", report_path.display()))
        })?;
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let cleanup = fs::remove_file(report_path);
        return Err(crate::EvidenceError::new(match cleanup {
            Ok(()) => format!("{}: {error}", report_path.display()),
            Err(cleanup_error) => format!(
                "{}: {error}; failed to remove partial report: {cleanup_error}",
                report_path.display()
            ),
        }));
    }
    Ok(())
}
