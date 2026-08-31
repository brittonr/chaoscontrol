//! Guest-declared branch markers for bounded exploration guidance.

#[cfg(feature = "full")]
use chaoscontrol_protocol::branch_marker::{BranchMarker, BranchMarkerError, BRANCH_MARKER_EVENT};

/// Emit one stable branch marker without assigning pass or fail meaning.
///
/// The marker identity uses only `namespace` and `key`. Structured details and
/// optional instance references remain bound observations and do not change the
/// logical marker identity.
#[cfg(feature = "full")]
pub fn branch_marker(
    namespace: &str,
    key: &str,
    owner: &str,
    details: &serde_json::Value,
    state_ref: Option<&str>,
    logical_position_ref: Option<&str>,
) -> Result<String, BranchMarkerError> {
    let marker = BranchMarker::new(
        namespace,
        key,
        owner,
        details.clone(),
        state_ref.map(str::to_string),
        logical_position_ref.map(str::to_string),
    )?;
    let value = serde_json::to_value(&marker).map_err(|_| BranchMarkerError::DetailsTooLarge)?;
    crate::lifecycle::send_event(BRANCH_MARKER_EVENT, &value);
    Ok(marker.identity)
}

/// Declare and emit a branch marker in the assertion identity catalog.
#[macro_export]
macro_rules! cc_branch_marker {
    ($namespace:expr, $key:expr, $owner:expr, $message:expr, $details:expr $(,)?) => {{
        $crate::cc_branch_marker!($namespace, $key, $owner, $message, $details, None, None)
    }};
    ($namespace:expr, $key:expr, $owner:expr, $message:expr, $details:expr, $state_ref:expr, $logical_position_ref:expr $(,)?) => {{
        let __cc_result = $crate::marker::branch_marker(
            $namespace,
            $key,
            $owner,
            $details,
            $state_ref,
            $logical_position_ref,
        );
        if __cc_result.is_ok() {
            $crate::cc_assert_reachable_stable!(
                $namespace,
                $key,
                $owner,
                $crate::marker::BRANCH_MARKER_CATEGORY,
                $message,
                $details,
            );
        }
        __cc_result
    }};
}

#[doc(hidden)]
pub const BRANCH_MARKER_CATEGORY: &str =
    chaoscontrol_protocol::branch_marker::BRANCH_MARKER_ASSERTION_CATEGORY;

#[cfg(all(test, feature = "full"))]
mod tests {
    use super::*;

    const TEST_DIGEST_HEX_BYTES: usize = 64;

    #[test]
    fn marker_identity_is_stable_and_instance_refs_are_validated() {
        let first = branch_marker(
            "raft",
            "leader-elected",
            "guest-0",
            &serde_json::json!({"term": 1}),
            None,
            None,
        )
        .unwrap();
        let second = branch_marker(
            "raft",
            "leader-elected",
            "guest-1",
            &serde_json::json!({"term": 2}),
            Some(&format!("b3:{}", "a".repeat(TEST_DIGEST_HEX_BYTES))),
            Some("term:2"),
        )
        .unwrap();
        assert_eq!(first, second);
        let macro_identity = crate::cc_branch_marker!(
            "raft",
            "commit-index-advanced",
            "guest-0",
            "commit index advanced",
            &serde_json::json!({"index": 1}),
        )
        .unwrap();
        assert_ne!(first, macro_identity);
        assert!(branch_marker(
            "raft",
            "leader-elected",
            "guest-0",
            &serde_json::json!({}),
            Some("invalid"),
            None,
        )
        .is_err());
    }
}
