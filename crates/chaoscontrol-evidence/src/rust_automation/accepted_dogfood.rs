//! Pure accepted snapshot-verdict dogfood decisions.

// r[impl chaoscontrol.rust_automation.evidence]
// r[impl chaoscontrol.rust_automation.parity]

use std::path::Path;

use serde_json::{json, Value};

pub const REPLAY_VERDICT_SCHEMA_VERSION: u64 = 2;
pub const SNAPSHOT_CODEC: &str = "simulation-snapshot-cbor-zstd-v2";
pub const SNAPSHOT_SCHEMA_VERSION: u64 = 2;

pub fn assertion_matches_profile(identity: &Value, profile: &Value) -> bool {
    let descriptor = identity.get("descriptor").unwrap_or(&Value::Null);
    let logical_key = descriptor.get("logical_key").unwrap_or(&Value::Null);
    descriptor.get("namespace") == profile.get("namespace")
        && logical_key.get("type").and_then(Value::as_str) == Some("stable")
        && logical_key.get("key") == profile.get("logical_key")
        && descriptor.get("compatibility_id") == profile.get("compatibility_id")
        && descriptor.get("guest") == profile.get("guest")
        && descriptor.get("category") == profile.get("category")
        && descriptor.get("message") == profile.get("message")
        && hex_identity(identity.get("fingerprint"))
        && hex_identity(identity.get("catalog_token"))
}

pub fn snapshot_bug_is_candidate(
    bug: &Value,
    assertion_id: i64,
    assertion_profile: &Value,
) -> bool {
    let faults = bug
        .pointer("/schedule/faults")
        .and_then(Value::as_array)
        .is_some_and(|values| !values.is_empty());
    bug.get("assertion_id").and_then(Value::as_i64) == Some(assertion_id)
        && assertion_matches_profile(
            bug.get("assertion_identity").unwrap_or(&Value::Null),
            assertion_profile,
        )
        && bug
            .get("replay_parent_depth")
            .and_then(Value::as_i64)
            .is_some_and(|depth| depth > 0)
        && bug
            .get("replay_parent_snapshot_ref")
            .is_some_and(Value::is_object)
        && faults
}

pub fn validate_snapshot_reference(reference: &Value, actual_digest: &str) -> Result<(), String> {
    let digest = reference
        .get("digest")
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("snapshot digest is missing"))?;
    if !digest.starts_with("sha256:") {
        return Err(format!("unsupported snapshot digest: {digest}"));
    }
    if digest != actual_digest {
        return Err(format!(
            "snapshot digest mismatch: expected {digest}, got {actual_digest}"
        ));
    }
    if reference.get("codec").and_then(Value::as_str) != Some(SNAPSHOT_CODEC) {
        return Err(format!(
            "unexpected snapshot codec: {}",
            display(reference.get("codec"))
        ));
    }
    if reference.get("schema_version").and_then(Value::as_u64) != Some(SNAPSHOT_SCHEMA_VERSION) {
        return Err(format!(
            "unexpected snapshot schema: {}",
            display(reference.get("schema_version"))
        ));
    }
    Ok(())
}

pub fn verdict_is_accepted(
    verdict: &Value,
    bug: &Value,
    assertion_id: i64,
    bug_path: &Path,
) -> bool {
    let reference = verdict
        .pointer("/snapshot/reference")
        .unwrap_or(&Value::Null);
    let hashes = verdict
        .get("artifact_hashes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    verdict.get("schema_version").and_then(Value::as_u64) == Some(REPLAY_VERDICT_SCHEMA_VERSION)
        && verdict.get("replay_class").and_then(Value::as_str) == Some("snapshot_backed_reproduced")
        && verdict.get("reproduced").and_then(Value::as_bool) == Some(true)
        && verdict.get("assertion_id").and_then(Value::as_i64) == Some(assertion_id)
        && verdict.get("assertion_identity") == bug.get("assertion_identity")
        && verdict
            .get("replay_parent_depth")
            .and_then(Value::as_i64)
            .is_some_and(|depth| depth > 0)
        && verdict.pointer("/snapshot/status").and_then(Value::as_str) == Some("valid")
        && verdict
            .pointer("/snapshot/digest_verified")
            .and_then(Value::as_bool)
            == Some(true)
        && reference.get("codec").and_then(Value::as_str) == Some(SNAPSHOT_CODEC)
        && reference.get("schema_version").and_then(Value::as_u64) == Some(SNAPSHOT_SCHEMA_VERSION)
        && verdict
            .pointer("/command/exit_status")
            .and_then(Value::as_i64)
            == Some(0)
        && hashes.iter().any(|item| {
            item.get("path")
                .and_then(Value::as_str)
                .is_some_and(|path| same_lexical_path(Path::new(path), bug_path))
        })
}

pub struct AttemptInput<'a> {
    pub workload: &'a str,
    pub seed: i64,
    pub fail_after: i64,
    pub run_exit_status: i32,
    pub export_exit_status: Option<i32>,
    pub reproduce_exit_status: Option<i32>,
    pub bugs: &'a [(String, Value)],
    pub verdict_path: Option<&'a Path>,
    pub verdict: Option<&'a Value>,
}

