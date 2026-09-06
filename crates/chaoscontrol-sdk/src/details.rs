//! Structured assertion detail builders.
//!
//! These helpers produce well-typed [`serde_json::Value`] objects for
//! common assertion patterns.  Using them instead of ad-hoc `json!({})`
//! guarantees consistent key names across all assertion sites.
//!
//! # Example
//!
//! ```rust
//! use chaoscontrol_sdk::assert::details;
//!
//! let d = details::merge(
//!     &details::node(0, 5, "leader"),
//!     &details::log(3, 5, 10),
//! );
//! assert_eq!(d["node_id"], 0);
//! assert_eq!(d["log_index"], 3);
//! ```
//!
//! Raw `json!({})` usage is still supported — these helpers are additive.

use serde_json::json;

/// Standard key constants for assertion detail fields.
///
/// Use these instead of string literals to prevent typos and keep
/// field names consistent across all assertion call sites.
pub mod keys {
    /// Node identity within a cluster (`usize`).
    pub const NODE_ID: &str = "node_id";
    /// Raft/consensus term number (`u64`).
    pub const TERM: &str = "term";
    /// Node role string (e.g. `"leader"`, `"follower"`, `"candidate"`).
    pub const ROLE: &str = "role";
    /// Log entry index (`usize`).
    pub const LOG_INDEX: &str = "log_index";
    /// Total log length (`usize`).
    pub const LOG_LENGTH: &str = "log_length";
    /// Committed log index (`usize`).
    pub const COMMIT_INDEX: &str = "commit_index";
    /// Remote peer identity (`usize`).
    pub const PEER_ID: &str = "peer_id";
    /// RPC / message type string.
    pub const MESSAGE_TYPE: &str = "message_type";
    /// Source of a network message (`usize`).
    pub const FROM: &str = "from";
    /// Destination of a network message (`usize`).
    pub const TO: &str = "to";
    /// Whether a message was delivered (`bool`).
    pub const DELIVERED: &str = "delivered";
    /// Fault type string (e.g. `"partition"`, `"kill"`).
    pub const FAULT_TYPE: &str = "fault_type";
    /// Fault target (`usize`).
    pub const TARGET: &str = "target";
}

/// Build node-state details.
///
/// Captures the identity, term, and role of a consensus node at the
/// moment an assertion fires.
///
/// ```rust
/// # use chaoscontrol_sdk::assert::details;
/// let d = details::node(2, 7, "leader");
/// assert_eq!(d["node_id"], 2);
/// assert_eq!(d["term"], 7);
/// assert_eq!(d["role"], "leader");
/// ```
pub fn node(id: usize, term: u64, role: &str) -> ::serde_json::Value {
    json!({
        keys::NODE_ID: id,
        keys::TERM: term,
        keys::ROLE: role,
    })
}

/// Build log-operation details.
///
/// Captures the index, term, and length of a replicated log at the
/// moment an assertion fires.
///
/// ```rust
/// # use chaoscontrol_sdk::assert::details;
/// let d = details::log(3, 5, 10);
/// assert_eq!(d["log_index"], 3);
/// assert_eq!(d["term"], 5);
/// assert_eq!(d["log_length"], 10);
/// ```
pub fn log(index: usize, term: u64, length: usize) -> ::serde_json::Value {
    json!({
        keys::LOG_INDEX: index,
        keys::LOG_LENGTH: length,
        keys::TERM: term,
    })
}

/// Build network-event details.
///
/// Captures the source, destination, and delivery status of a message.
///
/// ```rust
/// # use chaoscontrol_sdk::assert::details;
/// let d = details::network(0, 1, true);
/// assert_eq!(d["from"], 0);
/// assert_eq!(d["to"], 1);
/// assert_eq!(d["delivered"], true);
/// ```
pub fn network(from: usize, to: usize, delivered: bool) -> ::serde_json::Value {
    json!({
        keys::FROM: from,
        keys::TO: to,
        keys::DELIVERED: delivered,
    })
}

