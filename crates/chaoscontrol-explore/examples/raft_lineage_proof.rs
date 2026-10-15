//! Bounded KVM snapshot-lineage proof for the admitted Raft Rust guest.
//!
//! Owner: chaoscontrol-vmm. Run from the repository root:
//! nix develop -c cargo run -p chaoscontrol-explore --example raft_lineage_proof -- \
//!   --kernel /absolute/vmlinux --initrd /absolute/initrd --output /new/proof-directory
//!
//! This emits Rust-owned runtime facts for a separately Nickel-validated receipt.
//! It does not claim release readiness or manufacture a snapshot-backed bug verdict.
use chaoscontrol_explore::assertion_report::strict_projection;
use chaoscontrol_explore::snapshot_store::{FileSnapshotStore, SnapshotStore};
use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::oracle::{OracleReport, Verdict};
use chaoscontrol_fault::outcomes::{
    validate_fault_outcome_ledger, FaultObservationEffect, FaultOutcomeLedger, FaultStageKind,
    NANOSECONDS_PER_SIMULATION_TICK,
};
use chaoscontrol_fault::schedule::FaultScheduleBuilder;
use chaoscontrol_protocol::identity::AssertionKind;
use chaoscontrol_vmm::controller::{
    SimulationConfig, SimulationController, SimulationSnapshot, VmStatus,
};
use chaoscontrol_vmm::dlog::{DlogReader, DlogTag, RECORD_SIZE};
use chaoscontrol_vmm::vm::VmConfig;
use clap::Parser;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

// Values selected from contracts/evidence/examples/raft-run-config.ncl.
// Every level/continuation uses the admitted 500-tick branch bound. Depth four
// comes from the active repair-incremental-snapshot-lineage validation requirement.
const PROFILE: &str = "raft-lineage-proof-v1";
const VMS: usize = 3;
const MEMORY_MIB: usize = 128;
const SEED: u64 = 42;
const QUANTUM: u64 = 100;
const BOOTSTRAP_TICKS: u64 = 10_000;
const BRANCH_TICKS: u64 = 500;
const PARENT_DEPTH: u32 = 4;
// A reversible stall lasting one admitted quantum, not guest-memory corruption.
const STALL_TICKS: u64 = QUANTUM;
const MAX_DLOG_RECORDS: u64 = BRANCH_TICKS * (QUANTUM * 8 + 4);

#[derive(Parser)]
struct Args {
    #[arg(long)]
    kernel: PathBuf,
    #[arg(long)]
    initrd: PathBuf,
    /// Must not already exist: old successful evidence can never survive a failed attempt.
    #[arg(long)]
    output: PathBuf,
}

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message).into())
    }
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn file_binding(path: &Path) -> Result<Value> {
    let mut input = File::open(path)?;
    let mut sha = Sha256::new();
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buf)?;
        if read == 0 {
            break;
        }
        sha.update(&buf[..read]);
    }
    Ok(json!({"path": path, "sha256": format!("sha256:{:x}", sha.finalize())}))
}

fn memory_identities(snapshot: &SimulationSnapshot) -> Vec<String> {
    use chaoscontrol_vmm::snapshot::SnapshotMemory;
    snapshot
        .vm_snapshots
        .iter()
        .map(|(vm, _)| {
            let mut hasher = blake3::Hasher::new();
            match &vm.memory {
                SnapshotMemory::Full(bytes) => {
                    hasher.update(bytes);
                }
                SnapshotMemory::Overlay { base, dirty_pages } => {
                    let mut offset = 0;
                    for (&page, bytes) in dirty_pages {
                        let start = page * chaoscontrol_vmm::snapshot::PAGE_SIZE;
                        hasher.update(&base[offset..start]);
                        hasher.update(bytes.as_ref());
                        offset = start + bytes.len();
                    }
                    hasher.update(&base[offset..]);
                }
            }
            format!("blake3:{}", hasher.finalize().to_hex())
        })
        .collect()
}

fn reports(controller: &SimulationController) -> Vec<OracleReport> {
    // Preserve per-VM records as well as the merged report: aggregation must not
    // hide a true/false outcome moving between VMs.
    let mut reports = vec![controller.report()];
    for vm in 0..VMS {
        reports.push(
            controller
                .vm(vm)
                .fault_engine()
                .oracle()
                .finalized_report_projection(),
        );
    }
    reports
}

