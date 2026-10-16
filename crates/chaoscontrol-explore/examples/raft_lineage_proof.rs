//! Bounded KVM snapshot-lineage proof for the admitted Raft Rust guest.
//!
//! Owner: chaoscontrol-vmm. Run from the repository root:
//! nix develop -c cargo run -p chaoscontrol-explore --example raft_lineage_proof -- \
//!   --kernel /absolute/vmlinux --initrd /absolute/initrd --output /new/proof-directory
//!
//! This emits Rust-owned runtime facts for a separately Nickel-validated receipt.
//! It does not claim release readiness or manufacture a snapshot-backed bug verdict.
//! A separate CLI/JMP-self probe must expire the VMM's own POSIX watchdog.
//! SI_TIMER delivery, HostWatchdogTimeout, permanent poison rejection, and
//! production RecoverySnapshot replacement are required, not inferred. Probe
//! state is discarded before restoring the retained Raft parent; it is never a
//! guest bug or admitted fault. Runtime-proof v2 compares all three continuations.
use chaoscontrol_explore::assertion_report::strict_projection;
use chaoscontrol_explore::branch_recovery::{
    decide_after_watchdog, host_watchdog_vcpu, RecoverySnapshot, WatchdogDecision,
    MAX_BRANCH_WATCHDOG_ATTEMPTS,
};
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
use chaoscontrol_vmm::snapshot::{SnapshotMemory, VcpuSnapshot};
use chaoscontrol_vmm::vm::{VmConfig, VmError};
use clap::Parser;
use serde_json::{json, Value};
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

// Values selected from contracts/evidence/examples/raft-run-config.ncl.
// Every level/continuation uses the admitted 500-tick branch bound. Depth four
// comes from the active repair-incremental-snapshot-lineage validation requirement.
const PROFILE: &str = "raft-lineage-watchdog-proof-v2";
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
// Matches the existing single-vCPU VMM watchdog, not a replacement timer.
const WATCHDOG_INTERVAL: Duration = Duration::from_millis(100);
const WATCHDOG_PROBE_ROUNDS: u32 = 1;
const WATCHDOG_PROBE_VM: usize = 0;
const WATCHDOG_PROBE_VCPU: usize = 0;
// Below the boot page tables at PML4_START; the probe uses the boot identity map.
const WATCHDOG_PROBE_ADDRESS: u64 = 0x1000;
// cli; jmp to self. No I/O, halt, SDK call, sleep, or manufactured timeout.
const WATCHDOG_PROBE_CODE: [u8; 3] = [0xfa, 0xeb, 0xfe];
static WATCHDOG_TIMER_SIGNALS: AtomicUsize = AtomicUsize::new(0);

extern "C" fn observe_watchdog_signal(
    signal: libc::c_int,
    info: *mut libc::siginfo_t,
    _context: *mut libc::c_void,
) {
    // SAFETY: SA_SIGINFO supplies a valid siginfo_t during this handler.
    if signal == libc::SIGALRM && !info.is_null() && unsafe { (*info).si_code } == libc::SI_TIMER {
        // AtomicUsize is lock-free on the supported x86_64 host. No allocation,
        // logging, timer arming, or other signal-unsafe work occurs here.
        WATCHDOG_TIMER_SIGNALS.fetch_add(1, Ordering::Relaxed);
    }
}

struct WatchdogSignalObserver(libc::sigaction);

impl WatchdogSignalObserver {
    fn install() -> Result<Self> {
        WATCHDOG_TIMER_SIGNALS.store(0, Ordering::Relaxed);
        // The VMM has already installed its no-op handler via init_thread_timers.
        // Observe the same real POSIX timer, preserving its interrupt semantics.
        // SAFETY: both structures and the handler pointer are valid for sigaction.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            let mut previous: libc::sigaction = std::mem::zeroed();
            action.sa_flags = libc::SA_SIGINFO;
            action.sa_sigaction = observe_watchdog_signal as *const () as usize;
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(libc::SIGALRM, &action, &mut previous) != 0 {
                return Err(io::Error::last_os_error().into());
            }
            Ok(Self(previous))
        }
    }
}

