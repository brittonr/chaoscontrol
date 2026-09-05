use serde::{Deserialize, Serialize};

use crate::{ensure, EvidenceError, EvidenceResult};

pub const PRODUCT_SCOPE_REGISTRY_SOURCE: &str = "contracts/product-scope/registry.ncl";
pub const PRODUCT_SCOPE_REGISTRY_JSON: &str =
    "contracts/product-scope/generated/product-scope.json";
pub const PRODUCT_SCOPE_STATUS_DOC: &str = "docs/product-scope-status.md";
pub const PRODUCT_SCOPE_README_START: &str = "<!-- product-scope-facts:start -->";
pub const PRODUCT_SCOPE_README_END: &str = "<!-- product-scope-facts:end -->";
pub const PRODUCT_SCOPE_SUCCESS: &str =
    "product scope ok: registry, active changes, facts, claims, and documents";

const PRODUCT_SCOPE_SCHEMA_VERSION: u32 = 1;
const MAX_PRODUCT_SCOPE_FILE_BYTES: u64 = 2 * 1024 * 1024;
const INVALID_FIXTURES_DIRECTORY: &str = "contracts/product-scope/fixtures/invalid";
const README_PATH: &str = "README.md";
const DOCS_PATH_PREFIX: &str = "docs/";
const CARGO_WORKSPACE_HEADER: &str = "[workspace]";
const CARGO_MEMBERS_PREFIX: &str = "members = [";
const ACTIVE_CHANGES_DIRECTORY: &str = ".cairn/changes";
const TEST_INVENTORY_NOTE: &str =
    "The selected Cargo command owns the test inventory. This projection does not copy a test count.";

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ScopeState {
    Supported,
    Experimental,
    Deferred,
    Blocked,
    NonGoal,
}

impl ScopeState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Experimental => "experimental",
            Self::Deferred => "deferred",
            Self::Blocked => "blocked",
            Self::NonGoal => "non-goal",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceState {
    Passed,
    Missing,
    Blocked,
    NotRequired,
}

