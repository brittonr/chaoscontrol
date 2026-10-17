use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chaoscontrol_trace::collector::TraceCollector;
use chaoscontrol_trace::evidence::{
    admit_capture_profile, parse_tracepoint_layout, privileged_preflight, reconcile_accounting,
    source_conformance_guard, AdmissionStatus, AdmittedCaptureProfile, BuildIdentity,
    CaptureProfile, CompletenessStatus, PrivilegedPrerequisites,
};
use clap::Parser;

const SMOKE_DURATION: Duration = Duration::from_millis(250);
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

    /// Exact capture profile; mandatory for privileged capture, never synthesized.
    #[arg(long)]
    capture_profile: Option<PathBuf>,

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
    let profile_path = args.capture_profile.as_ref().ok_or_else(|| {
        anyhow!("privileged capture requires --capture-profile with the exact admitted cohort")
    })?;
    let profile_file = std::fs::File::open(profile_path).context("open exact capture profile")?;
    if profile_file.metadata()?.len() > 1_048_576 {
        return Err(anyhow!("capture profile exceeds its byte bound"));
    }
    let profile: CaptureProfile =
        serde_json::from_reader(profile_file).context("parse exact capture profile")?;
    let profile = admit_capture_profile(&profile)?;
    if profile.profile.target.tgid != pid {
        return Err(anyhow!("requested TGID differs from capture profile"));
    }
    let preflight = observe_privileged_preflight(&profile);
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
    run_privileged_smoke(profile)
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

fn observe_privileged_preflight(
    profile: &AdmittedCaptureProfile,
) -> chaoscontrol_trace::evidence::PrivilegedPreflight {
    let tracepoints = REQUIRED_TRACEPOINTS
        .iter()
        .all(|name| tracepoint_format_path(name).is_file());
    privileged_preflight(&PrivilegedPrerequisites {
        root_capability: unsafe { libc::geteuid() } == 0,
        kvm: Path::new(KVM_PATH).exists(),
        btf: Path::new(BTF_PATH).is_file(),
        tracepoints,
        pinned_loader: BuildIdentity::compiled(profile.profile.build.compiled_layouts.clone())
            == profile.profile.build
            && !profile.profile.build.fallback_types_used,
    })
}

fn run_privileged_smoke(profile: AdmittedCaptureProfile) -> Result<()> {
    for tracepoint in REQUIRED_TRACEPOINTS {
        let path = tracepoint_format_path(tracepoint);
        let format = std::fs::read_to_string(&path)
            .with_context(|| format!("read tracepoint format {}", path.display()))?;
        parse_tracepoint_layout(&format!("kvm:{tracepoint}"), &format)?;
    }

    let pid = profile.profile.target.tgid;
    let bounds = profile.profile.bounds.clone();
    let mut collector = TraceCollector::for_admitted_profile(profile)
        .context("admit exact runtime, loader, stable target, and producer affinity")?;
    collector.start().context("start privileged collector")?;
    thread::sleep(SMOKE_DURATION);
    collector.stop().context("stop privileged collector")?;

    let accounting = reconcile_accounting(
        &collector.producer_accounting(),
        &collector.userspace_accounting(),
        &bounds,
    )?;
    if accounting.status != CompletenessStatus::Complete || accounting.accepted_records == 0 {
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