pub fn summarize_attempt(input: &AttemptInput<'_>) -> Value {
    let bugs = input
        .bugs
        .iter()
        .map(|(name, bug)| {
            json!({
                "file": name,
                "assertion_id": bug.get("assertion_id").cloned().unwrap_or(Value::Null),
                "replay_parent_depth": bug.get("replay_parent_depth").cloned().unwrap_or(Value::Null),
                "has_snapshot_ref": bug.get("replay_parent_snapshot_ref").is_some(),
                "has_assertion_identity": bug.get("assertion_identity").is_some(),
            })
        })
        .collect::<Vec<_>>();
    let verdict = match (input.verdict_path, input.verdict) {
        (Some(path), Some(verdict)) => json!({
            "path": path,
            "replay_class": verdict.get("replay_class").cloned().unwrap_or(Value::Null),
            "reproduced": verdict.get("reproduced").cloned().unwrap_or(Value::Null),
            "replay_parent_depth": verdict.get("replay_parent_depth").cloned().unwrap_or(Value::Null),
            "snapshot_status": verdict.pointer("/snapshot/status").cloned().unwrap_or(Value::Null),
        }),
        _ => Value::Null,
    };
    json!({
        "workload": input.workload,
        "seed": input.seed,
        "snapshot_probe_fail_after": input.fail_after,
        "run_exit_status": input.run_exit_status,
        "export_exit_status": input.export_exit_status,
        "reproduce_exit_status": input.reproduce_exit_status,
        "bugs": bugs,
        "verdict": verdict,
    })
}

pub fn rewrite_public_verdict(
    verdict: &mut Value,
    old_bug_path: &str,
    public_bug_path: &str,
    public_snapshot_path: &str,
    bug_digest: &str,
    snapshot_digest: &str,
) -> Result<(), String> {
    verdict["bug_path"] = Value::String(public_bug_path.to_string());
    if let Some(command) = verdict.pointer_mut("/command/command") {
        let text = command
            .as_str()
            .ok_or_else(|| String::from("verdict command.command must be a string"))?;
        *command = Value::String(text.replace(old_bug_path, public_bug_path));
    }
    verdict["artifact_hashes"] = json!([
        {"path": public_bug_path, "sha256": bug_digest},
        {"path": public_snapshot_path, "sha256": snapshot_digest},
    ]);
    Ok(())
}

fn hex_identity(value: Option<&Value>) -> bool {
    const HEX_LENGTH: usize = 64;
    value.and_then(Value::as_str).is_some_and(|text| {
        text.len() == HEX_LENGTH && text.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn same_lexical_path(left: &Path, right: &Path) -> bool {
    left == right
}

fn display(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
        None => String::from("null"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::{
        assertion_matches_profile, rewrite_public_verdict, snapshot_bug_is_candidate,
        validate_snapshot_reference,
    };

    fn profile() -> serde_json::Value {
        json!({"namespace": "ns", "logical_key": "key", "compatibility_id": 7, "guest": "g", "category": "invariant", "message": "m"})
    }

    fn identity() -> serde_json::Value {
        json!({"descriptor": {"namespace": "ns", "logical_key": {"type": "stable", "key": "key"}, "compatibility_id": 7, "guest": "g", "category": "invariant", "message": "m"}, "fingerprint": "a".repeat(64), "catalog_token": "b".repeat(64)})
    }

    #[test]
    fn profile_and_fault_backed_bug_match() {
        assert!(assertion_matches_profile(&identity(), &profile()));
        let bug = json!({"assertion_id": 7, "assertion_identity": identity(), "replay_parent_depth": 1, "replay_parent_snapshot_ref": {}, "schedule": {"faults": [{}]}});
        assert!(snapshot_bug_is_candidate(&bug, 7, &profile()));
    }

    #[test]
    fn stale_snapshot_and_missing_fault_fail_closed() {
        let reference =
            json!({"digest": "sha256:old", "codec": super::SNAPSHOT_CODEC, "schema_version": 2});
        assert!(validate_snapshot_reference(&reference, "sha256:new").is_err());
        let bug = json!({"assertion_id": 7, "assertion_identity": identity(), "replay_parent_depth": 1, "replay_parent_snapshot_ref": {}, "schedule": {"faults": []}});
        assert!(!snapshot_bug_is_candidate(&bug, 7, &profile()));
    }

    #[test]
    fn display_command_is_rewritten_without_argument_execution() {
        let mut verdict = json!({"command": {"command": "tool --bug /old/bug.json"}});
        rewrite_public_verdict(
            &mut verdict,
            "/old/bug.json",
            "public/bug.json",
            "public/snapshot",
            "sha256:a",
            "sha256:b",
        )
        .expect("rewrite");
        assert_eq!(verdict["command"]["command"], "tool --bug public/bug.json");
        assert_eq!(verdict["bug_path"], "public/bug.json");
        let _ = Path::new("public/bug.json");
    }
}