fn assertion_outcomes(reports: &[OracleReport]) -> Result<Value> {
    let mut all = Vec::new();
    for report in reports {
        let projection = strict_projection(report)
            .map_err(|error| io::Error::other(format!("strict assertion report: {error:?}")))?;
        require(
            !projection.details.is_empty(),
            "empty admitted assertion catalog",
        )?;
        let mut rows = Vec::new();
        for (fingerprint, record) in &report.structured_assertions {
            let mut outcome = record.clone();
            // The ordinal is diagnostic, not execution identity; retain whether
            // there was a failure, its exact details, counters, and verdict.
            outcome.first_failure_run = outcome.first_failure_run.map(|_| 0);
            rows.push(json!({"fingerprint": fingerprint, "record": outcome,
                "verdict": record.verdict()}));
        }
        all.push(json!({"catalog_status": report.catalog_status,
            "collision_safe_evidence": report.collision_safe_evidence,
            "identity_conflicts": report.identity_conflicts,
            "assertions": rows}));
    }
    Ok(Value::Array(all))
}

fn safety_holds(reports: &[OracleReport]) -> bool {
    reports.iter().all(|report| {
        report.structured_assertions.values().all(|record| {
            !matches!(
                record.kind,
                AssertionKind::Always | AssertionKind::Unreachable
            ) || record.verdict() != Verdict::Failed
        })
    })
}

fn validate_fault(ledger: &FaultOutcomeLedger) -> Result<()> {
    validate_fault_outcome_ledger(ledger)?;
    require(
        ledger.attempts.len() == 1,
        "expected exactly one attributed fault attempt",
    )?;
    let state = ledger
        .attempts
        .values()
        .next()
        .ok_or_else(|| io::Error::other("missing fault"))?;
    require(
        state.attempt.fault
            == Fault::CpuStall {
                target: 0,
                vcpu: 0,
                duration_ticks: STALL_TICKS,
            },
        "ledger attempt differs from selected CPU stall",
    )?;
    require(
        ledger.counters.selected == 1
            && ledger.counters.applied == 1
            && ledger.counters.rejected == 0
            && ledger.counters.application_failed == 0,
        "CPU stall was not successfully applied",
    )?;
    require(
        ledger.events.iter().any(|event| {
            matches!(&event.kind,
        FaultStageKind::Observed { observation }
        if observation.attempt_id == state.attempt.id
            && observation.effect == FaultObservationEffect::CpuStallActivated)
        }),
        "CPU stall has no attributed Observed effect",
    )
}

fn ledger_json(ledger: &FaultOutcomeLedger) -> Value {
    // Same wire shape as chaoscontrol-replay recording::recorded_fault_ledger:
    // JSON object keys cannot represent the typed binary attempt IDs.
    json!({"attempts": ledger.attempts.values().collect::<Vec<_>>(),
        "events": ledger.events, "counters": ledger.counters})
}

