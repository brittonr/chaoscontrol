#!/usr/bin/env -S CARGO_TARGET_DIR=/tmp/onix-chaoscontrol-workflow-skill-script-target nix shell "github:nix-community/fenix?rev=092bd452904e749efa39907aa4a20a42678ac31e#minimal.toolchain" nixpkgs#gcc -c cargo -q -Zscript

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

const DEFAULT_SKILL_ROOT: &str = "docs/skills/chaoscontrol-workflow";
const SKILL_FILE: &str = "SKILL.md";
const EXPECTED_NAME_LINE: &str = "name: chaoscontrol-workflow";
const DESCRIPTION_PREFIX: &str = "description:";
const FRONTMATTER_OPEN: &str = "---\n";
const FRONTMATTER_CLOSE: &str = "\n---\n";
const KIBIBYTE_BYTES: u64 = 1024;
const MAX_SOURCE_KIBIBYTES: u64 = 64;
const MAX_SOURCE_BYTES: u64 = MAX_SOURCE_KIBIBYTES * KIBIBYTE_BYTES;
const EXIT_FAILURE: i32 = 1;

const RESEARCH_REFERENCE: &str = "references/research.md";
const WORKLOAD_REFERENCE: &str = "references/workload.md";
const CAMPAIGN_REFERENCE: &str = "references/campaign.md";
const TRIAGE_REFERENCE: &str = "references/triage.md";

const REQUIRED_REFERENCE_PATHS: &[&str] = &[
    RESEARCH_REFERENCE,
    WORKLOAD_REFERENCE,
    CAMPAIGN_REFERENCE,
    TRIAGE_REFERENCE,
];

const REQUIRED_SKILL_MARKERS: &[&str] = &[
    "Goal:",
    "## Evidence classes",
    "## Stop conditions",
    "## Completion report",
    "snapshot_backed_reproduced",
];

const REFERENCE_MARKERS: &[(&str, &[&str])] = &[
    (
        RESEARCH_REFERENCE,
        &[
            "# Research a ChaosControl Workload",
            "portfolio-search",
            "## Negative paths",
        ],
    ),
    (
        WORKLOAD_REFERENCE,
        &[
            "# Onboard a Rust Workload",
            "external harness",
            "## Positive and negative paths",
        ],
    ),
    (
        CAMPAIGN_REFERENCE,
        &[
            "# Run a Campaign",
            "snapshot_backed_reproduced",
            "## Negative paths",
        ],
    ),
    (
        TRIAGE_REFERENCE,
        &[
            "# Triage ChaosControl Evidence",
            "decision receipt",
            "## Negative paths",
        ],
    ),
];

const FORBIDDEN_RUNTIME_MARKERS: &[&str] = &[
    "snouty launch",
    "snouty validate",
    "docker compose build",
    "kubectl apply",
    "agent-browser --",
    "ANTITHESIS_API_KEY=",
];

const ISSUE_FRONTMATTER: &str = "frontmatter";
const ISSUE_NAME: &str = "name";
const ISSUE_DESCRIPTION: &str = "description";
const ISSUE_SKILL_MARKER: &str = "skill_marker";
const ISSUE_REFERENCE_LINK: &str = "reference_link";
const ISSUE_REFERENCE_MISSING: &str = "reference_missing";
const ISSUE_REFERENCE_EMPTY: &str = "reference_empty";
const ISSUE_REFERENCE_MARKER: &str = "reference_marker";
const ISSUE_FORBIDDEN_RUNTIME: &str = "forbidden_runtime";

#[derive(Debug, Eq, PartialEq)]
struct AuditIssue {
    code: &'static str,
    detail: String,
}

fn issue(code: &'static str, detail: impl Into<String>) -> AuditIssue {
    AuditIssue {
        code,
        detail: detail.into(),
    }
}

fn frontmatter(contents: &str) -> Result<&str, AuditIssue> {
    if !contents.starts_with(FRONTMATTER_OPEN) {
        return Err(issue(
            ISSUE_FRONTMATTER,
            "SKILL.md must start with a frontmatter delimiter",
        ));
    }
    let body = &contents[FRONTMATTER_OPEN.len()..];
    let Some(close_index) = body.find(FRONTMATTER_CLOSE) else {
        return Err(issue(
            ISSUE_FRONTMATTER,
            "SKILL.md must contain a closing frontmatter delimiter",
        ));
    };
    Ok(&body[..close_index])
}

