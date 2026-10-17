//! Pure, per-binary admission for the declared hostless core-MVP intersection.
//!
//! A corpus label or producer fixture name is never evidence of admission. Only
//! the exact bytes validated here may be passed to an execution shell.

use std::fmt;

use serde::{Deserialize, Serialize};
use wasmparser::{
    Encoding, ExternalKind, Operator, Parser, Payload, TypeRef, Validator, WasmFeatures,
};

use crate::DifferentialProfile;

/// Stable decision classes; parser diagnostics are explanatory, not classifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdmissionClass {
    Malformed,
    Unsupported,
    ProfileRejection,
    BoundExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DenialReason {
    InvalidProfile,
    InputBytes,
    Functions,
    Types,
    Exports,
    Globals,
    InvalidModule,
    Component,
    UnsupportedFeature,
    Wasi,
    Imports,
    Memories,
    Tables,
    MemoryGrowth,
    UnsupportedExport,
    MissingRunExport,
    RunExportKind,
    RunSignature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionDenial {
    pub class: AdmissionClass,
    pub reason: DenialReason,
    pub detail: String,
    pub offset: Option<usize>,
}

impl fmt::Display for AdmissionDenial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?}/{:?}: {}",
            self.class, self.reason, self.detail
        )
    }
}

impl std::error::Error for AdmissionDenial {}

/// Facts about the exact admitted bytes, not an authorization for other bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleAdmission {
    pub module_blake3: String,
    pub module_bytes: u64,
    pub functions: usize,
    pub types: usize,
    pub exports: usize,
    pub globals: usize,
    pub run_function_index: u32,
}

/// Malformed corpus inputs can be classified without authorizing execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", content = "facts", rename_all = "kebab-case")]
pub enum ModuleClassification {
    Admitted(ModuleAdmission),
    Denied(AdmissionDenial),
}

pub fn classify_module(profile: &DifferentialProfile, bytes: &[u8]) -> ModuleClassification {
    match admit_module(profile, bytes) {
        Ok(admission) => ModuleClassification::Admitted(admission),
        Err(denial) => ModuleClassification::Denied(denial),
    }
}