fn trace_identity(
    path: &Path,
    first_exit: u64,
    final_exit: u64,
    parent_tick: u64,
) -> Result<Value> {
    let bytes = fs::metadata(path)?.len();
    require(
        bytes > 0 && bytes % RECORD_SIZE as u64 == 0,
        "disabled or truncated dlog",
    )?;
    let count = bytes / RECORD_SIZE as u64;
    require(
        count <= MAX_DLOG_RECORDS,
        "dlog exceeds admitted record bound",
    )?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"chaoscontrol.raft-lineage.dlog-strict-fields.v1");
    let mut previous_exit = first_exit;
    let mut next_tick = parent_tick;
    let mut execution_records = 0_u64;
    let mut seen = 0_u64;
    for record in DlogReader::open(path)? {
        let record = record?;
        require(
            record.seq == seen,
            "dlog sequence missing, overflowed, or reordered",
        )?;
        seen += 1;
        let tag = record
            .tag()
            .ok_or_else(|| io::Error::other("unknown dlog tag"))?;
        // Logging starts AFTER restore and ends BEFORE the next capture. No
        // bootstrap/capture/restore records are in the comparison interval.
        require(
            !matches!(tag, DlogTag::SnapshotTaken | DlogTag::SnapshotRestored),
            "control record inside replay execution interval",
        )?;
        require(
            !matches!(tag, DlogTag::Shutdown | DlogTag::InternalError),
            "guest shutdown or internal execution error inside replay",
        )?;
        require(
            record.exit_count >= previous_exit && record.exit_count <= final_exit,
            "dlog exit counter regressed or exceeded replay interval",
        )?;
        require(
            record.exit_count - previous_exit <= 1,
            "dlog has a missing execution exit",
        )?;
        if record.exit_count > previous_exit {
            execution_records += 1;
            previous_exit = record.exit_count;
        }
        if tag == DlogTag::TickMarker {
            require(
                u64::from_le_bytes(record.data) == next_tick,
                "missing or reordered replay tick",
            )?;
            next_tick += 1;
        }
        // Exactly the existing DlogRecord::determinism_eq(strict=true) fields.
        // Excludes only writer sequence/padding; no payload or differing data
        // is discarded. This is not CanonicalTrace::identity (see proof gaps).
        hasher.update(&record.virtual_tsc.to_le_bytes());
        hasher.update(&record.exit_count.to_le_bytes());
        hasher.update(&record.rip.to_le_bytes());
        hasher.update(&[record.tag, record.vcpu]);
        hasher.update(&record.port_or_addr_lo.to_le_bytes());
        hasher.update(&record.port_or_addr_hi.to_le_bytes());
        hasher.update(&record.data);
        hasher.update(&record.extra);
    }
    require(
        seen == count
            && previous_exit == final_exit
            && final_exit > first_exit
            && execution_records == final_exit - first_exit,
        "incomplete or empty execution interval",
    )?;
    require(
        next_tick == parent_tick + BRANCH_TICKS,
        "incomplete replay tick interval",
    )?;
    Ok(
        json!({"digest": format!("blake3:{}", hasher.finalize().to_hex()),
        "records": count, "execution_records": execution_records,
        "first_exit": first_exit, "final_exit": final_exit}),
    )
}

fn continuation(
    controller: &mut SimulationController,
    parent: &SimulationSnapshot,
    output: &Path,
    name: &str,
) -> Result<Value> {
    let directory = output.join(name);
    fs::create_dir(&directory)?;
    require(
        controller
            .vm_slot(0)
            .is_some_and(|slot| slot.status == VmStatus::Running),
        "CPU stall target is not live",
    )?;
    let baseline = reports(controller);
    let initial_hits: u64 = baseline[0]
        .structured_assertions
        .values()
        .map(|r| r.hit_count)
        .sum();
    for vm in 0..VMS {
        controller
            .vm_mut(vm)
            .set_dlog_path(&directory.join(format!("vm_{vm}.dlog")))?;
    }
    let fault = Fault::CpuStall {
        target: 0,
        vcpu: 0,
        duration_ticks: STALL_TICKS,
    };
    let schedule = FaultScheduleBuilder::new()
        .at_ns_labeled(
            (parent.tick + 1) * NANOSECONDS_PER_SIMULATION_TICK,
            fault,
            PROFILE,
        )
        .build();
    // Restored guests have already completed setup; preserve that admission state.
    controller.begin_counterfactual_fault_run(schedule)?;
    let mut rounds = Vec::new();
    for _ in 0..BRANCH_TICKS {
        let round = controller.step_round()?;
        rounds.push(json!({"tick": round.tick, "running": round.vms_running,
            "halted": round.vms_halted, "messages_delivered": round.messages_delivered,
            "schedule_traces": round.schedule_traces}));
    }
    let observed = reports(controller);
    write_json(&directory.join("oracle-reports.json"), &observed)?;
    write_json(
        &directory.join("fault-ledger.json"),
        &ledger_json(controller.fault_outcomes()),
    )?;
    write_json(&directory.join("rounds.json"), &rounds)?;
    validate_fault(controller.fault_outcomes())?;
    let outcomes = assertion_outcomes(&observed)?;
    let final_hits: u64 = observed[0]
        .structured_assertions
        .values()
        .map(|r| r.hit_count)
        .sum();
    require(
        final_hits > initial_hits,
        "replay interval exercised no admitted SDK assertions",
    )?;
    let mut traces = Vec::new();
    let mut bindings = Vec::new();
    for vm in 0..VMS {
        controller.vm_mut(vm).flush_dlog();
        let path = directory.join(format!("vm_{vm}.dlog"));
        traces.push(trace_identity(
            &path,
            parent.vm_snapshots[vm].0.exit_count,
            controller.vm(vm).exit_count(),
            parent.tick,
        )?);
        bindings.push(file_binding(&path)?);
    }
    for artifact in ["oracle-reports.json", "fault-ledger.json", "rounds.json"] {
        bindings.push(file_binding(&directory.join(artifact))?);
    }
    let proof = json!({"name": name, "parent_tick": parent.tick,
        "end_tick": controller.tick(), "execution_traces": traces,
        "all_admitted_assertion_outcomes": outcomes,
        "safety_invariants_hold": safety_holds(&observed),
        "assertion_hits_in_interval": final_hits - initial_hits,
        "assertion_coverage": strict_projection(&observed[0]).map_err(|e| io::Error::other(format!("{e:?}")))?.stats,
        "fault_observed": true, "fault_ledger": ledger_json(controller.fault_outcomes()),
        "rounds": rounds, "artifacts": bindings});
    write_json(&directory.join("runtime-proof.json"), &proof)?;
    Ok(proof)
}