impl Drop for WatchdogSignalObserver {
    fn drop(&mut self) {
        // SAFETY: restore the disposition captured by the successful installation.
        unsafe {
            libc::sigaction(libc::SIGALRM, &self.0, std::ptr::null_mut());
        }
    }
}

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
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(json!({"path": path, "blake3": format!("blake3:{}", hasher.finalize().to_hex())}))
}

fn memory_identities(snapshot: &SimulationSnapshot) -> Vec<String> {
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
) -> Result<(Value, SimulationSnapshot)> {
    let restored_memory = memory_identities(&controller.snapshot_all()?);
    require(
        restored_memory == memory_identities(parent),
        "continuation did not start from exact retained parent memory",
    )?;
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
    let final_snapshot = controller.snapshot_all()?;
    let final_memory = memory_identities(&final_snapshot);
    let proof = json!({"name": name, "parent_tick": parent.tick,
        "end_tick": controller.tick(), "execution_traces": traces,
        "initial_memory_identities": restored_memory, "final_memory_identities": final_memory,
        "all_admitted_assertion_outcomes": outcomes,
        "safety_invariants_hold": safety_holds(&observed),
        "assertion_hits_in_interval": final_hits - initial_hits,
        "assertion_coverage": strict_projection(&observed[0]).map_err(|e| io::Error::other(format!("{e:?}")))?.stats,
        "fault_observed": true, "fault_ledger": ledger_json(controller.fault_outcomes()),
        "rounds": rounds, "artifacts": bindings});
    write_json(&directory.join("runtime-proof.json"), &proof)?;
    Ok((proof, final_snapshot))
}

fn continuations_equal(left: &Value, right: &Value) -> bool {
    [
        "initial_memory_identities",
        "final_memory_identities",
        "execution_traces",
        "all_admitted_assertion_outcomes",
        "fault_ledger",
        "rounds",
    ]
    .iter()
    .all(|field| left[*field] == right[*field])
}