impl EvidenceState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Missing => "missing",
            Self::Blocked => "blocked",
            Self::NotRequired => "not-required",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EvidencePrerequisite {
    pub id: String,
    pub state: EvidenceState,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CapabilityScope {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub state: ScopeState,
    pub status_label: String,
    pub evidence: EvidencePrerequisite,
    pub boundary: String,
    pub documentation_targets: Vec<String>,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ChangeScopeIntent {
    pub change: String,
    pub owner: String,
    pub target_state: ScopeState,
    pub evidence_prerequisite: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ScopeAuthority {
    pub workspace_manifest: String,
    pub test_inventory_command: String,
    pub replay_manifest: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProductScopeRegistry {
    pub schema_version: u32,
    pub authority: ScopeAuthority,
    pub capabilities: Vec<CapabilityScope>,
    pub change_intents: Vec<ChangeScopeIntent>,
    pub prohibited_current_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryFacts {
    pub workspace_members: Vec<String>,
    pub historical_workload_rows: usize,
    pub active_changes: Vec<String>,
    pub test_inventory_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentClaim<'a> {
    pub capability_id: &'a str,
    pub claimed_state: ScopeState,
    pub historical: bool,
}

pub fn parse_product_scope_registry(text: &str) -> EvidenceResult<ProductScopeRegistry> {
    serde_json::from_str(text).map_err(|error| {
        EvidenceError::new(format!("invalid product-scope registry JSON: {error}"))
    })
}

// r[impl chaoscontrol.product_scope.registry]
// r[impl chaoscontrol.product_scope.functional_core]
pub fn validate_product_scope_registry(registry: &ProductScopeRegistry) -> EvidenceResult<()> {
    ensure(
        registry.schema_version == PRODUCT_SCOPE_SCHEMA_VERSION,
        "unsupported product-scope registry schema",
    )?;
    ensure(
        !registry.capabilities.is_empty(),
        "product-scope registry has no capabilities",
    )?;
    ensure(
        !registry.change_intents.is_empty(),
        "product-scope registry has no change intents",
    )?;
    validate_authority(&registry.authority)?;
    validate_capabilities(&registry.capabilities)?;
    validate_change_intents(&registry.change_intents)?;
    validate_prohibited_claims(&registry.prohibited_current_claims)
}

fn validate_authority(authority: &ScopeAuthority) -> EvidenceResult<()> {
    ensure(
        authority.workspace_manifest == "Cargo.toml",
        "workspace authority must be Cargo.toml",
    )?;
    ensure(
        authority.test_inventory_command.starts_with("cargo test ")
            && authority.test_inventory_command.ends_with(" --list"),
        "test inventory authority must be an explicit Cargo list command",
    )?;
    ensure(
        authority.replay_manifest == "dogfood-results/accepted-workload-proofs.json",
        "replay authority must be the accepted workload proof manifest",
    )
}

fn validate_capabilities(capabilities: &[CapabilityScope]) -> EvidenceResult<()> {
    let mut ids = std::collections::BTreeSet::new();
    let mut names = std::collections::BTreeSet::new();
    for capability in capabilities {
        ensure(
            ids.insert(capability.id.as_str()),
            format!("duplicate product capability id: {}", capability.id),
        )?;
        ensure(
            names.insert(capability.name.as_str()),
            format!("duplicate product capability name: {}", capability.name),
        )?;
        ensure(
            !capability.owner.is_empty()
                && !capability.status_label.is_empty()
                && !capability.evidence.id.is_empty()
                && !capability.evidence.source.is_empty()
                && !capability.boundary.is_empty(),
            format!(
                "capability has incomplete ownership facts: {}",
                capability.id
            ),
        )?;
        ensure(
            !capability.documentation_targets.is_empty() && !capability.non_claims.is_empty(),
            format!(
                "capability has incomplete document facts: {}",
                capability.id
            ),
        )?;
        for target in &capability.documentation_targets {
            ensure(
                target == README_PATH || target.starts_with(DOCS_PATH_PREFIX),
                format!("capability has unsafe document target: {target}"),
            )?;
        }
        if capability.state == ScopeState::Supported {
            ensure(
                capability.evidence.state == EvidenceState::Passed,
                format!(
                    "supported capability lacks passing evidence: {}",
                    capability.id
                ),
            )?;
        }
        if capability.state == ScopeState::NonGoal {
            ensure(
                capability.evidence.state == EvidenceState::NotRequired,
                format!(
                    "non-goal capability has a promoting evidence state: {}",
                    capability.id
                ),
            )?;
        }
    }
    Ok(())
}

fn validate_change_intents(intents: &[ChangeScopeIntent]) -> EvidenceResult<()> {
    let mut names = std::collections::BTreeSet::new();
    for intent in intents {
        ensure(
            names.insert(intent.change.as_str()),
            format!("duplicate change scope intent: {}", intent.change),
        )?;
        ensure(
            !intent.owner.is_empty()
                && !intent.evidence_prerequisite.is_empty()
                && !intent.non_claims.is_empty(),
            format!("change scope intent is incomplete: {}", intent.change),
        )?;
    }
    Ok(())
}

fn validate_prohibited_claims(claims: &[String]) -> EvidenceResult<()> {
    ensure(
        !claims.is_empty(),
        "product-scope registry has no prohibited current claims",
    )?;
    let mut unique = std::collections::BTreeSet::new();
    for claim in claims {
        ensure(
            !claim.trim().is_empty() && unique.insert(claim.as_str()),
            "product-scope prohibited claims must be non-empty and unique",
        )?;
    }
    Ok(())
}

pub fn validate_active_change_intents(
    active_changes: &[String],
    registry: &ProductScopeRegistry,
) -> EvidenceResult<()> {
    let intents = registry
        .change_intents
        .iter()
        .map(|intent| intent.change.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for change in active_changes {
        ensure(
            intents.contains(change.as_str()),
            format!("active change lacks a product-scope intent: {change}"),
        )?;
    }
    Ok(())
}

pub fn validate_document_claim(
    claim: &DocumentClaim<'_>,
    registry: &ProductScopeRegistry,
) -> EvidenceResult<()> {
    if claim.historical {
        return Ok(());
    }
    let capability = registry
        .capabilities
        .iter()
        .find(|capability| capability.id == claim.capability_id)
        .ok_or_else(|| {
            EvidenceError::new(format!(
                "document claim names an unknown capability: {}",
                claim.capability_id
            ))
        })?;
    ensure(
        capability.state == claim.claimed_state,
        format!(
            "current document claim differs from scope registry: {}",
            claim.capability_id
        ),
    )
}

// r[impl chaoscontrol.product_scope.promotion]
// r[impl chaoscontrol.product_scope.boundary]
pub fn validate_current_claim_text(
    text: &str,
    registry: &ProductScopeRegistry,
) -> EvidenceResult<()> {
    let lowered = text.to_ascii_lowercase();
    for claim in &registry.prohibited_current_claims {
        ensure(
            !lowered.contains(&claim.to_ascii_lowercase()),
            format!("document contains a prohibited current claim: {claim}"),
        )?;
    }
    Ok(())
}

pub fn parse_workspace_members(manifest: &str) -> EvidenceResult<Vec<String>> {
    let mut in_workspace = false;
    let mut in_members = false;
    let mut members = Vec::new();
    for raw_line in manifest.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            if line == CARGO_WORKSPACE_HEADER {
                in_workspace = true;
                continue;
            }
            if in_workspace {
                break;
            }
        }
        if !in_workspace {
            continue;
        }
        if line.starts_with(CARGO_MEMBERS_PREFIX) {
            in_members = true;
            continue;
        }
        if in_members && line == "]" {
            break;
        }
        if in_members {
            let candidate = line.trim_end_matches(',').trim();
            let member = candidate
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'));
            ensure(
                member.is_some(),
                format!("malformed workspace member: {line}"),
            )?;
            members.push(member.expect("member presence asserted").to_string());
        }
    }
    ensure(
        !members.is_empty(),
        "Cargo workspace has no explicit members",
    )?;
    let unique = members.iter().collect::<std::collections::BTreeSet<_>>();
    ensure(
        unique.len() == members.len(),
        "Cargo workspace contains duplicate members",
    )?;
    Ok(members)
}

pub fn parse_historical_workload_row_count(manifest: &str) -> EvidenceResult<usize> {
    let value: serde_json::Value = serde_json::from_str(manifest).map_err(|error| {
        EvidenceError::new(format!("invalid accepted workload proof manifest: {error}"))
    })?;
    let proofs = value
        .get("proofs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| EvidenceError::new("accepted workload proof manifest lacks proofs"))?;
    Ok(proofs.len())
}

// r[impl chaoscontrol.product_scope.documentation]
pub fn render_product_scope_readme_block(
    registry: &ProductScopeRegistry,
    facts: &RepositoryFacts,
) -> String {
    let state_counts = scope_state_counts(registry);
    format!(
        "{PRODUCT_SCOPE_README_START}\n> **Product scope:** {} supported, {} experimental, {} deferred, {} blocked, and {} non-goal capabilities.\n>\n> The workspace has {} crates from `{}`. The replay manifest has {} historical workload rows.\n>\n> {} The authority is `{}`.\n>\n> Generated facts do not prove correctness, release eligibility, hosted support, or universal determinism.\n{PRODUCT_SCOPE_README_END}",
        count_for(&state_counts, ScopeState::Supported),
        count_for(&state_counts, ScopeState::Experimental),
        count_for(&state_counts, ScopeState::Deferred),
        count_for(&state_counts, ScopeState::Blocked),
        count_for(&state_counts, ScopeState::NonGoal),
        facts.workspace_members.len(),
        registry.authority.workspace_manifest,
        facts.historical_workload_rows,
        TEST_INVENTORY_NOTE,
        facts.test_inventory_command,
    )
}

pub fn render_product_scope_status(
    registry: &ProductScopeRegistry,
    facts: &RepositoryFacts,
) -> String {
    let mut output = String::new();
    output.push_str("# Product Scope Status\n\n");
    output.push_str("Generated from `contracts/product-scope/registry.ncl` and named repository facts. Do not edit this file.\n\n");
    output.push_str("## Architecture facts\n\n");
    output.push_str(&format!(
        "The Cargo workspace has {} explicit crates. The source is `{}`.\n\n",
        facts.workspace_members.len(),
        registry.authority.workspace_manifest
    ));
    output.push_str("| Capability | State | Status label | Owner | Evidence | Boundary |\n");
    output.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for capability in &registry.capabilities {
        output.push_str(&format!(
            "| {} | `{}` | `{}` | `{}` | `{}` from `{}` | {} |\n",
            capability.name,
            capability.state.as_str(),
            capability.status_label,
            capability.owner,
            capability.evidence.state.as_str(),
            capability.evidence.source,
            capability.boundary,
        ));
    }
    output.push_str("\n## Readiness facts\n\n");
    output.push_str(&format!(
        "The replay manifest has {} historical workload rows. Historical rows do not become fresh v2 proof.\n\n",
        facts.historical_workload_rows
    ));
    output.push_str(TEST_INVENTORY_NOTE);
    output.push_str(&format!(
        " The selected command is `{}`.\n\n",
        facts.test_inventory_command
    ));
    output.push_str("## Active change admission\n\n");
    output.push_str("| Change | Target state | Owner | Evidence prerequisite |\n");
    output.push_str("| --- | --- | --- | --- |\n");
    for change in &facts.active_changes {
        if let Some(intent) = registry
            .change_intents
            .iter()
            .find(|intent| intent.change == *change)
        {
            output.push_str(&format!(
                "| `{}` | `{}` | `{}` | {} |\n",
                intent.change,
                intent.target_state.as_str(),
                intent.owner,
                intent.evidence_prerequisite,
            ));
        }
    }
    output.push_str("\n## Roadmap by scope state\n\n");
    for state in [
        ScopeState::Supported,
        ScopeState::Experimental,
        ScopeState::Deferred,
        ScopeState::Blocked,
        ScopeState::NonGoal,
    ] {
        output.push_str(&format!("### {}\n\n", title_state(state)));
        for capability in registry
            .capabilities
            .iter()
            .filter(|capability| capability.state == state)
        {
            output.push_str(&format!(
                "- `{}`: {} Evidence is `{}`.\n",
                capability.id,
                capability.boundary,
                capability.evidence.state.as_str(),
            ));
        }
        output.push('\n');
    }
    output.push_str("## Claim boundary\n\n");
    output.push_str("These facts do not prove code quality, correctness, release eligibility, hosted support, or universal determinism.\n");
    output
}

fn scope_state_counts(
    registry: &ProductScopeRegistry,
) -> std::collections::BTreeMap<ScopeState, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for capability in &registry.capabilities {
        *counts.entry(capability.state).or_insert(0) += 1;
    }
    counts
}

fn count_for(counts: &std::collections::BTreeMap<ScopeState, usize>, state: ScopeState) -> usize {
    counts.get(&state).copied().unwrap_or(0)
}

fn title_state(state: ScopeState) -> &'static str {
    match state {
        ScopeState::Supported => "Supported",
        ScopeState::Experimental => "Experimental",
        ScopeState::Deferred => "Deferred",
        ScopeState::Blocked => "Blocked",
        ScopeState::NonGoal => "Non-goals",
    }
}

pub fn replace_marked_block(
    document: &str,
    start_marker: &str,
    end_marker: &str,
    replacement: &str,
) -> EvidenceResult<String> {
    let start = document
        .find(start_marker)
        .ok_or_else(|| EvidenceError::new(format!("missing start marker: {start_marker}")))?;
    let relative_end = document[start..]
        .find(end_marker)
        .ok_or_else(|| EvidenceError::new(format!("missing end marker: {end_marker}")))?;
    let end = start + relative_end + end_marker.len();
    Ok(format!(
        "{}{}{}",
        &document[..start],
        replacement,
        &document[end..]
    ))
}

// r[impl chaoscontrol.product_scope.change_admission]
// r[impl chaoscontrol.product_scope.validation]
pub fn check_product_scope(
    root: impl AsRef<std::path::Path>,
    write: bool,
) -> EvidenceResult<&'static str> {
    let root = root.as_ref();
    let registry = load_and_check_registry_projection(root)?;
    validate_product_scope_registry(&registry)?;
    let facts = collect_repository_facts(root, &registry)?;
    validate_active_change_intents(&facts.active_changes, &registry)?;
    update_or_check_documents(root, &registry, &facts, write)?;
    validate_document_targets(root, &registry)?;
    Ok(PRODUCT_SCOPE_SUCCESS)
}

fn load_and_check_registry_projection(
    root: &std::path::Path,
) -> EvidenceResult<ProductScopeRegistry> {
    let generated_path = root.join(PRODUCT_SCOPE_REGISTRY_JSON);
    let generated = crate::bounded_file::read_bounded_regular_file(
        &generated_path,
        MAX_PRODUCT_SCOPE_FILE_BYTES,
    )?;
    let generated_value: serde_json::Value = serde_json::from_str(&generated).map_err(|error| {
        EvidenceError::new(format!("invalid generated product-scope JSON: {error}"))
    })?;
    let exported = export_nickel(root, PRODUCT_SCOPE_REGISTRY_SOURCE)?;
    let exported_value: serde_json::Value = serde_json::from_slice(&exported).map_err(|error| {
        EvidenceError::new(format!("invalid Nickel product-scope export: {error}"))
    })?;
    ensure(
        generated_value == exported_value,
        "product-scope projection drift; export contracts/product-scope/registry.ncl",
    )?;
    check_invalid_nickel_fixtures(root)?;
    parse_product_scope_registry(&generated)
}

fn export_nickel(root: &std::path::Path, relative_path: &str) -> EvidenceResult<Vec<u8>> {
    let output = std::process::Command::new("nickel")
        .args(["export", "--format", "json"])
        .arg(root.join(relative_path))
        .current_dir(root)
        .output()
        .map_err(|error| EvidenceError::new(format!("failed to run Nickel: {error}")))?;
    ensure(
        output.status.success(),
        format!(
            "Nickel rejected product-scope source: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    )?;
    Ok(output.stdout)
}

fn check_invalid_nickel_fixtures(root: &std::path::Path) -> EvidenceResult<()> {
    let directory = root.join(INVALID_FIXTURES_DIRECTORY);
    let mut fixtures = std::fs::read_dir(&directory)
        .map_err(|error| EvidenceError::new(format!("failed to read invalid fixtures: {error}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| EvidenceError::new(format!("failed to list invalid fixtures: {error}")))?;
    fixtures.sort_by_key(std::fs::DirEntry::file_name);
    ensure(
        !fixtures.is_empty(),
        "product-scope registry has no invalid Nickel fixtures",
    )?;
    for fixture in fixtures {
        let path = fixture.path();
        ensure(
            path.extension().is_some_and(|extension| extension == "ncl"),
            format!("unexpected invalid fixture file: {}", path.display()),
        )?;
        let output = std::process::Command::new("nickel")
            .args(["export", "--format", "json"])
            .arg(&path)
            .current_dir(root)
            .output()
            .map_err(|error| EvidenceError::new(format!("failed to run Nickel: {error}")))?;
        ensure(
            !output.status.success(),
            format!("invalid product-scope fixture passed: {}", path.display()),
        )?;
    }
    Ok(())
}

fn collect_repository_facts(
    root: &std::path::Path,
    registry: &ProductScopeRegistry,
) -> EvidenceResult<RepositoryFacts> {
    let manifest_path = root.join(&registry.authority.workspace_manifest);
    let manifest = crate::bounded_file::read_bounded_regular_file(
        &manifest_path,
        MAX_PRODUCT_SCOPE_FILE_BYTES,
    )?;
    let replay_path = root.join(&registry.authority.replay_manifest);
    let replay =
        crate::bounded_file::read_bounded_regular_file(&replay_path, MAX_PRODUCT_SCOPE_FILE_BYTES)?;
    Ok(RepositoryFacts {
        workspace_members: parse_workspace_members(&manifest)?,
        historical_workload_rows: parse_historical_workload_row_count(&replay)?,
        active_changes: discover_active_changes(root)?,
        test_inventory_command: registry.authority.test_inventory_command.clone(),
    })
}

fn discover_active_changes(root: &std::path::Path) -> EvidenceResult<Vec<String>> {
    let directory = root.join(ACTIVE_CHANGES_DIRECTORY);
    let mut changes = Vec::new();
    for entry in std::fs::read_dir(&directory)
        .map_err(|error| EvidenceError::new(format!("failed to read active changes: {error}")))?
    {
        let entry = entry.map_err(|error| {
            EvidenceError::new(format!("failed to list active changes: {error}"))
        })?;
        let file_type = entry.file_type().map_err(|error| {
            EvidenceError::new(format!("failed to inspect active change: {error}"))
        })?;
        ensure(
            file_type.is_dir() && !file_type.is_symlink(),
            format!(
                "active change entry is not a directory: {}",
                entry.path().display()
            ),
        )?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| EvidenceError::new("active change name is not valid UTF-8"))?;
        changes.push(name);
    }
    changes.sort();
    Ok(changes)
}

fn update_or_check_documents(
    root: &std::path::Path,
    registry: &ProductScopeRegistry,
    facts: &RepositoryFacts,
    write: bool,
) -> EvidenceResult<()> {
    let readme_path = root.join(README_PATH);
    let readme =
        crate::bounded_file::read_bounded_regular_file(&readme_path, MAX_PRODUCT_SCOPE_FILE_BYTES)?;
    let readme_block = render_product_scope_readme_block(registry, facts);
    let expected_readme = replace_marked_block(
        &readme,
        PRODUCT_SCOPE_README_START,
        PRODUCT_SCOPE_README_END,
        &readme_block,
    )?;
    let status_path = root.join(PRODUCT_SCOPE_STATUS_DOC);
    let expected_status = render_product_scope_status(registry, facts);
    validate_current_claim_text(&expected_readme, registry)?;
    validate_current_claim_text(&expected_status, registry)?;
    check_or_write(&readme_path, &readme, &expected_readme, write)?;
    let actual_status = read_optional_bounded(&status_path)?;
    check_or_write(&status_path, &actual_status, &expected_status, write)
}

fn validate_document_targets(
    root: &std::path::Path,
    registry: &ProductScopeRegistry,
) -> EvidenceResult<()> {
    for capability in &registry.capabilities {
        for target in &capability.documentation_targets {
            let path = root.join(target);
            let text = crate::bounded_file::read_bounded_regular_file(
                &path,
                MAX_PRODUCT_SCOPE_FILE_BYTES,
            )?;
            if target == PRODUCT_SCOPE_STATUS_DOC || target == "docs/replay-readiness-status.md" {
                ensure(
                    text.contains(&capability.status_label),
                    format!(
                        "document target lacks capability status label: {} in {}",
                        capability.status_label, target
                    ),
                )?;
            }
        }
    }
    Ok(())
}

fn read_optional_bounded(path: &std::path::Path) -> EvidenceResult<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    crate::bounded_file::read_bounded_regular_file(path, MAX_PRODUCT_SCOPE_FILE_BYTES)
}

fn check_or_write(
    path: &std::path::Path,
    actual: &str,
    expected: &str,
    write: bool,
) -> EvidenceResult<()> {
    if actual == expected {
        return Ok(());
    }
    if !write {
        return Err(EvidenceError::new(format!(
            "product-scope document drift: {}",
            path.display()
        )));
    }
    std::fs::write(path, expected).map_err(|error| {
        EvidenceError::new(format!(
            "failed to write product-scope document {}: {error}",
            path.display()
        ))
    })
}

pub fn product_scope_paths(
    root: impl AsRef<std::path::Path>,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = root.as_ref();
    (root.join(README_PATH), root.join(PRODUCT_SCOPE_STATUS_DOC))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_registry() -> ProductScopeRegistry {
        ProductScopeRegistry {
            schema_version: PRODUCT_SCOPE_SCHEMA_VERSION,
            authority: ScopeAuthority {
                workspace_manifest: "Cargo.toml".to_string(),
                test_inventory_command: "cargo test --workspace --all-targets -- --list"
                    .to_string(),
                replay_manifest: "dogfood-results/accepted-workload-proofs.json".to_string(),
            },
            capabilities: vec![CapabilityScope {
                id: "local-control-plane".to_string(),
                name: "Local control plane".to_string(),
                owner: "chaoscontrol-evidence".to_string(),
                state: ScopeState::Supported,
                status_label: "supported-bounded-local".to_string(),
                evidence: EvidencePrerequisite {
                    id: "local-proof".to_string(),
                    state: EvidenceState::Passed,
                    source: "receipt.json".to_string(),
                },
                boundary: "One machine.".to_string(),
                documentation_targets: vec![README_PATH.to_string()],
                non_claims: vec!["No hosted service.".to_string()],
            }],
            change_intents: vec![ChangeScopeIntent {
                change: "scope-change".to_string(),
                owner: "chaoscontrol-evidence".to_string(),
                target_state: ScopeState::Supported,
                evidence_prerequisite: "The scope gate passes.".to_string(),
                non_claims: vec!["No correctness claim.".to_string()],
            }],
            prohibited_current_claims: vec!["universal determinism is proven".to_string()],
        }
    }

    // r[verify chaoscontrol.product_scope.validation]
    #[test]
    fn valid_registry_and_active_change_pass() {
        let registry = valid_registry();
        validate_product_scope_registry(&registry).expect("valid registry");
        validate_active_change_intents(&["scope-change".to_string()], &registry)
            .expect("known active change");
    }

    // r[verify chaoscontrol.product_scope.validation]
    #[test]
    fn duplicate_and_incomplete_registry_facts_fail_closed() {
        let mut registry = valid_registry();
        registry.capabilities.push(registry.capabilities[0].clone());
        assert!(validate_product_scope_registry(&registry).is_err());

        let mut registry = valid_registry();
        registry.capabilities[0].evidence.state = EvidenceState::Missing;
        assert!(validate_product_scope_registry(&registry).is_err());

        let mut registry = valid_registry();
        registry.capabilities[0].documentation_targets = vec!["../README.md".to_string()];
        assert!(validate_product_scope_registry(&registry).is_err());
    }

    #[test]
    fn unknown_active_change_and_current_state_drift_fail_closed() {
        let registry = valid_registry();
        assert!(
            validate_active_change_intents(&["unknown-change".to_string()], &registry).is_err()
        );
        let current = DocumentClaim {
            capability_id: "local-control-plane",
            claimed_state: ScopeState::Experimental,
            historical: false,
        };
        assert!(validate_document_claim(&current, &registry).is_err());
    }

    #[test]
    fn historical_state_is_preserved_but_current_overclaim_is_rejected() {
        let registry = valid_registry();
        let historical = DocumentClaim {
            capability_id: "removed-capability",
            claimed_state: ScopeState::Supported,
            historical: true,
        };
        validate_document_claim(&historical, &registry).expect("historical claim remains readable");
        assert!(validate_current_claim_text(
            "This text says universal determinism is proven.",
            &registry
        )
        .is_err());
    }

    #[test]
    fn workspace_parser_accepts_explicit_members_and_rejects_malformed_input() {
        let valid =
            "[workspace]\nmembers = [\n  \"crates/a\",\n  \"crates/b\",\n]\nresolver = \"2\"\n";
        assert_eq!(
            parse_workspace_members(valid).expect("valid members"),
            vec!["crates/a".to_string(), "crates/b".to_string()]
        );
        let malformed = "[workspace]\nmembers = [\n  crates/a,\n]\n";
        assert!(parse_workspace_members(malformed).is_err());
    }

    #[test]
    fn projection_replacement_and_fact_count_detect_stale_documents() {
        let registry = valid_registry();
        let facts = RepositoryFacts {
            workspace_members: vec!["crates/a".to_string()],
            historical_workload_rows: 0,
            active_changes: vec!["scope-change".to_string()],
            test_inventory_command: registry.authority.test_inventory_command.clone(),
        };
        let block = render_product_scope_readme_block(&registry, &facts);
        let source = format!(
            "# Demo\n\n{PRODUCT_SCOPE_README_START}\nstale count: 9\n{PRODUCT_SCOPE_README_END}\n"
        );
        let updated = replace_marked_block(
            &source,
            PRODUCT_SCOPE_README_START,
            PRODUCT_SCOPE_README_END,
            &block,
        )
        .expect("markers");
        assert!(updated.contains("workspace has 1 crates"));
        assert!(!updated.contains("stale count: 9"));
        assert!(replace_marked_block(
            "# Missing markers\n",
            PRODUCT_SCOPE_README_START,
            PRODUCT_SCOPE_README_END,
            &block,
        )
        .is_err());
    }

    #[test]
    fn workload_manifest_requires_a_proofs_array() {
        assert_eq!(
            parse_historical_workload_row_count(r#"{"proofs":[{},{}]}"#).expect("proof count"),
            2
        );
        assert!(parse_historical_workload_row_count(r#"{"proofs":{}}"#).is_err());
        assert!(parse_historical_workload_row_count("not-json").is_err());
    }
}
