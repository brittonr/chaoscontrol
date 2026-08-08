use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chaoscontrol_trace::collector::{CollectorBounds, EvidenceTargetConfig, TraceCollector};
use chaoscontrol_trace::evidence::{
    parse_tracepoint_layout, privileged_preflight, reconcile_accounting, source_conformance_guard,
    AdmissionStatus, CaptureBounds, CompletenessStatus, PrivilegedPrerequisites,
    COMPILED_RING_BUFFER_BYTES,
};
use clap::Parser;

const SMOKE_DURATION: Duration = Duration::from_millis(250);
const SMOKE_MAXIMUM_EVENTS: u64 = 4_096;
const SMOKE_MAXIMUM_POLLS: u64 = 4_096;
const SMOKE_MAXIMUM_ARTIFACT_BYTES: u64 = 4 * 1_024 * 1_024;
const SMOKE_AGGREGATE_WINDOW: u64 = 256;
const SMOKE_VMM_PROFILE_REF: &str =
    "blake3:0000000000000000000000000000000000000000000000000000000000000000";
const TRACEFS_ROOT: &str = "/sys/kernel/tracing/events/kvm";
const BTF_PATH: &str = "/sys/kernel/btf/vmlinux";
const KVM_PATH: &str = "/dev/kvm";
const REQUIRED_TRACEPOINTS: [&str; 11] = [
    "kvm_exit",
    "kvm_entry",
    "kvm_pio",
    "kvm_mmio",
    "kvm_msr",
    "kvm_inj_virq",
    "kvm_pic_set_irq",
    "kvm_set_irq",
    "kvm_page_fault",
    "kvm_cr",
    "kvm_cpuid",
];

#[derive(Debug, Parser)]
#[command(about = "Validate bounded eBPF trace evidence fixtures and the privileged smoke lane")]
struct Args {
    /// Attach the bounded smoke collector to this existing TGID.
    #[arg(long)]
    privileged_smoke_pid: Option<u32>,

    /// Fail instead of reporting unsupported when privileged prerequisites are absent.
    #[arg(long)]
    require_privileged: bool,
}

fn main() {
    if let Err(error) = run(Args::parse()) {
        eprintln!("ebpf trace evidence selftest failed: {error:#}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    run_static_fixture_checks()?;
    let Some(pid) = args.privileged_smoke_pid else {
        println!("ebpf trace evidence fixtures ok; privileged smoke not requested");
        return Ok(());
    };
    let preflight = observe_privileged_preflight();
    if preflight.status != AdmissionStatus::Accepted {
        println!(
            "ebpf privileged smoke unsupported: missing={:?}; remediation={:?}",
            preflight.missing, preflight.remediation
        );
        if args.require_privileged {
            eprintln!("ebpf privileged smoke blocked: required prerequisites are absent");
            return Err(anyhow!(
                "required privileged smoke prerequisites are absent"
            ));
        }
        return Ok(());
    }
    run_privileged_smoke(pid)
}

fn run_static_fixture_checks() -> Result<()> {
    source_conformance_guard(
        include_str!("../collector.rs"),
        include_str!("../bpf/kvm_trace.bpf.c"),
    )?;
    for tracepoint in REQUIRED_TRACEPOINTS {
        let fixture = format!(
            "name: {tracepoint}\nformat:\n\tfield:unsigned short common_type; offset:0; size:2; signed:0;\n\tfield:unsigned int value; offset:8; size:4; signed:0;\n"
        );
        parse_tracepoint_layout(&format!("kvm:{tracepoint}"), &fixture)?;
    }
    let malformed = parse_tracepoint_layout(
        "kvm:kvm_exit",
        "field:unsigned int exit_reason; offset:invalid; size:4;",
    );
    if malformed.is_ok() {
        return Err(anyhow!("malformed tracepoint fixture was accepted"));
    }
    Ok(())
}

fn observe_privileged_preflight() -> chaoscontrol_trace::evidence::PrivilegedPreflight {
    let tracepoints = REQUIRED_TRACEPOINTS
        .iter()
        .all(|name| tracepoint_format_path(name).is_file());
    privileged_preflight(&PrivilegedPrerequisites {
        root_capability: unsafe { libc::geteuid() } == 0,
        kvm: Path::new(KVM_PATH).exists(),
        btf: Path::new(BTF_PATH).is_file(),
        tracepoints,
        pinned_loader: true,
    })
}

fn run_privileged_smoke(pid: u32) -> Result<()> {
    for tracepoint in REQUIRED_TRACEPOINTS {
        let path = tracepoint_format_path(tracepoint);
        let format = std::fs::read_to_string(&path)
            .with_context(|| format!("read tracepoint format {}", path.display()))?;
        parse_tracepoint_layout(&format!("kvm:{tracepoint}"), &format)?;
    }

    let mut collector = TraceCollector::with_evidence_target(
        pid,
        CollectorBounds {
            maximum_events: SMOKE_MAXIMUM_EVENTS,
            maximum_polls: SMOKE_MAXIMUM_POLLS,
        },
        EvidenceTargetConfig {
            run_id: "ebpf-attachment-smoke".to_string(),
            vmm_profile_ref: SMOKE_VMM_PROFILE_REF.to_string(),
        },
    )
    .context("open stable evidence target")?;
    collector.start().context("start privileged collector")?;
    thread::sleep(SMOKE_DURATION);
    collector.stop().context("stop privileged collector")?;

    let bounds = CaptureBounds {
        maximum_ring_bytes: COMPILED_RING_BUFFER_BYTES,
        maximum_queue_events: SMOKE_MAXIMUM_EVENTS,
        maximum_polls: SMOKE_MAXIMUM_POLLS,
        maximum_events: SMOKE_MAXIMUM_EVENTS,
        maximum_artifact_bytes: SMOKE_MAXIMUM_ARTIFACT_BYTES,
        aggregate_window_events: SMOKE_AGGREGATE_WINDOW,
    };
    let accounting = reconcile_accounting(
        &collector.producer_accounting(),
        &collector.userspace_accounting(),
        &bounds,
    )?;
    if accounting.status != CompletenessStatus::Complete {
        return Err(anyhow!(
            "privileged smoke produced incomplete accounting: {:?}",
            accounting.blockers
        ));
    }
    if !chaoscontrol_trace::evidence::cleanup_complete(&collector.cleanup_outcome()) {
        return Err(anyhow!("privileged smoke cleanup was incomplete"));
    }
    let target_binding = collector
        .target_binding_report()
        .ok_or_else(|| anyhow!("privileged smoke lacks target binding evidence"))?;
    if target_binding.status != chaoscontrol_trace::evidence::TargetBindingStatus::Stable {
        return Err(anyhow!(
            "privileged smoke target binding is not stable: {:?}",
            target_binding.blockers
        ));
    }
    println!(
        "ebpf privileged smoke complete: pid={pid} accepted_records={} non_claim='attachment smoke only; not VM determinism proof or release eligibility'",
        accounting.accepted_records
    );
    Ok(())
}

fn tracepoint_format_path(name: &str) -> PathBuf {
    Path::new(TRACEFS_ROOT).join(name).join("format")
}