fn audit_frontmatter(contents: &str, issues: &mut Vec<AuditIssue>) {
    let frontmatter = match frontmatter(contents) {
        Ok(value) => value,
        Err(error) => {
            issues.push(error);
            return;
        }
    };
    if !frontmatter.lines().any(|line| line == EXPECTED_NAME_LINE) {
        issues.push(issue(ISSUE_NAME, "skill name is missing or incorrect"));
    }
    let has_description = frontmatter.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with(DESCRIPTION_PREFIX) && trimmed.len() > DESCRIPTION_PREFIX.len()
    });
    if !has_description {
        issues.push(issue(
            ISSUE_DESCRIPTION,
            "skill description is missing or empty",
        ));
    }
}

fn audit_required_markers(contents: &str, issues: &mut Vec<AuditIssue>) {
    for marker in REQUIRED_SKILL_MARKERS {
        if !contents.contains(marker) {
            issues.push(issue(
                ISSUE_SKILL_MARKER,
                format!("SKILL.md is missing required marker: {marker}"),
            ));
        }
    }
}

fn reference_contents<'a>(references: &'a [(String, String)], path: &str) -> Option<&'a str> {
    references
        .iter()
        .find(|(candidate, _)| candidate == path)
        .map(|(_, contents)| contents.as_str())
}

fn audit_references(
    skill_contents: &str,
    references: &[(String, String)],
    issues: &mut Vec<AuditIssue>,
) {
    for path in REQUIRED_REFERENCE_PATHS {
        if !skill_contents.contains(path) {
            issues.push(issue(
                ISSUE_REFERENCE_LINK,
                format!("SKILL.md does not link required reference: {path}"),
            ));
        }
        let Some(contents) = reference_contents(references, path) else {
            issues.push(issue(
                ISSUE_REFERENCE_MISSING,
                format!("required reference is missing: {path}"),
            ));
            continue;
        };
        if contents.trim().is_empty() {
            issues.push(issue(
                ISSUE_REFERENCE_EMPTY,
                format!("required reference is empty: {path}"),
            ));
        }
    }
}

fn audit_reference_markers(references: &[(String, String)], issues: &mut Vec<AuditIssue>) {
    for (path, markers) in REFERENCE_MARKERS {
        let Some(contents) = reference_contents(references, path) else {
            continue;
        };
        for marker in *markers {
            if !contents.contains(marker) {
                issues.push(issue(
                    ISSUE_REFERENCE_MARKER,
                    format!("{path} is missing required marker: {marker}"),
                ));
            }
        }
    }
}

fn audit_forbidden_runtime(
    skill_contents: &str,
    references: &[(String, String)],
    issues: &mut Vec<AuditIssue>,
) {
    for marker in FORBIDDEN_RUNTIME_MARKERS {
        if skill_contents.contains(marker)
            || references
                .iter()
                .any(|(_, contents)| contents.contains(marker))
        {
            issues.push(issue(
                ISSUE_FORBIDDEN_RUNTIME,
                format!("skill source invokes a forbidden runtime surface: {marker}"),
            ));
        }
    }
}

fn audit_source(skill_contents: &str, references: &[(String, String)]) -> Vec<AuditIssue> {
    let mut issues = Vec::new();
    audit_frontmatter(skill_contents, &mut issues);
    audit_required_markers(skill_contents, &mut issues);
    audit_references(skill_contents, references, &mut issues);
    audit_reference_markers(references, &mut issues);
    audit_forbidden_runtime(skill_contents, references, &mut issues);
    issues
}

fn valid_skill_fixture() -> String {
    format!(
        "---\n{EXPECTED_NAME_LINE}\ndescription: Valid fixture.\n---\nGoal:\n## Evidence classes\n## Stop conditions\n## Completion report\nsnapshot_backed_reproduced\n{RESEARCH_REFERENCE}\n{WORKLOAD_REFERENCE}\n{CAMPAIGN_REFERENCE}\n{TRIAGE_REFERENCE}\n"
    )
}