fn run(args: Args) -> Result<()> {
    let kernel = fs::canonicalize(args.kernel)?;
    let initrd = fs::canonicalize(args.initrd)?;
    fs::create_dir(&args.output)?;
    let output = fs::canonicalize(&args.output)?;
    let profile = json!({"profile_id": PROFILE,
        "derived_from": "contracts/evidence/examples/raft-run-config.ncl",
        "intentional_difference": "raft_bug=none; no planted probe; admitted hide-TSC clock profile",
        "seed": SEED, "num_vms": VMS, "num_vcpus": 1,
        "memory_mib": MEMORY_MIB, "quantum": QUANTUM,
        "bootstrap_budget": BOOTSTRAP_TICKS, "ticks_per_branch": BRANCH_TICKS,
        "parent_depth": PARENT_DEPTH, "stall_ticks": STALL_TICKS,
        "guest_cmdline": "raft_bug=none", "hide_tsc": true,
        "max_dlog_records_per_vm": MAX_DLOG_RECORDS,
        "kernel": file_binding(&kernel)?, "initrd": file_binding(&initrd)?});
    write_json(&output.join("profile.json"), &profile)?;
    let config = SimulationConfig {
        num_vms: VMS,
        seed: SEED,
        quantum: QUANTUM,
        kernel_path: kernel.to_string_lossy().into_owned(),
        initrd_path: Some(initrd.to_string_lossy().into_owned()),
        bootstrap_budget: Some(BOOTSTRAP_TICKS),
        vm_config: VmConfig {
            memory_size: MEMORY_MIB * 1024 * 1024,
            num_vcpus: 1,
            extra_cmdline: Some("raft_bug=none".to_owned()),
            ..VmConfig::single_vcpu(SEED, true)
        },
        ..SimulationConfig::default()
    };
    let store = FileSnapshotStore::new(&output);
    let mut controller = SimulationController::new(config.clone())?;
    controller.run_until_setup_complete(BOOTSTRAP_TICKS)?;
    require(
        (0..VMS).all(|vm| controller.vm(vm).fault_engine().is_setup_complete()),
        "bootstrap bound exhausted without every guest setup_complete",
    )?;
    assertion_outcomes(&reports(&controller))?;
    let base = controller.snapshot_all()?;
    let bases = SimulationController::extract_memory_bases(&base);
    // Establish the captured image as the exact base and drain bootstrap dirties.
    controller.restore_all(&base)?;
    controller.set_memory_bases(bases.clone());
    let mut snapshots = Vec::with_capacity(PARENT_DEPTH as usize + 1);
    snapshots.push(json!({"depth": 0, "tick": base.tick, "persisted": false}));
    let mut parent = base;
    for depth in 1..=PARENT_DEPTH {
        for _ in 0..BRANCH_TICKS {
            controller.step_round()?;
        }
        let full_reference = memory_identities(&controller.snapshot_all()?);
        let (snapshot, dirty_pages) = controller.snapshot_all_incremental()?;
        snapshot
            .validate_assertion_identity(VMS)
            .map_err(io::Error::other)?;
        let incremental_memory = memory_identities(&snapshot);
        require(
            incremental_memory == full_reference,
            "incremental lineage differs from full reference memory",
        )?;
        snapshots.push(json!({"depth": depth, "persisted": depth == PARENT_DEPTH,
            "tick": snapshot.tick, "dirty_pages": dirty_pages,
            "memory_identities": incremental_memory, "full_reference_memory_equal": true}));
        controller.restore_all_incremental(&snapshot)?;
        parent = snapshot;
    }
    // Only the replay parent needs durable encoding. Earlier depths retain their
    // independent full-memory comparisons without repeatedly encoding the base.
    let expected_parent_memory = memory_identities(&parent);
    let parent_ref = store.put_snapshot(&parent, PARENT_DEPTH)?;
    let retained = store.get_snapshot_artifact(&parent_ref)?;
    require(
        retained.replay_parent_depth == PARENT_DEPTH,
        "stored snapshot depth mismatch",
    )?;
    require(
        memory_identities(&retained.snapshot) == expected_parent_memory,
        "persisted replay parent differs from captured memory",
    )?;
    let parent = retained.snapshot;
    parent
        .validate_assertion_identity(VMS)
        .map_err(io::Error::other)?;
    write_json(&output.join("snapshots.json"), &snapshots)?;
    controller.restore_all_incremental(&parent)?;
    let long_lived = continuation(&mut controller, &parent, &output, "long-lived-incremental")?;
    drop(controller);
    let mut fresh = SimulationController::new(config)?;
    fresh.restore_all(&parent)?;
    fresh.set_memory_bases(bases);
    let (recaptured, _) = fresh.snapshot_all_incremental()?;
    // Recapture verifies memory equivalence; it is not another replay artifact.
    let recapture_equal = expected_parent_memory == memory_identities(&recaptured);
    let fresh_proof = continuation(&mut fresh, &parent, &output, "fresh-full")?;
    let equal = long_lived["execution_traces"] == fresh_proof["execution_traces"]
        && long_lived["all_admitted_assertion_outcomes"]
            == fresh_proof["all_admitted_assertion_outcomes"]
        && long_lived["rounds"] == fresh_proof["rounds"];
    let safety = long_lived["safety_invariants_hold"] == true
        && fresh_proof["safety_invariants_hold"] == true;
    let proof = json!({"schema": "chaoscontrol.raft-lineage-runtime-proof.v1",
        "profile": file_binding(&output.join("profile.json"))?,
        "snapshots": snapshots, "replay_parent": parent_ref,
        "full_restore_incremental_recapture": {"memory_equal": recapture_equal},
        "replays": [long_lived, fresh_proof], "equal": equal,
        "safety_invariants_hold": safety, "passed": equal && safety && recapture_equal,
        "bug_reports": [], "probe": {"intentional": false, "enabled": false},
        "raw_artifact_policy": "debug-only-excluded-from-git",
        "gaps": ["Bounded local Raft execution only; no release-readiness or universal-determinism claim",
            "Canonical simulation RoundTrace is discarded by controller step_round; execution identity uses strict DlogRecord determinism fields, not CanonicalTrace::identity",
            "Dlog hot-path RIP is zero where the VMM does not fetch RIP; this is not a full register or instruction trace",
            "Every admitted assertion outcome is compared, including unexercised and bounded liveness failures; not every catalog site is claimed exercised",
            "No selected bug or planted probe; this is lineage/continuation evidence, not snapshot_backed_reproduced bug evidence",
            "Source revision, dirty-source identity, exact command, and strict readiness gates belong to the enclosing Nickel-validated receipt"]});
    write_json(&output.join("runtime-proof.json"), &proof)?;
    require(
        equal,
        "replay execution traces or ALL assertion outcomes differ",
    )?;
    require(
        recapture_equal,
        "full restore lost overlay pages in immediate incremental capture",
    )?;
    require(safety, "Raft safety invariant failed")?;
    println!("{}", output.join("runtime-proof.json").display());
    Ok(())
}

fn main() -> Result<()> {
    run(Args::parse())
}
