//! Bounded drift diagnosis for repeated restore probes.
//!
//! Compares two probe records and reports the first differing observation:
//! current tick, an executed round entry, or a final VM entry. The report is
//! diagnostic only; it does not classify nondeterminism causes.

use serde_json::Value;

// r[impl chaoscontrol.agent_investigation.restore_repeat_drift_detail]
/// Describe the first difference between two probe records.
///
/// Returns `None` when tick, round traces, and VM observations match.
pub fn first_difference(first: &Value, second: &Value) -> Option<Value> {
    if first["current_tick"] != second["current_tick"] {
        return Some(serde_json::json!({
            "kind": "tick",
            "first": first["current_tick"],
            "second": second["current_tick"],
        }));
    }
    if let Some(difference) = first_array_difference(
        "round",
        first["forward_rounds"].as_array(),
        second["forward_rounds"].as_array(),
    ) {
        return Some(difference);
    }
    first_array_difference("vm", first["vms"].as_array(), second["vms"].as_array())
}

fn first_array_difference(
    kind: &str,
    first: Option<&Vec<Value>>,
    second: Option<&Vec<Value>>,
) -> Option<Value> {
    let (first, second) = match (first, second) {
        (Some(first), Some(second)) => (first, second),
        _ => {
            return Some(serde_json::json!({
                "kind": kind,
                "first_len": first.map(Vec::len),
                "second_len": second.map(Vec::len),
            }))
        }
    };
    if first.len() != second.len() {
        return Some(serde_json::json!({
            "kind": kind,
            "first_len": first.len(),
            "second_len": second.len(),
        }));
    }
    for (index, (left, right)) in first.iter().zip(second).enumerate() {
        if left != right {
            let mut difference = serde_json::json!({
                "kind": kind,
                "index": index,
                "first": left,
                "second": right,
            });
            if let Some(tick) = left["tick"].as_u64() {
                difference["tick"] = serde_json::json!(tick);
            }
            return Some(difference);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(rounds: Value, vms: Value, tick: u64) -> Value {
        serde_json::json!({
            "current_tick": tick,
            "forward_rounds": rounds,
            "vms": vms,
        })
    }

    #[test]
    fn identical_records_have_no_difference() {
        let value = record(
            serde_json::json!([{"tick": 10, "faults": []}]),
            serde_json::json!([{"rip": 1, "rax": 2}]),
            10,
        );
        assert_eq!(first_difference(&value, &value), None);
    }

    #[test]
    fn tick_difference_is_reported_first() {
        let first = record(serde_json::json!([]), serde_json::json!([]), 10);
        let second = record(serde_json::json!([]), serde_json::json!([]), 11);
        let difference = first_difference(&first, &second).unwrap();
        assert_eq!(difference["kind"], "tick");
        assert_eq!(difference["first"], 10);
        assert_eq!(difference["second"], 11);
    }

    #[test]
    fn round_difference_names_the_tick() {
        let first = record(
            serde_json::json!([{"tick": 10, "faults": ["a"]}, {"tick": 11, "faults": []}]),
            serde_json::json!([]),
            11,
        );
        let second = record(
            serde_json::json!([{"tick": 10, "faults": ["a"]}, {"tick": 11, "faults": ["b"]}]),
            serde_json::json!([]),
            11,
        );
        let difference = first_difference(&first, &second).unwrap();
        assert_eq!(difference["kind"], "round");
        assert_eq!(difference["index"], 1);
        assert_eq!(difference["tick"], 11);
    }

    #[test]
    fn vm_or_length_differences_are_reported() {
        let first = record(
            serde_json::json!([]),
            serde_json::json!([{"rip": 1}, {"rip": 2}]),
            5,
        );
        let second = record(
            serde_json::json!([]),
            serde_json::json!([{"rip": 1}, {"rip": 3}]),
            5,
        );
        let difference = first_difference(&first, &second).unwrap();
        assert_eq!(difference["kind"], "vm");
        assert_eq!(difference["index"], 1);
        let shorter = record(serde_json::json!([]), serde_json::json!([{"rip": 1}]), 5);
        let difference = first_difference(&first, &shorter).unwrap();
        assert_eq!(difference["first_len"], 2);
        assert_eq!(difference["second_len"], 1);
    }
}
