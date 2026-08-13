//! Pure Rust-workload scaffold projection.

// r[impl chaoscontrol.rust_automation.tools]
// r[impl chaoscontrol.rust_automation.parity]

use serde_json::{json, Value};

pub const TEXT_EXTENSIONS: [&str; 3] = ["md", "rs", "toml"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldPlan {
    pub package: String,
    pub replacements: Vec<(String, String)>,
    pub manifest: Value,
}

pub fn plan(workload: &str, source_root: &str) -> Result<ScaffoldPlan, String> {
    if workload.is_empty() || workload.contains(['\0', '\n', '\r']) {
        return Err(String::from(
            "workload name is empty or contains a control character",
        ));
    }
    if source_root.is_empty() {
        return Err(String::from("source root is empty"));
    }
    let package = format!(
        "{}-chaos-workload",
        workload.replace('_', "-").to_lowercase()
    );
    let sdk = format!("{source_root}/crates/chaoscontrol-sdk");
    let replacements = vec![
        (String::from("../../../crates/chaoscontrol-sdk"), sdk),
        (String::from("my-service-chaos-workload"), package.clone()),
        (String::from("my-service"), workload.to_string()),
    ];
    let manifest = json!({
        "schema": "chaoscontrol.rust_workload_scaffold.v1",
        "workload": workload,
        "template_source": "docs/templates/rust-workload",
        "local_dry_run": format!("CHAOSCONTROL_SDK_LOCAL_OUTPUT=/tmp/{workload}.sdk.jsonl cargo run --bin {package}"),
        "local_report": format!("summarize-sdk-local-output --input /tmp/{workload}.sdk.jsonl --output /tmp/{workload}.local-report.json"),
        "quality_gate": format!("check-sdk-assertion-quality --input /tmp/{workload}.local-report.json"),
        "bounded_vm_campaign": "nix run github:your-org/chaoscontrol#explore-rust-workload -- /tmp/cc-rust-workload-vm",
        "promotion_boundary": "local assertion quality is not snapshot-backed replay proof; require accepted replay verdict artifacts before support promotion",
    });
    Ok(ScaffoldPlan {
        package,
        replacements,
        manifest,
    })
}

pub fn transform_text(text: &str, replacements: &[(String, String)]) -> String {
    replacements
        .iter()
        .fold(text.to_string(), |current, (old, new)| {
            current.replace(old, new)
        })
}

#[cfg(test)]
mod tests {
    use super::{plan, transform_text};

    #[test]
    fn projection_preserves_public_manifest_and_replacements() {
        let plan = plan("My_Service", "/source").expect("plan");
        assert_eq!(plan.package, "my-service-chaos-workload");
        assert_eq!(plan.manifest["workload"], "My_Service");
        let transformed = transform_text(
            "my-service-chaos-workload ../../../crates/chaoscontrol-sdk my-service",
            &plan.replacements,
        );
        assert_eq!(
            transformed,
            "My_Service-chaos-workload /source/crates/chaoscontrol-sdk My_Service"
        );
    }

    #[test]
    fn invalid_names_fail_before_filesystem_effects() {
        assert!(plan("", "/source").is_err());
        assert!(plan("bad\nname", "/source").is_err());
        assert!(plan("valid", "").is_err());
    }
}
