//! Bounded symbolized coverage mapping and reporting.
//!
//! Build-owned symbol catalogs, runtime module loads, and coverage observations
//! are joined here. This module performs no file, clock, process, network, or
//! environment access; callers supply admitted facts.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Current symbol-catalog schema.
pub const SYMBOL_CATALOG_SCHEMA: &str = "chaoscontrol.coverage-symbol-catalog.v1";
/// Current symbolized-report schema.
pub const SYMBOLIZED_REPORT_SCHEMA: &str = "chaoscontrol.symbolized-coverage-report.v1";
/// Current symbolized-receipt schema.
pub const SYMBOLIZED_RECEIPT_SCHEMA: &str = "chaoscontrol.symbolized-coverage-receipt.v1";

pub const MAX_SYMBOL_MODULES: usize = 256;
pub const MAX_SYMBOL_LOCATIONS_PER_MODULE: usize = 1_000_000;
pub const MAX_SYMBOL_OBSERVATIONS: usize = 1_000_000;
pub const MAX_SYMBOL_REPORT_ROWS: usize = 100_000;
pub const MAX_SOURCE_PATH_BYTES: usize = 1_024;
pub const MAX_FUNCTION_NAME_BYTES: usize = 512;
pub const MAX_INSTRUMENTED_IDS_PER_LOCATION: usize = 4_096;

/// Instrumentation family that produced the identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentationKind {
    SanCov,
    Kcov,
    Other,
}

/// Exact content identity and reviewed path of one instrumented module.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleIdentity {
    /// BLAKE3 identity of the module bytes.
    pub content_id: [u8; 32],
    /// Reviewed repository-relative module path.
    pub path: String,
}

/// One source location and the instrumentation identifiers that reach it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolLocation {
    pub function: String,
    pub source_file: String,
    pub source_line: u32,
    pub source_column: u32,
    pub instrumented_ids: Vec<u64>,
}

/// Build-owned catalog for one module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolCatalog {
    pub schema: String,
    pub module: ModuleIdentity,
    pub language: String,
    pub instrumentation: InstrumentationKind,
    /// True only when the producer could map every instrumented location.
    pub complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete_reason: Option<String>,
    pub locations: Vec<SymbolLocation>,
}

/// One runtime module load fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadedModule {
    pub module: ModuleIdentity,
    pub load_ordinal: u32,
}

/// One coverage identifier observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageObservation {
    pub module_id: [u8; 32],
    pub identifier: u64,
    pub hits: u64,
}

/// Stable location key used by the pure mapping.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LocationKey {
    source_file: String,
    source_line: u32,
    source_column: u32,
    function: String,
}

impl LocationKey {
    fn of(location: &SymbolLocation) -> Self {
        Self {
            source_file: location.source_file.clone(),
            source_line: location.source_line,
            source_column: location.source_column,
            function: location.function.clone(),
        }
    }
}

/// Named finite limits for mapping and reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolCoverageLimits {
    pub max_modules: usize,
    pub max_locations_per_module: usize,
    pub max_observations: usize,
    pub max_report_rows: usize,
}

impl Default for SymbolCoverageLimits {
    fn default() -> Self {
        Self {
            max_modules: MAX_SYMBOL_MODULES,
            max_locations_per_module: MAX_SYMBOL_LOCATIONS_PER_MODULE,
            max_observations: MAX_SYMBOL_OBSERVATIONS,
            max_report_rows: MAX_SYMBOL_REPORT_ROWS,
        }
    }
}

impl SymbolCoverageLimits {
    pub fn validate(&self) -> Result<(), SymbolCoverageError> {
        if self.max_modules == 0
            || self.max_modules > MAX_SYMBOL_MODULES
            || self.max_locations_per_module == 0
            || self.max_locations_per_module > MAX_SYMBOL_LOCATIONS_PER_MODULE
            || self.max_observations == 0
            || self.max_observations > MAX_SYMBOL_OBSERVATIONS
            || self.max_report_rows == 0
            || self.max_report_rows > MAX_SYMBOL_REPORT_ROWS
        {
            return Err(SymbolCoverageError::InvalidLimits);
        }
        Ok(())
    }
}