/// Build fault-condition details.
///
/// Captures the fault type and target node.
///
/// ```rust
/// # use chaoscontrol_sdk::assert::details;
/// let d = details::fault("partition", 2);
/// assert_eq!(d["fault_type"], "partition");
/// assert_eq!(d["target"], 2);
/// ```
pub fn fault(fault_type: &str, target: usize) -> ::serde_json::Value {
    json!({
        keys::FAULT_TYPE: fault_type,
        keys::TARGET: target,
    })
}

/// Merge two detail objects into one.
///
/// Both values must be JSON objects.  Keys from `b` overwrite keys
/// from `a` on collision.  Non-object values are returned as-is
/// (prefers `a`).
///
/// ```rust
/// # use chaoscontrol_sdk::assert::details;
/// let combined = details::merge(
///     &details::node(0, 5, "leader"),
///     &details::log(3, 5, 10),
/// );
/// assert_eq!(combined["node_id"], 0);
/// assert_eq!(combined["log_index"], 3);
/// ```
pub fn merge(a: &::serde_json::Value, b: &::serde_json::Value) -> ::serde_json::Value {
    match (a, b) {
        (::serde_json::Value::Object(ma), ::serde_json::Value::Object(mb)) => {
            let mut merged = ma.clone();
            for (k, v) in mb {
                merged.insert(k.clone(), v.clone());
            }
            ::serde_json::Value::Object(merged)
        }
        _ => a.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_has_correct_keys() {
        let d = node(2, 7, "leader");
        assert_eq!(d[keys::NODE_ID], 2);
        assert_eq!(d[keys::TERM], 7);
        assert_eq!(d[keys::ROLE], "leader");
    }

    #[test]
    fn log_has_correct_keys() {
        let d = log(3, 5, 10);
        assert_eq!(d[keys::LOG_INDEX], 3);
        assert_eq!(d[keys::TERM], 5);
        assert_eq!(d[keys::LOG_LENGTH], 10);
    }

    #[test]
    fn network_has_correct_keys() {
        let d = network(0, 1, true);
        assert_eq!(d[keys::FROM], 0);
        assert_eq!(d[keys::TO], 1);
        assert_eq!(d[keys::DELIVERED], true);
    }

    #[test]
    fn fault_has_correct_keys() {
        let d = fault("partition", 2);
        assert_eq!(d[keys::FAULT_TYPE], "partition");
        assert_eq!(d[keys::TARGET], 2);
    }

    #[test]
    fn merge_combines_both_objects() {
        let a = node(0, 5, "leader");
        let b = log(3, 5, 10);
        let combined = merge(&a, &b);

        assert_eq!(combined[keys::NODE_ID], 0);
        assert_eq!(combined[keys::ROLE], "leader");
        assert_eq!(combined[keys::LOG_INDEX], 3);
        assert_eq!(combined[keys::LOG_LENGTH], 10);
        // Shared key "term" — b overwrites a
        assert_eq!(combined[keys::TERM], 5);
    }

    #[test]
    fn merge_b_overwrites_a_on_collision() {
        let a = json!({"x": 1, "y": 2});
        let b = json!({"y": 99, "z": 3});
        let combined = merge(&a, &b);

        assert_eq!(combined["x"], 1);
        assert_eq!(combined["y"], 99);
        assert_eq!(combined["z"], 3);
    }

    #[test]
    fn merge_non_objects_returns_a() {
        let a = json!(42);
        let b = json!("hello");
        assert_eq!(merge(&a, &b), json!(42));
    }

    #[test]
    fn merge_empty_objects() {
        let a = json!({});
        let b = json!({});
        assert_eq!(merge(&a, &b), json!({}));
    }

    #[test]
    fn node_and_log_compose() {
        let d = merge(&node(1, 3, "follower"), &log(5, 3, 20));
        assert_eq!(d[keys::NODE_ID], 1);
        assert_eq!(d[keys::ROLE], "follower");
        assert_eq!(d[keys::LOG_INDEX], 5);
        assert_eq!(d[keys::LOG_LENGTH], 20);
        assert_eq!(d[keys::TERM], 3);
    }
}
