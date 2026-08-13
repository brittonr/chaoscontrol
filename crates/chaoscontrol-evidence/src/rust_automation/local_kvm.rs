//! Pure local multi-hypervisor campaign plan construction.

// r[impl chaoscontrol.rust_automation.kvm]
// r[impl chaoscontrol.rust_automation.functional_core]

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{json, Value};

const REQUIRED_WORKLOAD_COUNT: usize = 2;
const MAX_HYPERVISORS: usize = 2;
const VCPUS_PER_WORKER: u64 = 2;
const MEMORY_MIB_PER_WORKER: u64 = 1_024;

pub fn parse_workloads(raw: &str) -> Result<Vec<String>, String> {
    let workloads = raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if workloads.len() < REQUIRED_WORKLOAD_COUNT {
        return Err(String::from(
            "--workloads must select at least two workloads for multi-hypervisor smoke",
        ));
    }
    let unique = workloads.iter().collect::<BTreeSet<_>>();
    if unique.len() != workloads.len() {
        return Err(String::from(
            "--workloads must not contain duplicate workloads",
        ));
    }
    Ok(workloads)
}

pub fn campaign_plan(
    out: &Path,
    workloads: &[String],
    command_plans: &[Value],
) -> Result<Value, String> {
    if workloads.len() != command_plans.len() {
        return Err(String::from(
            "workload and command-plan cardinality differs",
        ));
    }
    let entries = workloads
        .iter()
        .zip(command_plans)
        .enumerate()
        .map(|(index, (workload, command_plan))| {
            let number = index + 1;
            json!({
                "queue_entry_id": format!("kvm-mhq-{number:04}"),
                "run_id": format!("kvm-mh-run-{number:04}"),
                "workload": workload,
                "command_plan": command_plan,
                "receipt_path": out.join("run-receipts").join(format!("{number:02}-{workload}-replay-readiness.json")),
            })
        })
        .collect::<Vec<_>>();
    let hypervisors = workloads
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let number = index + 1;
            json!({
                "hypervisor_worker_id": format!("local-kvm-hv-{number}"),
                "node_id": format!("local-kvm-node-{number}"),
                "resource_budget": {"vcpus": VCPUS_PER_WORKER, "memory_mib": MEMORY_MIB_PER_WORKER},
                "artifact_root": out.join("hypervisors").join(format!("local-kvm-hv-{number}")),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "schema_version": 1,
        "campaign_id": "local-kvm-smoke-0001",
        "max_hypervisors": workloads.len().min(MAX_HYPERVISORS),
        "state_path": out.join("campaign-state.json"),
        "artifact_index_path": out.join("artifact-index.json"),
        "follow_up_policy": {"enabled": false, "reproduce": false, "minimize": false},
        "hypervisors": hypervisors,
        "queue": {"entries": entries},
        "operator_decisions": [out.join("operator-decision-receipt.json")],
    }))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::{campaign_plan, parse_workloads};

    #[test]
    fn distinct_workloads_produce_bounded_plan() {
        let workloads = parse_workloads("raft,rust-workload").expect("workloads");
        let plan = campaign_plan(Path::new("/tmp/out"), &workloads, &[json!({}), json!({})])
            .expect("plan");
        assert_eq!(plan["max_hypervisors"], 2);
        assert_eq!(plan["queue"]["entries"][1]["workload"], "rust-workload");
    }

    #[test]
    fn duplicates_short_sets_and_cardinality_fail() {
        assert!(parse_workloads("raft").is_err());
        assert!(parse_workloads("raft,raft").is_err());
        assert!(campaign_plan(Path::new("/tmp/out"), &[String::from("raft")], &[]).is_err());
    }
}