/// Per-location hit facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationCoverage {
    pub function: String,
    pub source_file: String,
    pub source_line: u32,
    pub source_column: u32,
    pub hits: u64,
}

/// Per-module symbolized facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleCoverage {
    pub module: ModuleIdentity,
    pub complete: bool,
    pub unmapped_hits: u64,
    pub locations: Vec<LocationCoverage>,
}

/// Complete bounded mapping result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolizedCoverage {
    pub schema: String,
    pub modules: Vec<ModuleCoverage>,
    pub unaccounted_observations: u64,
    pub unaccounted_hits: u64,
    pub mapped_observations: u64,
    pub mapped_hits: u64,
}

/// One differential-coverage row. This is descriptive, not causal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DifferentialCoverageRow {
    pub function: String,
    pub source_file: String,
    pub source_line: u32,
    pub source_column: u32,
    pub failing_hits: u64,
    pub control_hits: u64,
    pub delta_hits: i128,
}

/// Evidence receipt for one symbolized mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolizedCoverageReceipt {
    pub schema: String,
    pub catalog_ids: Vec<ModuleIdentity>,
    pub load_ids: Vec<LoadedModule>,
    pub observation_source: [u8; 32],
    pub limits: SymbolCoverageLimits,
    pub mapped_observations: u64,
    pub mapped_hits: u64,
    pub unaccounted_observations: u64,
    pub unaccounted_hits: u64,
    pub incomplete_modules: Vec<ModuleIdentity>,
    pub report_digest: [u8; 32],
}

/// Symbolized coverage failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolCoverageError {
    InvalidLimits,
    CatalogSchema,
    DuplicateCatalog,
    DuplicateModuleLoad,
    ModuleBound,
    LocationBound,
    ObservationBound,
    ReportRowBound,
    InvalidPath,
    InvalidText,
    EmptyIds,
    IdBound,
    DuplicateId,
    CounterOverflow,
    ReportDigest,
    ReceiptDrift,
}

impl core::fmt::Display for SymbolCoverageError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::InvalidLimits => "symbol coverage limits are invalid",
            Self::CatalogSchema => "symbol catalog schema is not current",
            Self::DuplicateCatalog => "duplicate symbol catalog identity",
            Self::DuplicateModuleLoad => "duplicate module load identity",
            Self::ModuleBound => "symbol module count exceeds the bound",
            Self::LocationBound => "symbol location count exceeds the bound",
            Self::ObservationBound => "coverage observation count exceeds the bound",
            Self::ReportRowBound => "symbolized report rows exceed the bound",
            Self::InvalidPath => "source or module path is not relative and confined",
            Self::InvalidText => "symbol text is empty, over-bound, or contains controls",
            Self::EmptyIds => "symbol location has no instrumented identifiers",
            Self::IdBound => "instrumented identifier count exceeds the bound",
            Self::DuplicateId => "instrumented identifier appears in more than one location",
            Self::CounterOverflow => "symbol coverage counter overflow",
            Self::ReportDigest => "symbolized report digest failed",
            Self::ReceiptDrift => "symbolized coverage receipt does not match the inputs",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for SymbolCoverageError {}

