//! Pure admission for the `molten.wasm.abi.v1` full-ABI differential lane.
//!
//! The MVP profile never admits these modules, and this profile admits only
//! Molten's exact actor surface. Any other module is denied before an engine
//! runs, with the first violated rule.

#[cfg(feature = "full-abi-wasmtime")]
mod case;
pub mod host;
#[cfg(feature = "full-abi-wasmtime")]
pub mod wasmtime;
#[cfg(feature = "full-abi-wasmtime")]
pub use case::{AdmittedCase, CaseDenial, DiagnosticObservation, Terminal};

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use wasmparser::{
    Encoding, ExternalKind, MemoryType, Operator, Parser, Payload, TypeRef, ValType, Validator,
    WasmFeatures,
};

use crate::admission::{
    add_count, check_count, denial, malformed, AdmissionClass, AdmissionDenial, DenialReason,
};

pub const FULL_ABI_PROFILE_SCHEMA: &str = "chaoscontrol.spacewasm-full-abi-differential-profile.v1";
pub const MOLTEN_ABI: &str = "molten.wasm.abi.v1";
pub const HOSTCALL_MODULE: &str = "molten:hostcall";
pub const HOSTCALL_NAMES: [&str; 2] = ["assert", "send"];
/// Molten's host writes every reply at this guest offset.
pub const ABI_REPLY_OFFSET: u64 = 4096;
pub const ABI_REPLY_DESCRIPTOR: &str = "high-u32-offset-low-u32-length";
pub const WASM_PAGE_BYTES: u64 = 65_536;
pub const SCENARIO_CLASSES: [&str; 6] = [
    "abi",
    "cumulative-fuel",
    "hostcall",
    "memory",
    "stack",
    "trap",
];
pub const FULL_ABI_NON_CLAIMS: [&str; 9] = [
    "not-spacewasm-correctness",
    "not-spacewasm-wasmtime-equivalence",
    "not-memory-safety",
    "not-webassembly-conformance",
    "not-molten-runtime-correctness",
    "not-flight-qualification",
    "not-sandbox-effectiveness",
    "not-production-readiness",
    "not-release-eligibility",
];
const NOT_ADMITTED: &str = "not-admitted";
const WASMTIME_VERSION: &str = "wasmtime 49.0.5";
const WASMTIME_PATCH_LINE: &str = "patched-wasmtime-49.0.5";
const WASM_TOOLS_VERSION: &str = "wasm-tools 1.245.0";
const HOST_AUTHORITY: &str = "declared-hostcalls-only";
const MANTLE_CHANGE: &str = "materialize-molten-spacewasm-abi";
const PUBLICATION: &str = "release-authority-receipt-required";
const MAXIMUM_MEMORY_PAGES: u64 = 256;
const MAXIMUM_CASES: usize = 1024;
const MAXIMUM_INPUT_BYTES: u64 = 16_777_216;
const MAXIMUM_HOSTCALL_BYTES: u64 = 1_048_576;
const MAXIMUM_HOSTCALLS_PER_RUN: usize = 4096;
const MAXIMUM_FUEL: u64 = 1_000_000_000;
const MAXIMUM_RESUME_SEGMENTS: usize = 4096;
const MAXIMUM_STRUCTURAL_COUNT: usize = 65_536;
const MAXIMUM_GLOBALS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullAbiProfile {
    pub schema: String,
    pub profile_id: String,
    pub abi: AbiSurface,
    pub cohort: FullAbiCohort,
    pub bundle: FullAbiBundle,
    pub scenario_classes: Vec<String>,
    pub bounds: FullAbiBounds,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiSurface {
    pub name: String,
    pub hostcalls: Vec<AbiFunction>,
    pub exports: Vec<AbiExport>,
    pub memories: u32,
    pub tables: u32,
    pub start_function: String,
    pub memory_grow: String,
    pub maximum_memory_pages: u64,
    pub reply_offset: u64,
    pub reply_descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiFunction {
    pub module: String,
    pub name: String,
    pub parameters: Vec<String>,
    pub results: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiExport {
    pub name: String,
    pub kind: String,
    pub parameters: Vec<String>,
    pub results: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullAbiCohort {
    pub spacewasm_revision: String,
    pub wasmtime_version: String,
    pub wasmtime_patch_line: String,
    pub wasm_tools_version: String,
    pub host_authority: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullAbiBundle {
    pub mantle_change: String,
    pub mantle_revision: String,
    pub bundle_identity_blake3: String,
    pub publication: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullAbiBounds {
    pub maximum_cases: usize,
    pub maximum_input_bytes: u64,
    pub maximum_hostcall_bytes: u64,
    pub maximum_hostcalls_per_run: usize,
    pub wasmtime_fuel: u64,
    pub spacewasm_run_fuel: u64,
    pub maximum_resume_segments: usize,
    pub maximum_functions: usize,
    pub maximum_types: usize,
    pub maximum_globals: usize,
}

/// Facts about the exact admitted bytes, not an authorization for other bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullAbiAdmission {
    pub module_blake3: String,
    pub module_bytes: u64,
    /// Declared hostcalls the module imports, in name order.
    pub hostcalls: Vec<String>,
    pub memory_minimum_pages: u64,
    pub memory_maximum_pages: u64,
    pub run_function_index: u32,
}

/// The exact `molten.wasm.abi.v1` hostcalls.
pub fn molten_abi_hostcalls() -> Vec<AbiFunction> {
    HOSTCALL_NAMES
        .iter()
        .map(|name| AbiFunction {
            module: HOSTCALL_MODULE.to_owned(),
            name: (*name).to_owned(),
            parameters: value_types(&["i32", "i32"]),
            results: value_types(&["i64"]),
        })
        .collect()
}

/// The exact `molten.wasm.abi.v1` export set, in declaration order.
pub fn molten_abi_exports() -> Vec<AbiExport> {
    let export = |name: &str, kind: &str, parameters: &[&str], results: &[&str]| AbiExport {
        name: name.to_owned(),
        kind: kind.to_owned(),
        parameters: value_types(parameters),
        results: value_types(results),
    };
    vec![
        export("memory", "memory", &[], &[]),
        export("molten_alloc", "function", &["i32"], &["i32"]),
        export("molten_dealloc", "function", &["i32", "i32"], &[]),
        export("molten_run", "function", &["i32", "i32"], &["i64"]),
    ]
}

/// Rust-side check of the Nickel profile contract at the consumption boundary.
pub fn validate_full_abi_profile(profile: &FullAbiProfile) -> Result<(), String> {
    let abi = &profile.abi;
    let cohort = &profile.cohort;
    let bundle = &profile.bundle;
    let bounds = &profile.bounds;
    let within = |value: u64, maximum: u64| (1..=maximum).contains(&value);
    let reply_end = abi.reply_offset.checked_add(bounds.maximum_hostcall_bytes);
    let memory_bytes = abi.maximum_memory_pages.checked_mul(WASM_PAGE_BYTES);
    let checks = [
        (profile.schema == FULL_ABI_PROFILE_SCHEMA, "schema"),
        (is_identifier(&profile.profile_id), "profile_id"),
        (abi.name == MOLTEN_ABI, "abi.name"),
        (abi.hostcalls == molten_abi_hostcalls(), "abi.hostcalls"),
        (abi.exports == molten_abi_exports(), "abi.exports"),
        (
            abi.memories == 1 && abi.tables == 0,
            "abi memory and table counts",
        ),
        (
            abi.start_function == NOT_ADMITTED && abi.memory_grow == NOT_ADMITTED,
            "abi start function and memory.grow",
        ),
        (
            within(abi.maximum_memory_pages, MAXIMUM_MEMORY_PAGES),
            "abi.maximum_memory_pages",
        ),
        (
            abi.reply_offset == ABI_REPLY_OFFSET && abi.reply_descriptor == ABI_REPLY_DESCRIPTOR,
            "abi reply placement",
        ),
        (
            is_lower_hex(&cohort.spacewasm_revision, 40),
            "cohort.spacewasm_revision",
        ),
        (
            cohort.wasmtime_version == WASMTIME_VERSION
                && cohort.wasmtime_patch_line == WASMTIME_PATCH_LINE
                && cohort.wasm_tools_version == WASM_TOOLS_VERSION
                && cohort.host_authority == HOST_AUTHORITY,
            "cohort engines and host authority",
        ),
        (
            bundle.mantle_change == MANTLE_CHANGE
                && is_lower_hex(&bundle.mantle_revision, 40)
                && is_lower_hex(&bundle.bundle_identity_blake3, 64)
                && bundle.publication == PUBLICATION,
            "bundle",
        ),
        (
            profile.scenario_classes == SCENARIO_CLASSES,
            "scenario_classes",
        ),
        (
            within(bounds.maximum_cases as u64, MAXIMUM_CASES as u64)
                && within(bounds.maximum_input_bytes, MAXIMUM_INPUT_BYTES)
                && within(bounds.maximum_hostcall_bytes, MAXIMUM_HOSTCALL_BYTES)
                && within(
                    bounds.maximum_hostcalls_per_run as u64,
                    MAXIMUM_HOSTCALLS_PER_RUN as u64,
                )
                && within(bounds.wasmtime_fuel, MAXIMUM_FUEL)
                && within(bounds.spacewasm_run_fuel, MAXIMUM_FUEL)
                && within(
                    bounds.maximum_resume_segments as u64,
                    MAXIMUM_RESUME_SEGMENTS as u64,
                )
                && within(
                    bounds.maximum_functions as u64,
                    MAXIMUM_STRUCTURAL_COUNT as u64,
                )
                && within(bounds.maximum_types as u64, MAXIMUM_STRUCTURAL_COUNT as u64)
                && within(bounds.maximum_globals as u64, MAXIMUM_GLOBALS as u64),
            "bounds",
        ),
        (
            matches!((reply_end, memory_bytes), (Some(end), Some(bytes)) if end <= bytes),
            "reply bound beyond the smallest admitted memory",
        ),
        (
            FULL_ABI_NON_CLAIMS
                .iter()
                .all(|claim| profile.non_claims.iter().any(|value| value == claim)),
            "non_claims",
        ),
    ];
    match checks.iter().find(|(admitted, _)| !admitted) {
        Some((_, rule)) => Err(format!("full-ABI profile rejects {rule}")),
        None => Ok(()),
    }
}

/// Validate a binary and admit it only on the exact declared ABI surface.
///
/// Count preflight precedes validator allocation. No runtime, filesystem,
/// clock, environment or producer label participates in the decision.
pub fn admit_full_abi_module(
    profile: &FullAbiProfile,
    bytes: &[u8],
) -> Result<FullAbiAdmission, AdmissionDenial> {
    validate_full_abi_profile(profile).map_err(|detail| AdmissionDenial {
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
    let surface = read_surface(profile, bytes)?;
    // WASM1 is MVP plus mutable globals, the Mantle fixtures' feature profile.
    let validated = Validator::new_with_features(WasmFeatures::WASM1)
        .validate_all(bytes)
        .map_err(malformed)?;
    let signature = |index: u32| {
        let function = validated[validated.as_ref().core_function_at(index)].unwrap_func();
        (
            value_names(function.params()),
            value_names(function.results()),
        )
    };
    let hostcalls = admit_imports(profile, &surface.imports, signature)?;
    if surface.tables != 0 {
        return Err(rejection(DenialReason::Tables, "the ABI admits no table"));
    }
    let memory = admit_memory(profile, &surface.memories)?;
    if surface.start {
        return Err(rejection(
            DenialReason::StartFunction,
            "the ABI admits no start function",
        ));
    }
    if surface.memory_grow {
        return Err(rejection(
            DenialReason::MemoryGrowth,
            "memory.grow is not admitted, including unreachable or zero-growth instructions",
        ));
    }
    let run_function_index = admit_exports(profile, &surface.exports, signature)?;
    Ok(FullAbiAdmission {
        module_blake3: blake3::hash(bytes).to_hex().to_string(),
        module_bytes: bytes.len() as u64,
        hostcalls,
        memory_minimum_pages: memory.initial,
        memory_maximum_pages: memory.maximum.unwrap_or(memory.initial),
        run_function_index,
    })
}

#[derive(Default)]
struct Surface {
    imports: Vec<(String, String, TypeRef)>,
    memories: Vec<MemoryType>,
    tables: u32,
    exports: Vec<(String, ExternalKind, u32)>,
    start: bool,
    memory_grow: bool,
}

fn read_surface(profile: &FullAbiProfile, bytes: &[u8]) -> Result<Surface, AdmissionDenial> {
    let bounds = &profile.bounds;
    let mut surface = Surface::default();
    let mut functions = 0;
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
                    bounds.maximum_types,
                    DenialReason::Types,
                )?;
            }
            Payload::ImportSection(reader) => {
                for import in reader {
                    let import = import.map_err(malformed)?;
                    if matches!(import.ty, TypeRef::Func(_)) {
                        functions = add_count(
                            functions,
                            1,
                            bounds.maximum_functions,
                            DenialReason::Functions,
                        )?;
                    }
                    surface.imports.push((
                        import.module.to_owned(),
                        import.name.to_owned(),
                        import.ty,
                    ));
                }
            }
            Payload::FunctionSection(reader) => {
                functions = add_count(
                    functions,
                    reader.count() as usize,
                    bounds.maximum_functions,
                    DenialReason::Functions,
                )?;
            }
            Payload::TableSection(reader) => surface.tables += reader.count(),
            Payload::MemorySection(reader) => {
                for memory in reader {
                    surface.memories.push(memory.map_err(malformed)?);
                }
            }
            Payload::GlobalSection(reader) => {
                check_count(
                    reader.count() as usize,
                    bounds.maximum_globals,
                    DenialReason::Globals,
                )?;
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(malformed)?;
                    surface
                        .exports
                        .push((export.name.to_owned(), export.kind, export.index));
                }
            }
            Payload::StartSection { .. } => surface.start = true,
            Payload::CodeSectionEntry(body) => {
                let mut operators = body.get_operators_reader().map_err(malformed)?;
                while !operators.eof() {
                    if matches!(
                        operators.read().map_err(malformed)?,
                        Operator::MemoryGrow { .. }
                    ) {
                        surface.memory_grow = true;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(surface)
}

type Signature = (Vec<String>, Vec<String>);

/// Imports must be declared hostcalls with their exact signatures, at most once each.
fn admit_imports(
    profile: &FullAbiProfile,
    imports: &[(String, String, TypeRef)],
    signature: impl Fn(u32) -> Signature,
) -> Result<Vec<String>, AdmissionDenial> {
    let mut admitted = BTreeSet::new();
    // Any non-function import is denied before its signature is read, so the
    // import position is the function index of every admitted hostcall.
    for (function_index, (module, name, ty)) in (0_u32..).zip(imports) {
        let declared = profile
            .abi
            .hostcalls
            .iter()
            .find(|hostcall| &hostcall.module == module && &hostcall.name == name);
        let (TypeRef::Func(_), Some(declared)) = (ty, declared) else {
            return Err(rejection(
                DenialReason::Imports,
                "only declared molten:hostcall functions may be imported",
            ));
        };
        if signature(function_index) != (declared.parameters.clone(), declared.results.clone()) {
            return Err(rejection(
                DenialReason::HostcallSignature,
                "hostcall import signature differs from the ABI",
            ));
        }
        if !admitted.insert(name.clone()) {
            return Err(rejection(
                DenialReason::DuplicateHostcall,
                "a hostcall may be imported only once",
            ));
        }
    }
    Ok(admitted.into_iter().collect())
}

/// Exactly one non-shared 32-bit memory with a bounded maximum.
fn admit_memory(
    profile: &FullAbiProfile,
    memories: &[MemoryType],
) -> Result<MemoryType, AdmissionDenial> {
    let [memory] = memories else {
        return Err(rejection(
            DenialReason::Memories,
            "the ABI requires exactly one memory",
        ));
    };
    let bounded = memory.maximum.is_some_and(|maximum| {
        memory.initial >= 1
            && memory.initial <= maximum
            && maximum <= profile.abi.maximum_memory_pages
    });
    if !bounded || memory.memory64 || memory.shared || memory.page_size_log2.is_some() {
        return Err(rejection(
            DenialReason::Memories,
            "memory must be non-shared, 32-bit and bounded within the profile",
        ));
    }
    Ok(*memory)
}

/// Exports must be exactly the declared ABI set, with matching kinds and signatures.
fn admit_exports(
    profile: &FullAbiProfile,
    exports: &[(String, ExternalKind, u32)],
    signature: impl Fn(u32) -> Signature,
) -> Result<u32, AdmissionDenial> {
    let declared = &profile.abi.exports;
    let mismatch = || {
        rejection(
            DenialReason::AbiExports,
            "exports differ from the exact ABI set",
        )
    };
    if exports.len() != declared.len() {
        return Err(mismatch());
    }
    let mut run_function_index = None;
    for abi in declared {
        let (_, kind, index) = exports
            .iter()
            .find(|(name, _, _)| *name == abi.name)
            .ok_or_else(mismatch)?;
        match (abi.kind.as_str(), kind) {
            ("memory", ExternalKind::Memory) if *index == 0 => {}
            ("function", ExternalKind::Func) => {
                if signature(*index) != (abi.parameters.clone(), abi.results.clone()) {
                    return Err(rejection(
                        DenialReason::AbiExportSignature,
                        "ABI export signature differs",
                    ));
                }
                if abi.name == "molten_run" {
                    run_function_index = Some(*index);
                }
            }
            _ => return Err(mismatch()),
        }
    }
    run_function_index.ok_or_else(mismatch)
}

fn rejection(reason: DenialReason, detail: &str) -> AdmissionDenial {
    denial(AdmissionClass::ProfileRejection, reason, detail)
}

fn value_names(types: &[ValType]) -> Vec<String> {
    types
        .iter()
        .map(|value| {
            match value {
                ValType::I32 => "i32",
                ValType::I64 => "i64",
                ValType::F32 => "f32",
                ValType::F64 => "f64",
                _ => "unsupported",
            }
            .to_owned()
        })
        .collect()
}

fn value_types(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "full-abi-wasmtime")]
    mod wasmtime_tests;

    const EXAMPLE: &str = include_str!(
        "../../../contracts/evidence/examples/spacewasm-full-abi-differential-profile.json"
    );
    const I32: u8 = 0x7f;
    const I64: u8 = 0x7e;

    fn profile() -> FullAbiProfile {
        serde_json::from_str(EXAMPLE).expect("example full-ABI profile")
    }

    fn leb(mut value: u32, out: &mut Vec<u8>) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    fn name(text: &str, out: &mut Vec<u8>) {
        leb(text.len() as u32, out);
        out.extend_from_slice(text.as_bytes());
    }

    fn section(id: u8, entries: &[Vec<u8>], out: &mut Vec<u8>) {
        if entries.is_empty() {
            return;
        }
        let mut body = Vec::new();
        leb(entries.len() as u32, &mut body);
        entries.iter().for_each(|entry| body.extend(entry));
        out.push(id);
        leb(body.len() as u32, out);
        out.extend(body);
    }

    /// A core module shaped like Mantle's `molten-abi-*` fixtures.
    #[derive(Clone)]
    struct Shape {
        /// Hostcall imports: name and type index into `TYPES`.
        imports: Vec<(&'static str, &'static str, u32)>,
        /// Memory limits: initial pages and optional maximum.
        memories: Vec<(u32, Option<u32>)>,
        table: bool,
        start: bool,
        grow: bool,
        run_type: u32,
        exports: Vec<&'static str>,
    }

    /// Function types: 0 `(i32 i32) -> i64`, 1 `(i32) -> i32`,
    /// 2 `(i32 i32) -> ()`, 3 `(i64) -> i64`, 4 `() -> ()`.
    const TYPES: [(&[u8], &[u8]); 5] = [
        (&[I32, I32], &[I64]),
        (&[I32], &[I32]),
        (&[I32, I32], &[]),
        (&[I64], &[I64]),
        (&[], &[]),
    ];

    fn roundtrip() -> Shape {
        Shape {
            imports: Vec::new(),
            memories: vec![(1, Some(1))],
            table: false,
            start: false,
            grow: false,
            run_type: 0,
            exports: vec!["memory", "molten_alloc", "molten_dealloc", "molten_run"],
        }
    }

    fn hostcall() -> Shape {
        Shape {
            imports: vec![("molten:hostcall", "assert", 0)],
            ..roundtrip()
        }
    }

    fn encode(shape: &Shape) -> Vec<u8> {
        let run: &[u8] = if shape.grow {
            &[0x41, 0x00, 0x40, 0x00, 0x1a, 0x42, 0x00, 0x0b]
        } else {
            &[0x42, 0x00, 0x0b]
        };
        encode_with_run(shape, run)
    }

    fn encode_with_run(shape: &Shape, run: &[u8]) -> Vec<u8> {
        let mut out = b"\0asm\x01\0\0\0".to_vec();
        let types = TYPES
            .iter()
            .map(|(parameters, results)| {
                let mut entry = vec![0x60];
                leb(parameters.len() as u32, &mut entry);
                entry.extend_from_slice(parameters);
                leb(results.len() as u32, &mut entry);
                entry.extend_from_slice(results);
                entry
            })
            .collect::<Vec<_>>();
        section(1, &types, &mut out);
        let imports = shape
            .imports
            .iter()
            .map(|(module, field, ty)| {
                let mut entry = Vec::new();
                name(module, &mut entry);
                name(field, &mut entry);
                entry.push(0x00);
                leb(*ty, &mut entry);
                entry
            })
            .collect::<Vec<_>>();
        section(2, &imports, &mut out);
        // Defined functions: alloc, dealloc, run, then an optional start function.
        let mut functions = vec![vec![1], vec![2], vec![shape.run_type as u8]];
        if shape.start {
            functions.push(vec![4]);
        }
        section(3, &functions, &mut out);
        if shape.table {
            section(4, &[vec![0x70, 0x00, 0x00]], &mut out);
        }
        let memories = shape
            .memories
            .iter()
            .map(|(initial, maximum)| {
                let mut entry = vec![u8::from(maximum.is_some())];
                leb(*initial, &mut entry);
                if let Some(maximum) = maximum {
                    leb(*maximum, &mut entry);
                }
                entry
            })
            .collect::<Vec<_>>();
        section(5, &memories, &mut out);
        let base = shape.imports.len() as u32;
        let exports = shape
            .exports
            .iter()
            .map(|export| {
                let (kind, index) = match *export {
                    "memory" => (0x02, 0),
                    "molten_alloc" => (0x00, base),
                    "molten_dealloc" => (0x00, base + 1),
                    _ => (0x00, base + 2),
                };
                let mut entry = Vec::new();
                name(export, &mut entry);
                entry.push(kind);
                leb(index, &mut entry);
                entry
            })
            .collect::<Vec<_>>();
        section(7, &exports, &mut out);
        if shape.start {
            let mut start = vec![8];
            let mut index = Vec::new();
            leb(base + 3, &mut index);
            leb(index.len() as u32, &mut start);
            start.extend(index);
            out.extend(start);
        }
        let mut bodies = vec![
            vec![0x00, 0x41, 0x80, 0x08, 0x0b],
            vec![0x00, 0x0b],
            [&[0x00][..], run].concat(),
        ];
        if shape.start {
            bodies.push(vec![0x00, 0x0b]);
        }
        let bodies = bodies
            .into_iter()
            .map(|body| {
                let mut entry = Vec::new();
                leb(body.len() as u32, &mut entry);
                entry.extend(body);
                entry
            })
            .collect::<Vec<_>>();
        section(10, &bodies, &mut out);
        out
    }

    fn denied(shape: &Shape, reason: DenialReason) {
        let denial = admit_full_abi_module(&profile(), &encode(shape)).unwrap_err();
        assert_eq!(denial.reason, reason, "{denial}");
    }

    #[test]
    fn example_profile_is_the_exact_molten_abi() {
        let profile = profile();
        validate_full_abi_profile(&profile).expect("example profile");
        assert_eq!(profile.abi.hostcalls, molten_abi_hostcalls());
        assert_eq!(profile.abi.exports, molten_abi_exports());
    }

    #[test]
    fn widened_or_incoherent_profiles_are_rejected() {
        let mut clock = profile();
        clock.abi.hostcalls.push(AbiFunction {
            name: "clock".to_owned(),
            ..molten_abi_hostcalls()[0].clone()
        });
        let mut table = profile();
        table.abi.tables = 1;
        let mut overflow = profile();
        overflow.bounds.maximum_hostcall_bytes = WASM_PAGE_BYTES;
        let mut unpublished = profile();
        unpublished.bundle.publication = "local-build".to_owned();
        let mut overclaim = profile();
        overclaim
            .non_claims
            .retain(|claim| claim != "not-release-eligibility");
        for (profile, rule) in [
            (clock, "abi.hostcalls"),
            (table, "abi memory and table counts"),
            (overflow, "reply bound beyond the smallest admitted memory"),
            (unpublished, "bundle"),
            (overclaim, "non_claims"),
        ] {
            assert_eq!(
                validate_full_abi_profile(&profile),
                Err(format!("full-ABI profile rejects {rule}"))
            );
            let denial = admit_full_abi_module(&profile, &encode(&roundtrip())).unwrap_err();
            assert_eq!(denial.reason, DenialReason::InvalidProfile);
        }
    }

    #[test]
    fn fixture_shaped_abi_modules_are_admitted() {
        let bytes = encode(&roundtrip());
        let admitted = admit_full_abi_module(&profile(), &bytes).expect("roundtrip");
        assert_eq!(
            admitted.module_blake3,
            blake3::hash(&bytes).to_hex().as_str()
        );
        assert_eq!(
            (
                admitted.memory_minimum_pages,
                admitted.memory_maximum_pages,
                admitted.run_function_index
            ),
            (1, 1, 2)
        );
        assert!(admitted.hostcalls.is_empty());
        let both = Shape {
            imports: vec![
                ("molten:hostcall", "send", 0),
                ("molten:hostcall", "assert", 0),
            ],
            ..roundtrip()
        };
        let admitted = admit_full_abi_module(&profile(), &encode(&both)).expect("hostcalls");
        assert_eq!(admitted.hostcalls, ["assert", "send"]);
        assert_eq!(admitted.run_function_index, 4);
    }

    #[test]
    fn the_mvp_profile_never_admits_a_full_abi_module() {
        let mvp = crate::tests::profile();
        assert!(crate::admission::admit_module(&mvp, &encode(&hostcall())).is_err());
        assert!(crate::admission::admit_module(&mvp, &encode(&roundtrip())).is_err());
    }

    #[test]
    fn undeclared_or_misdeclared_hostcalls_are_denied() {
        let clock = Shape {
            imports: vec![("molten:hostcall", "clock", 0)],
            ..roundtrip()
        };
        denied(&clock, DenialReason::Imports);
        let wasi = Shape {
            imports: vec![("wasi_snapshot_preview1", "fd_write", 0)],
            ..roundtrip()
        };
        denied(&wasi, DenialReason::Imports);
        let wide = Shape {
            imports: vec![("molten:hostcall", "send", 3)],
            ..roundtrip()
        };
        denied(&wide, DenialReason::HostcallSignature);
        let twice = Shape {
            imports: vec![
                ("molten:hostcall", "assert", 0),
                ("molten:hostcall", "assert", 0),
            ],
            ..roundtrip()
        };
        denied(&twice, DenialReason::DuplicateHostcall);
    }

    #[test]
    fn memory_table_start_and_growth_outside_the_abi_are_denied() {
        // Mantle's molten-abi-memory-limit fixture: two pages under a one-page budget.
        let two_pages = Shape {
            memories: vec![(2, Some(2))],
            ..roundtrip()
        };
        denied(&two_pages, DenialReason::Memories);
        let unbounded = Shape {
            memories: vec![(1, None)],
            ..roundtrip()
        };
        denied(&unbounded, DenialReason::Memories);
        let none = Shape {
            memories: Vec::new(),
            exports: vec!["molten_alloc", "molten_dealloc", "molten_run"],
            ..roundtrip()
        };
        denied(&none, DenialReason::Memories);
        let table = Shape {
            table: true,
            ..roundtrip()
        };
        denied(&table, DenialReason::Tables);
        let start = Shape {
            start: true,
            ..roundtrip()
        };
        denied(&start, DenialReason::StartFunction);
        let grow = Shape {
            grow: true,
            ..roundtrip()
        };
        denied(&grow, DenialReason::MemoryGrowth);
    }

    #[test]
    fn exports_must_be_the_exact_abi_set() {
        let missing = Shape {
            exports: vec!["memory", "molten_alloc", "molten_run"],
            ..roundtrip()
        };
        denied(&missing, DenialReason::AbiExports);
        let wrong_run = Shape {
            run_type: 3,
            ..roundtrip()
        };
        denied(&wrong_run, DenialReason::AbiExportSignature);
    }

    #[test]
    fn input_bound_precedes_parsing() {
        let mut limited = profile();
        let bytes = encode(&roundtrip());
        limited.bounds.maximum_input_bytes = bytes.len() as u64 - 1;
        let denial = admit_full_abi_module(&limited, &bytes).unwrap_err();
        assert_eq!(
            (denial.class, denial.reason),
            (AdmissionClass::BoundExceeded, DenialReason::InputBytes)
        );
    }
}