fn valid_reference_fixtures() -> Vec<(String, String)> {
    REFERENCE_MARKERS
        .iter()
        .map(|(path, markers)| (path.to_string(), markers.join("\n")))
        .collect()
}

fn has_issue(issues: &[AuditIssue], code: &str) -> bool {
    issues.iter().any(|candidate| candidate.code == code)
}

fn run_self_tests() {
    let skill = valid_skill_fixture();
    let references = valid_reference_fixtures();
    let valid_issues = audit_source(&skill, &references);
    assert!(valid_issues.is_empty());

    let wrong_name = skill.replace(EXPECTED_NAME_LINE, "name: wrong-skill");
    let wrong_name_issues = audit_source(&wrong_name, &references);
    assert!(has_issue(&wrong_name_issues, ISSUE_NAME));

    let mut missing_reference = references.clone();
    missing_reference.retain(|(path, _)| path != TRIAGE_REFERENCE);
    let missing_reference_issues = audit_source(&skill, &missing_reference);
    assert!(has_issue(
        &missing_reference_issues,
        ISSUE_REFERENCE_MISSING
    ));

    let mut empty_reference = references.clone();
    if let Some((_, contents)) = empty_reference
        .iter_mut()
        .find(|(path, _)| path == WORKLOAD_REFERENCE)
    {
        contents.clear();
    }
    let empty_reference_issues = audit_source(&skill, &empty_reference);
    assert!(has_issue(&empty_reference_issues, ISSUE_REFERENCE_EMPTY));
    assert!(has_issue(&empty_reference_issues, ISSUE_REFERENCE_MARKER));

    let forbidden_skill = format!("{skill}\nsnouty launch\n");
    let forbidden_issues = audit_source(&forbidden_skill, &references);
    assert!(has_issue(&forbidden_issues, ISSUE_FORBIDDEN_RUNTIME));

    let malformed_skill = skill.trim_start_matches(FRONTMATTER_OPEN);
    let malformed_issues = audit_source(malformed_skill, &references);
    assert!(has_issue(&malformed_issues, ISSUE_FRONTMATTER));
}

fn read_bounded(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    if metadata.len() > MAX_SOURCE_BYTES {
        return Err(format!(
            "{} exceeds the {MAX_SOURCE_KIBIBYTES} KiB source limit",
            path.display()
        ));
    }
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn load_source(root: &Path) -> Result<(String, Vec<(String, String)>), String> {
    let skill_path = root.join(SKILL_FILE);
    let skill_contents = read_bounded(&skill_path)?;
    let mut references = Vec::new();
    for relative_path in REQUIRED_REFERENCE_PATHS {
        let path = root.join(relative_path);
        let contents = read_bounded(&path)?;
        references.push((relative_path.to_string(), contents));
    }
    Ok((skill_contents, references))
}

fn audit_root(root: &Path) -> Result<(), String> {
    let (skill_contents, references) = load_source(root)?;
    let issues = audit_source(&skill_contents, &references);
    if issues.is_empty() {
        println!(
            "PASS chaoscontrol-workflow skill source: {}",
            root.display()
        );
        return Ok(());
    }
    for audit_issue in &issues {
        eprintln!("{}: {}", audit_issue.code, audit_issue.detail);
    }
    Err(format!(
        "skill source audit found {} issue(s)",
        issues.len()
    ))
}

fn run() -> Result<(), String> {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    match arguments.as_slice() {
        [] => {
            run_self_tests();
            audit_root(Path::new(DEFAULT_SKILL_ROOT))
        }
        [argument] if argument == OsStr::new("--self-test") => {
            run_self_tests();
            println!("PASS chaoscontrol-workflow skill source self-tests");
            Ok(())
        }
        [root] => {
            run_self_tests();
            audit_root(&PathBuf::from(root))
        }
        _ => Err(format!(
            "usage: tools/check-chaoscontrol-workflow-skill.rs [--self-test|SKILL_ROOT]"
        )),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR: {error}");
        process::exit(EXIT_FAILURE);
    }
}