/// Admit one catalog and return its canonical identity.
pub fn admit_catalog(
    catalog: &SymbolCatalog,
    limits: &SymbolCoverageLimits,
) -> Result<(), SymbolCoverageError> {
    limits.validate()?;
    if catalog.schema != SYMBOL_CATALOG_SCHEMA {
        return Err(SymbolCoverageError::CatalogSchema);
    }
    validate_module(&catalog.module)?;
    validate_text(&catalog.language)?;
    if catalog.locations.len() > limits.max_locations_per_module {
        return Err(SymbolCoverageError::LocationBound);
    }
    if !catalog.complete
        && catalog
            .incomplete_reason
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Err(SymbolCoverageError::CatalogSchema);
    }
    let mut ids = BTreeSet::new();
    for location in &catalog.locations {
        validate_text(&location.function)?;
        validate_path(&location.source_file)?;
        if location.source_line == 0 || location.source_column == 0 {
            return Err(SymbolCoverageError::InvalidText);
        }
        if location.instrumented_ids.is_empty() {
            return Err(SymbolCoverageError::EmptyIds);
        }
        if location.instrumented_ids.len() > MAX_INSTRUMENTED_IDS_PER_LOCATION {
            return Err(SymbolCoverageError::IdBound);
        }
        for id in &location.instrumented_ids {
            if !ids.insert(*id) {
                return Err(SymbolCoverageError::DuplicateId);
            }
        }
    }
    Ok(())
}

/// Map admitted observations through catalogs and runtime module loads.
pub fn map_coverage(
    catalogs: &[SymbolCatalog],
    loads: &[LoadedModule],
    observations: &[CoverageObservation],
    limits: &SymbolCoverageLimits,
) -> Result<SymbolizedCoverage, SymbolCoverageError> {
    limits.validate()?;
    if catalogs.len() > limits.max_modules || loads.len() > limits.max_modules {
        return Err(SymbolCoverageError::ModuleBound);
    }
    if observations.len() > limits.max_observations {
        return Err(SymbolCoverageError::ObservationBound);
    }

    let mut catalog_by_id: BTreeMap<[u8; 32], &SymbolCatalog> = BTreeMap::new();
    let mut id_to_location: BTreeMap<[u8; 32], BTreeMap<u64, LocationKey>> = BTreeMap::new();
    for catalog in catalogs {
        admit_catalog(catalog, limits)?;
        if catalog_by_id
            .insert(catalog.module.content_id, catalog)
            .is_some()
        {
            return Err(SymbolCoverageError::DuplicateCatalog);
        }
        let locations = id_to_location.entry(catalog.module.content_id).or_default();
        for location in &catalog.locations {
            let key = LocationKey::of(location);
            for id in &location.instrumented_ids {
                locations.insert(*id, key.clone());
            }
        }
    }
    let mut loaded = BTreeSet::new();
    for load in loads {
        validate_module(&load.module)?;
        if !loaded.insert(load.module.content_id) {
            return Err(SymbolCoverageError::DuplicateModuleLoad);
        }
    }

    let mut module_hits: BTreeMap<[u8; 32], BTreeMap<LocationKey, u64>> = BTreeMap::new();
    let mut unmapped_hits: BTreeMap<[u8; 32], u64> = BTreeMap::new();
    let mut unaccounted_observations = 0_u64;
    let mut unaccounted_hits = 0_u64;
    let mut mapped_observations = 0_u64;
    let mut mapped_hits = 0_u64;

    for observation in observations {
        if !catalog_by_id.contains_key(&observation.module_id) {
            unaccounted_observations = unaccounted_observations.saturating_add(1);
            unaccounted_hits = unaccounted_hits.saturating_add(observation.hits);
            continue;
        }
        if !loaded.contains(&observation.module_id) {
            unaccounted_observations = unaccounted_observations.saturating_add(1);
            unaccounted_hits = unaccounted_hits.saturating_add(observation.hits);
            continue;
        }
        let Some(key) = id_to_location
            .get(&observation.module_id)
            .and_then(|locations| locations.get(&observation.identifier))
        else {
            let hits = unmapped_hits.entry(observation.module_id).or_insert(0);
            *hits = hits.saturating_add(observation.hits);
            unaccounted_hits = unaccounted_hits.saturating_add(observation.hits);
            unaccounted_observations = unaccounted_observations.saturating_add(1);
            continue;
        };
        let hits = module_hits
            .entry(observation.module_id)
            .or_default()
            .entry(key.clone())
            .or_insert(0);
        *hits = hits.saturating_add(observation.hits);
        mapped_observations = mapped_observations.saturating_add(1);
        mapped_hits = mapped_hits.saturating_add(observation.hits);
    }

    let mut modules = Vec::new();
    let mut rows = 0_usize;
    for (module_id, catalog) in &catalog_by_id {
        let hits_by_location = module_hits.remove(module_id).unwrap_or_default();
        let mut locations = Vec::new();
        for location in &catalog.locations {
            if let Some(hits) = hits_by_location.get(&LocationKey::of(location)) {
                rows = rows
                    .checked_add(1)
                    .ok_or(SymbolCoverageError::CounterOverflow)?;
                if rows > limits.max_report_rows {
                    return Err(SymbolCoverageError::ReportRowBound);
                }
                locations.push(LocationCoverage {
                    function: location.function.clone(),
                    source_file: location.source_file.clone(),
                    source_line: location.source_line,
                    source_column: location.source_column,
                    hits: *hits,
                });
            }
        }
        modules.push(ModuleCoverage {
            module: catalog.module.clone(),
            complete: catalog.complete,
            unmapped_hits: unmapped_hits.get(module_id).copied().unwrap_or(0),
            locations,
        });
    }
    modules.sort_by(|left, right| left.module.path.cmp(&right.module.path));

    Ok(SymbolizedCoverage {
        schema: SYMBOLIZED_REPORT_SCHEMA.to_string(),
        modules,
        unaccounted_observations,
        unaccounted_hits,
        mapped_observations,
        mapped_hits,
    })
}