fn watchdog_replacement(
    mut controller: SimulationController,
    mut boot_cpu: VcpuSnapshot,
    parent: &SimulationSnapshot,
    recovery: &RecoverySnapshot,
    config: SimulationConfig,
    expected_base_memory: &[String],
    output: &Path,
) -> Result<(SimulationController, Value)> {
    let directory = output.join("watchdog-interrupted");
    fs::create_dir(&directory)?;
    // Close the successful continuation logs before any restore/probe effects.
    for vm in 0..VMS {
        controller
            .vm_mut(vm)
            .set_dlog_path(&directory.join(format!("vm_{vm}.dlog")))?;
    }
    let mut probe = parent.clone();
    // Reuse the VMM's exact pre-boot CPU setup, not a hand-built CPU profile.
    // Retain the admitted parent oracle/setup state so the real watchdog is armed.
    boot_cpu.regs.rip = WATCHDOG_PROBE_ADDRESS;
    probe.vm_snapshots[WATCHDOG_PROBE_VM].0.vcpu_snapshots[WATCHDOG_PROBE_VCPU] = boot_cpu;
    controller.restore_all_incremental(&probe)?;
    controller.reset_vm_statuses();
    controller
        .vm(WATCHDOG_PROBE_VM)
        .memory()
        .setup_page_tables()?;
    controller
        .vm(WATCHDOG_PROBE_VM)
        .write_guest_memory(WATCHDOG_PROBE_ADDRESS, &WATCHDOG_PROBE_CODE)?;
    require(
        controller
            .vm(WATCHDOG_PROBE_VM)
            .fault_engine()
            .is_setup_complete(),
        "watchdog probe lacks admitted setup_complete state",
    )?;
    controller.init_thread_timers()?;
    let observer = WatchdogSignalObserver::install()?;
    let started = Instant::now();
    // Exactly one bounded round: the CLI/JMP loop cannot produce a normal exit.
    let result = controller.step_round();
    let elapsed = started.elapsed();
    let timer_signals = WATCHDOG_TIMER_SIGNALS.load(Ordering::Relaxed);
    drop(observer);
    let error = match result {
        Err(error) => error,
        Ok(_) => return Err(io::Error::other("watchdog probe completed without a timeout").into()),
    };
    let observed = host_watchdog_vcpu(&error) == Some(WATCHDOG_PROBE_VCPU)
        && timer_signals == 1
        && elapsed >= WATCHDOG_INTERVAL;
    let mut event = json!({"schema": "chaoscontrol.watchdog-recovery-event.v1",
        "classification": "operational-host-watchdog-timeout", "guest_bug": false,
        "probe": {"intentional": true, "admitted_to_guest_evidence": false,
            "vm": WATCHDOG_PROBE_VM, "vcpu": WATCHDOG_PROBE_VCPU,
            "guest_physical_address": WATCHDOG_PROBE_ADDRESS,
            "guest_virtual_address": WATCHDOG_PROBE_ADDRESS, "bytes": WATCHDOG_PROBE_CODE,
            "cpu_state": "VMM pre-boot CPU; RIP redirected to probe",
            "page_tables": "GuestMemoryManager::setup_page_tables boot identity map"},
        "configured_watchdog_milliseconds": WATCHDOG_INTERVAL.as_millis(),
        "max_probe_rounds": WATCHDOG_PROBE_ROUNDS,
        "max_branch_watchdog_attempts": MAX_BRANCH_WATCHDOG_ATTEMPTS,
        "attempts_made": 1, "timer_signal_code": "SI_TIMER",
        "timer_signals_observed": timer_signals, "elapsed_nanoseconds": elapsed.as_nanos(),
        "error": error.to_string(), "watchdog_observed": observed,
        "replacement_created": false, "poisoned_controller_dropped": false,
        "passed": false});
    let event_path = output.join("watchdog-event.json");
    write_json(&event_path, &event)?;
    require(
        observed,
        "missing actual POSIX watchdog signal or HostWatchdogTimeout",
    )?;

    let mut rejections = Vec::new();
    for (operation, result) in [
        ("step_round", controller.step_round().map(|_| ())),
        ("run", controller.run(0).map(|_| ())),
        ("snapshot_all", controller.snapshot_all().map(|_| ())),
        (
            "snapshot_all_incremental",
            controller.snapshot_all_incremental().map(|_| ()),
        ),
        ("restore_all", controller.restore_all(parent)),
        (
            "restore_all_incremental",
            controller.restore_all_incremental(parent),
        ),
    ] {
        let rejected = matches!(&result, Err(VmError::ControllerRoundPoisoned { .. }));
        rejections.push(json!({"operation": operation, "rejected": rejected,
            "error": result.err().map(|error| error.to_string())}));
    }
    let poison_enforced = rejections.iter().all(|row| row["rejected"] == true);
    event["poison_rejections"] = json!(rejections);
    event["poison_enforced"] = json!(poison_enforced);
    write_json(&event_path, &event)?;
    require(
        poison_enforced,
        "timed-out controller did not permanently reject all operations",
    )?;
    require(
        decide_after_watchdog(1, true) == WatchdogDecision::Retry,
        "production watchdog policy refused bounded parent-backed retry",
    )?;
    drop(controller);
    event["poisoned_controller_dropped"] = json!(true);
    // This is the same replacement helper used by sequential and worker branches.
    let mut replacement = recovery.rebuild_controller(config)?;
    replacement.init_thread_timers()?;
    event["replacement_created"] = json!(true);
    event["replacement_api"] = json!("RecoverySnapshot::rebuild_controller");
    let base_memory = memory_identities(&replacement.snapshot_all()?);
    let base_equal = base_memory == expected_base_memory;
    event["bootstrap_memory_identities"] = json!(base_memory);
    event["bootstrap_base_memory_equal"] = json!(base_equal);
    write_json(&event_path, &event)?;
    require(
        base_equal,
        "replacement memory does not equal retained bootstrap base",
    )?;
    replacement.restore_all_incremental(parent)?;
    replacement.reset_vm_statuses();
    let parent_memory = memory_identities(parent);
    let restored_memory = memory_identities(&replacement.snapshot_all()?);
    let (recaptured, dirty_pages) = replacement.snapshot_all_incremental()?;
    let recaptured_memory = memory_identities(&recaptured);
    let parent_equal = restored_memory == parent_memory;
    let recapture_equal = recaptured_memory == parent_memory;
    event["retained_parent_tick"] = json!(parent.tick);
    event["retained_parent_memory_identities"] = json!(parent_memory);
    event["restored_parent_memory_identities"] = json!(restored_memory);
    event["restored_parent_memory_equal"] = json!(parent_equal);
    event["immediate_incremental_recapture"] = json!({"memory_identities": recaptured_memory,
        "memory_equal": recapture_equal, "dirty_pages": dirty_pages});
    event["passed"] =
        json!(observed && poison_enforced && base_equal && parent_equal && recapture_equal);
    write_json(&event_path, &event)?;
    require(
        parent_equal,
        "replacement incremental restore differs from retained parent",
    )?;
    require(
        recapture_equal,
        "replacement immediately recaptured incomplete incremental memory",
    )?;
    Ok((replacement, event))
}