/// Fully validate a binary and resolve its actual exported function signature.
///
/// Count preflight precedes validator allocation. No runtime, filesystem, clock,
/// environment, producer label, or mutable state participates in the decision.
/// Unsupported and malformed inputs are never returned as admitted modules.
pub fn admit_module(
    profile: &DifferentialProfile,
    bytes: &[u8],
) -> Result<ModuleAdmission, AdmissionDenial> {
    crate::validate_profile(profile).map_err(|detail| AdmissionDenial {
        class: AdmissionClass::ProfileRejection,
        reason: DenialReason::InvalidProfile,
        detail,
        offset: None,
    })?;
    if bytes.len() as u64 > profile.bounds.maximum_input_bytes {
        return Err(denial(
            AdmissionClass::BoundExceeded,
            DenialReason::InputBytes,
            "module exceeds maximum_input_bytes",
        ));
    }

    let mut functions = 0;
    let mut types = 0;
    let mut exports = 0;
    let mut globals = 0;
    let mut run_function_index = None;
    let mut surface_denial = None;
    let mut has_memory_growth = false;

    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(malformed)? {
            Payload::Version {
                encoding: Encoding::Component,
                ..
            } => {
                return Err(denial(
                    AdmissionClass::Unsupported,
                    DenialReason::Component,
                    "component binaries are outside the core-module ABI",
                ));
            }
            Payload::TypeSection(reader) => {
                check_count(
                    reader.count() as usize,
                    profile.bounds.maximum_types,
                    DenialReason::Types,
                )?;
                for group in reader {
                    let group = group.map_err(malformed)?;
                    types = add_count(
                        types,
                        group.types().len(),
                        profile.bounds.maximum_types,
                        DenialReason::Types,
                    )?;
                }
            }
            Payload::FunctionSection(reader) => {
                functions = add_count(
                    functions,
                    reader.count() as usize,
                    profile.bounds.maximum_functions,
                    DenialReason::Functions,
                )?;
            }
            Payload::GlobalSection(reader) => {
                globals = add_count(
                    globals,
                    reader.count() as usize,
                    profile.bounds.maximum_globals,
                    DenialReason::Globals,
                )?;
            }
            Payload::CodeSectionStart { count, .. } => {
                check_count(
                    count as usize,
                    profile.bounds.maximum_functions,
                    DenialReason::Functions,
                )?;
            }
            Payload::ImportSection(reader) => {
                // This exact profile has no host authority, including WASI. Read
                // actual import records rather than searching arbitrary bytes.
                for import in reader {
                    let import = import.map_err(malformed)?;
                    if matches!(import.ty, TypeRef::Func(_)) {
                        functions = add_count(
                            functions,
                            1,
                            profile.bounds.maximum_functions,
                            DenialReason::Functions,
                        )?;
                    }
                    if matches!(import.ty, TypeRef::Global(_)) {
                        globals = add_count(
                            globals,
                            1,
                            profile.bounds.maximum_globals,
                            DenialReason::Globals,
                        )?;
                    }
                    let wasi = import.module == "wasi_snapshot_preview1"
                        || import.module == "wasi_unstable"
                        || import.module.starts_with("wasi:");
                    let already_wasi = matches!(
                        surface_denial.as_ref(),
                        Some(AdmissionDenial {
                            reason: DenialReason::Wasi,
                            ..
                        })
                    );
                    if (wasi && !already_wasi) || surface_denial.is_none() {
                        surface_denial = Some(denial(
                            AdmissionClass::Unsupported,
                            if wasi {
                                DenialReason::Wasi
                            } else {
                                DenialReason::Imports
                            },
                            "module requires imports outside the hostless ABI",
                        ));
                    }
                }
            }
            Payload::MemorySection(reader) if reader.count() != 0 => {
                surface_denial.get_or_insert_with(|| {
                    denial(
                        AdmissionClass::ProfileRejection,
                        DenialReason::Memories,
                        "linear memories exceed the profile's zero-memory ABI",
                    )
                });
            }
            Payload::TableSection(reader) if reader.count() != 0 => {
                surface_denial.get_or_insert_with(|| {
                    denial(
                        AdmissionClass::ProfileRejection,
                        DenialReason::Tables,
                        "tables exceed the profile's zero-table ABI",
                    )
                });
            }
            Payload::ExportSection(reader) => {
                exports = add_count(
                    exports,
                    reader.count() as usize,
                    profile.bounds.maximum_exports,
                    DenialReason::Exports,
                )?;
                for export in reader {
                    let export = export.map_err(malformed)?;
                    if export.name == profile.module_abi.required_export {
                        if export.kind != ExternalKind::Func {
                            surface_denial = Some(denial(
                                AdmissionClass::ProfileRejection,
                                DenialReason::RunExportKind,
                                "required run export is not a function",
                            ));
                        } else {
                            run_function_index = Some(export.index);
                        }
                    } else if export.kind != ExternalKind::Func {
                        // The profile declares no observation or ABI for exported
                        // globals, memories, tables, or proposal-specific items.
                        surface_denial.get_or_insert_with(|| {
                            denial(
                                AdmissionClass::Unsupported,
                                DenialReason::UnsupportedExport,
                                "only core function exports have an admitted observation model",
                            )
                        });
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let mut operators = body.get_operators_reader().map_err(malformed)?;
                while !operators.eof() {
                    if matches!(
                        operators.read().map_err(malformed)?,
                        Operator::MemoryGrow { .. }
                    ) {
                        has_memory_growth = true;
                    }
                }
            }
            _ => {}
        }
    }

    // WASM1 is MVP plus mutable globals, exactly the profile's admitted feature
    // intersection. Defaults must never enable later proposals implicitly.
    let validated = match Validator::new_with_features(WasmFeatures::WASM1).validate_all(bytes) {
        Ok(validated) => validated,
        Err(error) => {
            // A broader *validation-only* pass distinguishes a valid disallowed
            // proposal from a malformed module without parsing error-message text.
            // It cannot authorize execution under any circumstances.
            if Validator::new_with_features(WasmFeatures::all())
                .validate_all(bytes)
                .is_ok()
            {
                return Err(AdmissionDenial {
                    class: AdmissionClass::Unsupported,
                    reason: DenialReason::UnsupportedFeature,
                    detail: error.message().to_owned(),
                    offset: Some(error.offset()),
                });
            }
            return Err(malformed(error));
        }
    };
    if has_memory_growth {
        return Err(denial(
            AdmissionClass::ProfileRejection,
            DenialReason::MemoryGrowth,
            "memory.grow is not admitted, including unreachable or zero-growth instructions",
        ));
    }
    if let Some(denial) = surface_denial {
        return Err(denial);
    }
    let run_function_index = run_function_index.ok_or_else(|| {
        denial(
            AdmissionClass::ProfileRejection,
            DenialReason::MissingRunExport,
            "module does not export the required run function",
        )
    })?;
    // Full validation proved the export index and function kind. Reuse its
    // canonical type table instead of copying function/type index vectors.
    let type_id = validated.as_ref().core_function_at(run_function_index);
    let signature = validated[type_id].unwrap_func();
    if signature.params().len() != profile.module_abi.parameter_count
        || signature.results().len() != profile.module_abi.result_count
    {
        return Err(denial(
            AdmissionClass::ProfileRejection,
            DenialReason::RunSignature,
            "required run export must have signature () -> ()",
        ));
    }
    Ok(ModuleAdmission {
        module_blake3: blake3::hash(bytes).to_hex().to_string(),
        module_bytes: bytes.len() as u64,
        functions,
        types,
        exports,
        globals,
        run_function_index,
    })
}

fn denial(class: AdmissionClass, reason: DenialReason, detail: &str) -> AdmissionDenial {
    AdmissionDenial {
        class,
        reason,
        detail: detail.to_owned(),
        offset: None,
    }
}

fn malformed(error: wasmparser::BinaryReaderError) -> AdmissionDenial {
    AdmissionDenial {
        class: AdmissionClass::Malformed,
        reason: DenialReason::InvalidModule,
        detail: error.message().to_owned(),
        offset: Some(error.offset()),
    }
}

fn check_count(actual: usize, maximum: usize, reason: DenialReason) -> Result<(), AdmissionDenial> {
    if actual > maximum {
        Err(denial(
            AdmissionClass::BoundExceeded,
            reason,
            "module exceeds the named structural bound",
        ))
    } else {
        Ok(())
    }
}

fn add_count(
    current: usize,
    added: usize,
    maximum: usize,
    reason: DenialReason,
) -> Result<usize, AdmissionDenial> {
    let actual = current.checked_add(added).ok_or_else(|| {
        denial(
            AdmissionClass::BoundExceeded,
            reason,
            "structural count overflow",
        )
    })?;
    check_count(actual, maximum, reason)?;
    Ok(actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &[u8] = b"\0asm\x01\0\0\0";
    const EMPTY_TYPE: &[u8] = &[1, 4, 1, 0x60, 0, 0];
    const ONE_FUNCTION: &[u8] = &[3, 2, 1, 0];
    const RUN_EXPORT: &[u8] = &[7, 7, 1, 3, b'r', b'u', b'n', 0, 0];
    const EMPTY_BODY: &[u8] = &[10, 4, 1, 2, 0, 0x0b];

    fn profile() -> DifferentialProfile {
        crate::tests::profile()
    }

    fn module(sections: &[&[u8]]) -> Vec<u8> {
        std::iter::once(HEADER)
            .chain(sections.iter().copied())
            .flatten()
            .copied()
            .collect()
    }

    fn completion() -> Vec<u8> {
        module(&[EMPTY_TYPE, ONE_FUNCTION, RUN_EXPORT, EMPTY_BODY])
    }

    fn rejected(bytes: &[u8], class: AdmissionClass, reason: DenialReason) {
        let result = admit_module(&profile(), bytes).unwrap_err();
        assert_eq!((result.class, result.reason), (class, reason), "{result}");
        assert_eq!(
            classify_module(&profile(), bytes),
            ModuleClassification::Denied(result)
        );
    }

    #[test]
    fn actual_mvp_body_and_digest_are_admitted() {
        // Arithmetic and mutation of an unexported MVP global are not assumed
        // to be a no-op, and must pass real instruction/type validation.
        let bytes = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            &[6, 6, 1, 0x7f, 1, 0x41, 0, 0x0b],
            RUN_EXPORT,
            &[10, 11, 1, 9, 0, 0x41, 1, 0x41, 2, 0x6a, 0x24, 0, 0x0b],
        ]);
        let admitted = admit_module(&profile(), &bytes).unwrap();
        assert_eq!(
            admitted.module_blake3,
            blake3::hash(&bytes).to_hex().as_str()
        );
        assert_eq!(
            (
                admitted.functions,
                admitted.types,
                admitted.exports,
                admitted.run_function_index
            ),
            (1, 1, 1, 0)
        );
        assert_eq!(admitted.globals, 1);
        assert_eq!(admit_module(&profile(), &bytes), Ok(admitted));
    }

    #[test]
    fn custom_bytes_do_not_masquerade_as_instructions_or_imports() {
        let bytes = module(&[
            &[0, 6, 1, b'x', 0x40, 0, 0x0b, 0xfd],
            EMPTY_TYPE,
            ONE_FUNCTION,
            RUN_EXPORT,
            EMPTY_BODY,
        ]);
        assert_eq!(
            admit_module(&profile(), &bytes).unwrap().module_bytes,
            bytes.len() as u64
        );
    }

    #[test]
    fn components_and_actual_wasi_and_host_imports_are_unsupported() {
        rejected(
            b"\0asm\x0d\0\x01\0",
            AdmissionClass::Unsupported,
            DenialReason::Component,
        );
        let wasi = module(&[
            EMPTY_TYPE,
            &[
                2, 36, 1, 22, b'w', b'a', b's', b'i', b'_', b's', b'n', b'a', b'p', b's', b'h',
                b'o', b't', b'_', b'p', b'r', b'e', b'v', b'i', b'e', b'w', b'1', 9, b'p', b'r',
                b'o', b'c', b'_', b'e', b'x', b'i', b't', 0, 0,
            ],
            &[7, 7, 1, 3, b'r', b'u', b'n', 0, 0],
        ]);
        rejected(&wasi, AdmissionClass::Unsupported, DenialReason::Wasi);
        let host = module(&[EMPTY_TYPE, &[2, 7, 1, 1, b'h', 1, b'f', 0, 0], RUN_EXPORT]);
        rejected(&host, AdmissionClass::Unsupported, DenialReason::Imports);
    }

    #[test]
    fn post_mvp_instructions_and_types_are_not_admitted() {
        // i32.extend8_s (sign-extension proposal), with a valid stack.
        let sign_extension = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            RUN_EXPORT,
            &[10, 8, 1, 6, 0, 0x41, 0, 0xc0, 0x1a, 0x0b],
        ]);
        rejected(
            &sign_extension,
            AdmissionClass::Unsupported,
            DenialReason::UnsupportedFeature,
        );
        // Two return values: multi-value is not in the intersection.
        let multi_value = module(&[
            &[1, 6, 1, 0x60, 0, 2, 0x7f, 0x7f],
            ONE_FUNCTION,
            RUN_EXPORT,
            &[10, 8, 1, 6, 0, 0x41, 0, 0x41, 1, 0x0b],
        ]);
        rejected(
            &multi_value,
            AdmissionClass::Unsupported,
            DenialReason::UnsupportedFeature,
        );
        // SIMD syntax is decoded for classification, never admitted by WASM1.
        let simd = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            RUN_EXPORT,
            &[
                10, 23, 1, 21, 0, 0xfd, 0x0c, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x1a,
                0x0b,
            ],
        ]);
        rejected(
            &simd,
            AdmissionClass::Unsupported,
            DenialReason::UnsupportedFeature,
        );
    }

    #[test]
    fn memories_tables_and_zero_growth_are_not_admitted() {
        let memory = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            &[5, 3, 1, 0, 1],
            RUN_EXPORT,
            EMPTY_BODY,
        ]);
        rejected(
            &memory,
            AdmissionClass::ProfileRejection,
            DenialReason::Memories,
        );
        let table = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            &[4, 4, 1, 0x70, 0, 1],
            RUN_EXPORT,
            EMPTY_BODY,
        ]);
        rejected(
            &table,
            AdmissionClass::ProfileRejection,
            DenialReason::Tables,
        );
        let growth = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            &[5, 3, 1, 0, 0],
            RUN_EXPORT,
            &[10, 9, 1, 7, 0, 0x41, 0, 0x40, 0, 0x1a, 0x0b],
        ]);
        rejected(
            &growth,
            AdmissionClass::ProfileRejection,
            DenialReason::MemoryGrowth,
        );
    }

    #[test]
    fn export_kind_signature_and_state_boundary_are_checked() {
        rejected(
            &module(&[EMPTY_TYPE, ONE_FUNCTION, EMPTY_BODY]),
            AdmissionClass::ProfileRejection,
            DenialReason::MissingRunExport,
        );
        let parameter = module(&[
            &[1, 5, 1, 0x60, 1, 0x7f, 0],
            ONE_FUNCTION,
            RUN_EXPORT,
            EMPTY_BODY,
        ]);
        rejected(
            &parameter,
            AdmissionClass::ProfileRejection,
            DenialReason::RunSignature,
        );
        // The first function has the right signature; the exported second
        // function does not. Resolve the export's index and declared type.
        let second_function = module(&[
            &[1, 8, 2, 0x60, 0, 0, 0x60, 1, 0x7f, 0],
            &[3, 3, 2, 0, 1],
            &[7, 7, 1, 3, b'r', b'u', b'n', 0, 1],
            &[10, 7, 2, 2, 0, 0x0b, 2, 0, 0x0b],
        ]);
        rejected(
            &second_function,
            AdmissionClass::ProfileRejection,
            DenialReason::RunSignature,
        );
        let wrong_kind = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            &[6, 6, 1, 0x7f, 1, 0x41, 0, 0x0b],
            &[7, 7, 1, 3, b'r', b'u', b'n', 3, 0],
            EMPTY_BODY,
        ]);
        rejected(
            &wrong_kind,
            AdmissionClass::ProfileRejection,
            DenialReason::RunExportKind,
        );
        let global_export = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            &[6, 6, 1, 0x7f, 1, 0x41, 0, 0x0b],
            &[7, 11, 2, 3, b'r', b'u', b'n', 0, 0, 1, b'g', 3, 0],
            EMPTY_BODY,
        ]);
        rejected(
            &global_export,
            AdmissionClass::Unsupported,
            DenialReason::UnsupportedExport,
        );
    }

    #[test]
    fn validation_checks_body_stack_indices_trailing_bytes_and_truncation() {
        let invalid_stack = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            RUN_EXPORT,
            &[10, 5, 1, 3, 0, 0x6a, 0x0b],
        ]);
        rejected(
            &invalid_stack,
            AdmissionClass::Malformed,
            DenialReason::InvalidModule,
        );
        let invalid_proposal_stack = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            RUN_EXPORT,
            &[10, 5, 1, 3, 0, 0xc0, 0x0b],
        ]);
        rejected(
            &invalid_proposal_stack,
            AdmissionClass::Malformed,
            DenialReason::InvalidModule,
        );
        let invalid_index = module(&[
            EMPTY_TYPE,
            ONE_FUNCTION,
            &[7, 7, 1, 3, b'r', b'u', b'n', 0, 1],
            EMPTY_BODY,
        ]);
        rejected(
            &invalid_index,
            AdmissionClass::Malformed,
            DenialReason::InvalidModule,
        );
        let bytes = completion();
        for end in 0..bytes.len() {
            assert!(
                admit_module(&profile(), &bytes[..end]).is_err(),
                "prefix {end} was admitted"
            );
        }
        let mut trailing = bytes;
        trailing.push(0xff);
        rejected(
            &trailing,
            AdmissionClass::Malformed,
            DenialReason::InvalidModule,
        );
    }

    #[test]
    fn structural_bounds_apply_to_real_counts_and_accept_the_boundary() {
        let bytes = module(&[
            &[1, 7, 2, 0x60, 0, 0, 0x60, 0, 0],
            &[3, 3, 2, 0, 1],
            &[6, 11, 2, 0x7f, 0, 0x41, 0, 0x0b, 0x7f, 1, 0x41, 1, 0x0b],
            &[
                7, 14, 2, 3, b'r', b'u', b'n', 0, 1, 4, b'a', b'l', b's', b'o', 0, 0,
            ],
            &[10, 7, 2, 2, 0, 0x0b, 2, 0, 0x0b],
        ]);
        let mut bounded = profile();
        bounded.bounds.maximum_functions = 2;
        bounded.bounds.maximum_types = 2;
        bounded.bounds.maximum_exports = 2;
        bounded.bounds.maximum_globals = 2;
        let admitted = admit_module(&bounded, &bytes).unwrap();
        assert_eq!(
            (
                admitted.functions,
                admitted.types,
                admitted.exports,
                admitted.run_function_index
            ),
            (2, 2, 2, 1)
        );
        assert_eq!(admitted.globals, 2);
        for reason in [
            DenialReason::Functions,
            DenialReason::Types,
            DenialReason::Exports,
            DenialReason::Globals,
        ] {
            let mut limited = bounded.clone();
            match reason {
                DenialReason::Functions => limited.bounds.maximum_functions = 1,
                DenialReason::Types => limited.bounds.maximum_types = 1,
                DenialReason::Exports => limited.bounds.maximum_exports = 1,
                DenialReason::Globals => limited.bounds.maximum_globals = 1,
                _ => unreachable!(),
            }
            let result = admit_module(&limited, &bytes).unwrap_err();
            assert_eq!(
                (result.class, result.reason),
                (AdmissionClass::BoundExceeded, reason)
            );
        }
        bounded.bounds.maximum_input_bytes = bytes.len() as u64;
        assert_eq!(admit_module(&bounded, &bytes).unwrap(), admitted);
        bounded.bounds.maximum_input_bytes -= 1;
        let result = admit_module(&bounded, &bytes).unwrap_err();
        assert_eq!(
            (result.class, result.reason),
            (AdmissionClass::BoundExceeded, DenialReason::InputBytes)
        );
    }
}