/// Rank source locations by descriptive failing-versus-control coverage delta.
pub fn rank_differential(
    failing: &SymbolizedCoverage,
    control: &SymbolizedCoverage,
    limits: &SymbolCoverageLimits,
) -> Result<Vec<DifferentialCoverageRow>, SymbolCoverageError> {
    limits.validate()?;
    let mut rows: BTreeMap<(String, u32, u32, String), (u64, u64)> = BTreeMap::new();
    for (is_failing, coverage) in [(true, failing), (false, control)] {
        for module in &coverage.modules {
            for location in &module.locations {
                let key = (
                    location.source_file.clone(),
                    location.source_line,
                    location.source_column,
                    location.function.clone(),
                );
                let entry = rows.entry(key).or_insert((0, 0));
                if is_failing {
                    entry.0 = entry.0.saturating_add(location.hits);
                } else {
                    entry.1 = entry.1.saturating_add(location.hits);
                }
            }
        }
    }
    let mut ranked: Vec<DifferentialCoverageRow> = rows
        .into_iter()
        .map(
            |(
                (source_file, source_line, source_column, function),
                (failing_hits, control_hits),
            )| {
                DifferentialCoverageRow {
                    function,
                    source_file,
                    source_line,
                    source_column,
                    failing_hits,
                    control_hits,
                    delta_hits: i128::from(failing_hits) - i128::from(control_hits),
                }
            },
        )
        .collect();
    ranked.sort_by(|left, right| {
        right
            .delta_hits
            .abs()
            .cmp(&left.delta_hits.abs())
            .then_with(|| left.source_file.cmp(&right.source_file))
            .then_with(|| left.source_line.cmp(&right.source_line))
            .then_with(|| left.source_column.cmp(&right.source_column))
            .then_with(|| left.function.cmp(&right.function))
    });
    if ranked.len() > limits.max_report_rows {
        return Err(SymbolCoverageError::ReportRowBound);
    }
    Ok(ranked)
}

/// Recompute the mapping and reject any receipt that differs from it.
pub fn validate_receipt(
    receipt: &SymbolizedCoverageReceipt,
    catalogs: &[SymbolCatalog],
    loads: &[LoadedModule],
    observations: &[CoverageObservation],
    observation_source: [u8; 32],
    limits: &SymbolCoverageLimits,
) -> Result<(), SymbolCoverageError> {
    let coverage = map_coverage(catalogs, loads, observations, limits)?;
    let expected = receipt_for(&coverage, catalogs, loads, observation_source, limits)?;
    if &expected != receipt {
        return Err(SymbolCoverageError::ReceiptDrift);
    }
    Ok(())
}