fn run(args: Args) -> Result<()> {
    let kernel = fs::canonicalize(args.kernel)?;
    let initrd = fs::canonicalize(args.initrd)?;
    fs::create_dir(&args.output)?;
    let output = fs::canonicalize(&args.output)?;
    let profile = json!({"profile_id": PROFILE,
        "derived_from": "contracts/evidence/examples/raft-run-config.ncl",
        "intentional_difference": "raft_bug=none; no planted guest bug; isolated watchdog-only CLI/JMP probe; admitted hide-TSC clock profile",
        "seed": SEED, "num_vms": VMS, "num_vcpus": 1,
        "memory_mib": MEMORY_MIB, "quantum": QUANTUM,
        "bootstrap_budget": BOOTSTRAP_TICKS, "ticks_per_branch": BRANCH_TICKS,
        "parent_depth": PARENT_DEPTH, "stall_ticks": STALL_TICKS,
        "guest_cmdline": "raft_bug=none", "hide_tsc": true,
        "max_dlog_records_per_vm": MAX_DLOG_RECORDS,
        "watchdog": {"interval_milliseconds": WATCHDOG_INTERVAL.as_millis(),
            "max_probe_rounds": WATCHDOG_PROBE_ROUNDS,
            "production_max_attempts": MAX_BRANCH_WATCHDOG_ATTEMPTS,
            "probe_address": WATCHDOG_PROBE_ADDRESS, "probe_bytes": WATCHDOG_PROBE_CODE},
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
    // Existing snapshot API supplies exact boot architecture state. Retain only
    // its single CPU; the temporary pre-boot memory capture is dropped here.
    let boot_cpu = controller
        .vm_mut(WATCHDOG_PROBE_VM)
        .snapshot()?
        .vcpu_snapshots
        .remove(0);
    controller.run_until_setup_complete(BOOTSTRAP_TICKS)?;
    require(
        (0..VMS).all(|vm| controller.vm(vm).fault_engine().is_setup_complete()),
        "bootstrap bound exhausted without every guest setup_complete",
    )?;
    assertion_outcomes(&reports(&controller))?;
    let base = controller.snapshot_all()?;
    let bases = SimulationController::extract_memory_bases(&base);
    let expected_base_memory = memory_identities(&base);
    let recovery = RecoverySnapshot::new(&base, bases.clone());
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
    let (long_lived, long_lived_final) =
        continuation(&mut controller, &parent, &output, "long-lived-incremental")?;
    let (mut replacement, watchdog) = watchdog_replacement(
        controller,
        boot_cpu,
        &parent,
        &recovery,
        config.clone(),
        &expected_base_memory,
        &output,
    )?;
    let (recovered_proof, recovered_final) = continuation(
        &mut replacement,
        &parent,
        &output,
        "watchdog-replacement-incremental",
    )?;
    drop(replacement);
    let mut fresh = SimulationController::new(config)?;
    fresh.restore_all(&parent)?;
    fresh.set_memory_bases(bases);
    let (recaptured, _) = fresh.snapshot_all_incremental()?;
    // Recapture verifies memory equivalence; it is not another replay artifact.
    let recapture_equal = expected_parent_memory == memory_identities(&recaptured);
    let (fresh_proof, fresh_final) = continuation(&mut fresh, &parent, &output, "fresh-full")?;
    let fresh_equal = continuations_equal(&long_lived, &fresh_proof);
    let recovery_equal = continuations_equal(&long_lived, &recovered_proof);
    let equal = fresh_equal && recovery_equal;
    let mut mismatch_memory = Vec::new();
    if !equal {
        for (name, snapshot) in [
            ("long-lived-incremental", &long_lived_final),
            ("watchdog-replacement-incremental", &recovered_final),
            ("fresh-full", &fresh_final),
        ] {
            for (vm, (snapshot, _)) in snapshot.vm_snapshots.iter().enumerate() {
                let path = output.join(name).join(format!("vm_{vm}.memory.bin"));
                let mut file = File::create(&path)?;
                match &snapshot.memory {
                    SnapshotMemory::Full(bytes) => file.write_all(bytes)?,
                    SnapshotMemory::Overlay { base, dirty_pages } => {
                        let mut offset = 0;
                        for (&page, bytes) in dirty_pages {
                            let start = page * chaoscontrol_vmm::snapshot::PAGE_SIZE;
                            file.write_all(&base[offset..start])?;
                            file.write_all(bytes.as_ref())?;
                            offset = start + bytes.len();
                        }
                        file.write_all(&base[offset..])?;
                    }
                }
                mismatch_memory.push(file_binding(&path)?);
            }
        }
    }
    let watchdog_passed = watchdog["passed"] == true
        && watchdog["watchdog_observed"] == true
        && watchdog["replacement_created"] == true
        && watchdog["poisoned_controller_dropped"] == true;
    let safety = long_lived["safety_invariants_hold"] == true
        && fresh_proof["safety_invariants_hold"] == true
        && recovered_proof["safety_invariants_hold"] == true;
    let proof = json!({"schema": "chaoscontrol.raft-lineage-runtime-proof.v2",
        "profile": file_binding(&output.join("profile.json"))?,
        "snapshots": snapshots, "replay_parent": parent_ref,
        "full_restore_incremental_recapture": {"memory_equal": recapture_equal},
        "replays": [long_lived, fresh_proof, recovered_proof], "equal": equal,
        "fresh_full_equal": fresh_equal, "watchdog_replacement_equal": recovery_equal,
        "watchdog_recovery": watchdog, "watchdog_event": file_binding(&output.join("watchdog-event.json"))?,
        "continuation_mismatch_memory": mismatch_memory,
        "safety_invariants_hold": safety, "passed": equal && safety && recapture_equal && watchdog_passed,
        "bug_reports": [], "probe": {"intentional": true, "enabled": true,
            "kind": "isolated-watchdog-only", "admitted_to_guest_evidence": false},
        "raw_artifact_policy": "debug-only-excluded-from-git",
        "gaps": ["Bounded local Raft execution only; no release-readiness or universal-determinism claim",
            "Canonical simulation RoundTrace is discarded by controller step_round; execution identity uses strict DlogRecord determinism fields, not CanonicalTrace::identity",
            "Dlog hot-path RIP is zero where the VMM does not fetch RIP; this is not a full register or instruction trace",
            "Every admitted assertion outcome is compared, including unexercised and bounded liveness failures; not every catalog site is claimed exercised",
            "No selected guest bug; isolated watchdog probe is discarded, not snapshot_backed_reproduced bug evidence",
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
    require(
        watchdog_passed,
        "watchdog timeout and controller replacement were not observed",
    )?;
    require(safety, "Raft safety invariant failed")?;
    println!("{}", output.join("runtime-proof.json").display());
    Ok(())
}

fn main() -> Result<()> {
    run(Args::parse())
}