/// Build a receipt binding catalogs, loads, limits, and report identity.
pub fn receipt_for(
    coverage: &SymbolizedCoverage,
    catalogs: &[SymbolCatalog],
    loads: &[LoadedModule],
    observation_source: [u8; 32],
    limits: &SymbolCoverageLimits,
) -> Result<SymbolizedCoverageReceipt, SymbolCoverageError> {
    let report_bytes =
        serde_json::to_vec(coverage).map_err(|_| SymbolCoverageError::ReportDigest)?;
    let report_digest = *blake3::hash(&report_bytes).as_bytes();
    let mut catalog_ids = catalogs
        .iter()
        .map(|catalog| catalog.module.clone())
        .collect::<Vec<_>>();
    catalog_ids.sort();
    let mut load_ids = loads.to_vec();
    load_ids.sort_by_key(|load| (load.load_ordinal, load.module.content_id));
    let mut incomplete_modules = coverage
        .modules
        .iter()
        .filter(|module| !module.complete)
        .map(|module| module.module.clone())
        .collect::<Vec<_>>();
    incomplete_modules.sort();
    Ok(SymbolizedCoverageReceipt {
        schema: SYMBOLIZED_RECEIPT_SCHEMA.to_string(),
        catalog_ids,
        load_ids,
        observation_source,
        limits: *limits,
        mapped_observations: coverage.mapped_observations,
        mapped_hits: coverage.mapped_hits,
        unaccounted_observations: coverage.unaccounted_observations,
        unaccounted_hits: coverage.unaccounted_hits,
        incomplete_modules,
        report_digest,
    })
}

fn validate_module(module: &ModuleIdentity) -> Result<(), SymbolCoverageError> {
    validate_path(&module.path)?;
    if module.content_id == [0; 32] {
        return Err(SymbolCoverageError::InvalidText);
    }
    Ok(())
}

fn validate_path(value: &str) -> Result<(), SymbolCoverageError> {
    if value.is_empty()
        || value.len() > MAX_SOURCE_PATH_BYTES
        || value.starts_with('/')
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(SymbolCoverageError::InvalidPath);
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<(), SymbolCoverageError> {
    if value.is_empty()
        || value.len() > MAX_FUNCTION_NAME_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(SymbolCoverageError::InvalidText);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(id: u8, path: &str) -> ModuleIdentity {
        ModuleIdentity {
            content_id: [id; 32],
            path: path.to_string(),
        }
    }

    fn location(file: &str, line: u32, ids: &[u64]) -> SymbolLocation {
        SymbolLocation {
            function: format!("function_{line}"),
            source_file: file.to_string(),
            source_line: line,
            source_column: 2,
            instrumented_ids: ids.to_vec(),
        }
    }

    fn catalog(id: u8, path: &str, locations: Vec<SymbolLocation>) -> SymbolCatalog {
        SymbolCatalog {
            schema: SYMBOL_CATALOG_SCHEMA.to_string(),
            module: module(id, path),
            language: "rust".to_string(),
            instrumentation: InstrumentationKind::SanCov,
            complete: true,
            incomplete_reason: None,
            locations,
        }
    }

    fn load(id: u8, path: &str, ordinal: u32) -> LoadedModule {
        LoadedModule {
            module: module(id, path),
            load_ordinal: ordinal,
        }
    }

    fn observation(module_id: u8, identifier: u64, hits: u64) -> CoverageObservation {
        CoverageObservation {
            module_id: [module_id; 32],
            identifier,
            hits,
        }
    }

    fn coverage_with(hits: &[(&str, u32, u64)]) -> SymbolizedCoverage {
        SymbolizedCoverage {
            schema: SYMBOLIZED_REPORT_SCHEMA.to_string(),
            modules: vec![ModuleCoverage {
                module: module(1, "src/lib.rs"),
                complete: true,
                unmapped_hits: 0,
                locations: hits
                    .iter()
                    .map(|(file, line, count)| LocationCoverage {
                        function: format!("function_{line}"),
                        source_file: (*file).to_string(),
                        source_line: *line,
                        source_column: 2,
                        hits: *count,
                    })
                    .collect(),
            }],
            unaccounted_observations: 0,
            unaccounted_hits: 0,
            mapped_observations: hits.len() as u64,
            mapped_hits: hits.iter().map(|(_, _, count)| count).sum(),
        }
    }

    #[test]
    fn maps_admitted_locations_and_counts() {
        let catalogs = vec![catalog(
            1,
            "src/lib.rs",
            vec![
                location("src/lib.rs", 10, &[1, 2]),
                location("src/lib.rs", 20, &[3]),
            ],
        )];
        let loads = vec![load(1, "src/lib.rs", 0)];
        let observations = vec![observation(1, 1, 5), observation(1, 3, 2)];
        let mapped = map_coverage(
            &catalogs,
            &loads,
            &observations,
            &SymbolCoverageLimits::default(),
        )
        .expect("mapping");

        assert_eq!(mapped.mapped_observations, 2);
        assert_eq!(mapped.mapped_hits, 7);
        assert_eq!(mapped.modules.len(), 1);
        assert_eq!(mapped.modules[0].locations.len(), 2);
        assert_eq!(mapped.modules[0].locations[0].hits, 5);
        assert_eq!(mapped.modules[0].locations[1].hits, 2);
    }

    #[test]
    fn unknown_modules_and_ids_are_unaccounted_not_guessed() {
        let catalogs = vec![catalog(
            1,
            "src/lib.rs",
            vec![location("src/lib.rs", 10, &[1])],
        )];
        let loads = vec![load(1, "src/lib.rs", 0)];
        let observations = vec![observation(9, 1, 4), observation(1, 99, 3)];
        let mapped = map_coverage(
            &catalogs,
            &loads,
            &observations,
            &SymbolCoverageLimits::default(),
        )
        .expect("mapping");

        assert_eq!(mapped.unaccounted_observations, 2);
        assert_eq!(mapped.unaccounted_hits, 7);
        assert_eq!(mapped.mapped_observations, 0);
        assert_eq!(mapped.modules[0].unmapped_hits, 3);
    }

    #[test]
    fn duplicate_ids_paths_and_loads_are_rejected() {
        let mut duplicate = catalog(
            1,
            "src/lib.rs",
            vec![
                location("src/lib.rs", 10, &[1]),
                location("src/lib.rs", 20, &[1]),
            ],
        );
        assert_eq!(
            admit_catalog(&duplicate, &SymbolCoverageLimits::default()).unwrap_err(),
            SymbolCoverageError::DuplicateId
        );
        duplicate.locations = vec![location("../escape.rs", 10, &[1])];
        assert_eq!(
            admit_catalog(&duplicate, &SymbolCoverageLimits::default()).unwrap_err(),
            SymbolCoverageError::InvalidPath
        );

        let valid = catalog(1, "src/lib.rs", vec![location("src/lib.rs", 10, &[1])]);
        let duplicate_catalog = vec![valid.clone(), valid];
        assert_eq!(
            map_coverage(
                &duplicate_catalog,
                &[],
                &[],
                &SymbolCoverageLimits::default()
            )
            .unwrap_err(),
            SymbolCoverageError::DuplicateCatalog
        );
        assert_eq!(
            map_coverage(
                &duplicate_catalog[..1],
                &[load(1, "src/lib.rs", 0), load(1, "src/lib.rs", 1)],
                &[],
                &SymbolCoverageLimits::default()
            )
            .unwrap_err(),
            SymbolCoverageError::DuplicateModuleLoad
        );
    }

    #[test]
    fn incomplete_catalogs_are_reported_without_invented_locations() {
        let mut incomplete = catalog(1, "src/lib.rs", vec![location("src/lib.rs", 10, &[1])]);
        incomplete.complete = false;
        incomplete.incomplete_reason = Some("link-time optimization removed symbols".to_string());
        let loads = vec![load(1, "src/lib.rs", 0)];
        let mapped = map_coverage(
            std::slice::from_ref(&incomplete),
            &loads,
            &[observation(1, 1, 1)],
            &SymbolCoverageLimits::default(),
        )
        .expect("mapping");
        assert!(!mapped.modules[0].complete);
        let receipt = receipt_for(
            &mapped,
            &[incomplete],
            &loads,
            [7; 32],
            &SymbolCoverageLimits::default(),
        )
        .expect("receipt");
        assert_eq!(receipt.incomplete_modules, vec![module(1, "src/lib.rs")]);
    }

    #[test]
    fn differential_ranking_is_descriptive_and_ordered() {
        let failing = coverage_with(&[("src/a.rs", 10, 10), ("src/b.rs", 20, 1)]);
        let control = coverage_with(&[("src/a.rs", 10, 2), ("src/b.rs", 20, 6)]);
        let ranked = rank_differential(&failing, &control, &SymbolCoverageLimits::default())
            .expect("ranking");
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].source_file, "src/a.rs");
        assert_eq!(ranked[0].delta_hits, 8);
        assert_eq!(ranked[1].source_file, "src/b.rs");
        assert_eq!(ranked[1].delta_hits, -5);
    }

    #[test]
    fn maps_kcov_program_counters_and_shared_modules() {
        let mut kcov = catalog(
            1,
            "src/kernel.rs",
            vec![location("src/kernel.rs", 5, &[0xdead_beef])],
        );
        kcov.instrumentation = InstrumentationKind::Kcov;
        let shared = catalog(
            2,
            "lib/shared.so",
            vec![location("lib/shared.rs", 7, &[0xfeed_face])],
        );
        let loads = vec![load(1, "src/kernel.rs", 0), load(2, "lib/shared.so", 1)];
        let observations = vec![
            observation(1, 0xdead_beef, 2),
            observation(2, 0xfeed_face, 4),
        ];
        let mapped = map_coverage(
            &[kcov, shared],
            &loads,
            &observations,
            &SymbolCoverageLimits::default(),
        )
        .expect("mapping");
        assert_eq!(mapped.mapped_observations, 2);
        assert_eq!(mapped.mapped_hits, 6);
        assert_eq!(mapped.modules.len(), 2);
    }

    #[test]
    fn receipt_binds_report_digest_and_sorted_identities() {
        let catalogs = vec![
            catalog(2, "src/z.rs", vec![location("src/z.rs", 1, &[9])]),
            catalog(1, "src/a.rs", vec![location("src/a.rs", 1, &[1])]),
        ];
        let loads = vec![load(2, "src/z.rs", 1), load(1, "src/a.rs", 0)];
        let mapped = map_coverage(
            &catalogs,
            &loads,
            &[observation(1, 1, 3)],
            &SymbolCoverageLimits::default(),
        )
        .expect("mapping");
        let receipt = receipt_for(
            &mapped,
            &catalogs,
            &loads,
            [4; 32],
            &SymbolCoverageLimits::default(),
        )
        .expect("receipt");
        let bytes = serde_json::to_vec(&mapped).expect("report");
        assert_eq!(receipt.report_digest, *blake3::hash(&bytes).as_bytes());
        assert_eq!(receipt.catalog_ids[0], module(1, "src/a.rs"));
        assert_eq!(receipt.load_ids[0].load_ordinal, 0);
        assert_eq!(receipt.mapped_observations, 1);
    }

    #[test]
    fn receipt_validation_rejects_drift() {
        let catalogs = vec![catalog(
            1,
            "src/lib.rs",
            vec![location("src/lib.rs", 10, &[1])],
        )];
        let loads = vec![load(1, "src/lib.rs", 0)];
        let observations = vec![observation(1, 1, 3)];
        let limits = SymbolCoverageLimits::default();
        let coverage = map_coverage(&catalogs, &loads, &observations, &limits).unwrap();
        let receipt = receipt_for(&coverage, &catalogs, &loads, [4; 32], &limits).unwrap();
        validate_receipt(&receipt, &catalogs, &loads, &observations, [4; 32], &limits).unwrap();

        let mut wrong_source = receipt.clone();
        wrong_source.observation_source = [5; 32];
        assert_eq!(
            validate_receipt(
                &wrong_source,
                &catalogs,
                &loads,
                &observations,
                [4; 32],
                &limits
            )
            .unwrap_err(),
            SymbolCoverageError::ReceiptDrift
        );

        let mut wrong_limit = receipt.clone();
        wrong_limit.limits.max_report_rows -= 1;
        assert_eq!(
            validate_receipt(
                &wrong_limit,
                &catalogs,
                &loads,
                &observations,
                [4; 32],
                &limits
            )
            .unwrap_err(),
            SymbolCoverageError::ReceiptDrift
        );

        let mut wrong_digest = receipt.clone();
        wrong_digest.report_digest[0] ^= 1;
        assert_eq!(
            validate_receipt(
                &wrong_digest,
                &catalogs,
                &loads,
                &observations,
                [4; 32],
                &limits
            )
            .unwrap_err(),
            SymbolCoverageError::ReceiptDrift
        );

        let changed_catalogs = vec![catalog(
            1,
            "src/lib.rs",
            vec![location("src/lib.rs", 11, &[1])],
        )];
        assert_eq!(
            validate_receipt(
                &receipt,
                &changed_catalogs,
                &loads,
                &observations,
                [4; 32],
                &limits
            )
            .unwrap_err(),
            SymbolCoverageError::ReceiptDrift
        );

        let changed_loads = vec![load(1, "src/lib.rs", 1)];
        assert_eq!(
            validate_receipt(
                &receipt,
                &catalogs,
                &changed_loads,
                &observations,
                [4; 32],
                &limits
            )
            .unwrap_err(),
            SymbolCoverageError::ReceiptDrift
        );
    }

    #[test]
    fn limits_enforce_each_named_bound() {
        SymbolCoverageLimits::default().validate().unwrap();
        let over = [
            SymbolCoverageLimits {
                max_modules: MAX_SYMBOL_MODULES + 1,
                ..SymbolCoverageLimits::default()
            },
            SymbolCoverageLimits {
                max_locations_per_module: MAX_SYMBOL_LOCATIONS_PER_MODULE + 1,
                ..SymbolCoverageLimits::default()
            },
            SymbolCoverageLimits {
                max_observations: MAX_SYMBOL_OBSERVATIONS + 1,
                ..SymbolCoverageLimits::default()
            },
            SymbolCoverageLimits {
                max_report_rows: MAX_SYMBOL_REPORT_ROWS + 1,
                ..SymbolCoverageLimits::default()
            },
        ];
        for limits in over {
            assert_eq!(
                limits.validate().unwrap_err(),
                SymbolCoverageError::InvalidLimits
            );
        }
    }

    #[test]
    fn limits_fail_closed() {
        let catalog = catalog(
            1,
            "src/lib.rs",
            vec![
                location("src/lib.rs", 10, &[1]),
                location("src/lib.rs", 20, &[2]),
            ],
        );
        let limits = SymbolCoverageLimits {
            max_locations_per_module: 1,
            ..SymbolCoverageLimits::default()
        };
        assert_eq!(
            admit_catalog(&catalog, &limits).unwrap_err(),
            SymbolCoverageError::LocationBound
        );

        let limits = SymbolCoverageLimits {
            max_observations: 1,
            ..SymbolCoverageLimits::default()
        };
        assert_eq!(
            map_coverage(
                std::slice::from_ref(&catalog),
                &[load(1, "src/lib.rs", 0)],
                &[observation(1, 1, 1), observation(1, 2, 1)],
                &limits
            )
            .unwrap_err(),
            SymbolCoverageError::ObservationBound
        );
    }
}
