//! Pure, bounded eBPF KVM trace evidence semantics.
//!
//! This module performs no filesystem, process, clock, kernel, or BPF effects.
//! The collector shell supplies explicit facts and retains host authority.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::events::{
    EventKind, EventType, RawEvent, TraceEvent, RAW_EVENT_SCHEMA_VERSION, RAW_EVENT_SIZE,
};

pub const CAPTURE_PROFILE_SCHEMA: &str = "chaoscontrol.ebpf-trace-capture-profile.v1";
pub const TRACE_MANIFEST_SCHEMA: &str = "chaoscontrol.ebpf-trace-manifest.v1";
pub const COMPARISON_RECEIPT_SCHEMA: &str = "chaoscontrol.ebpf-trace-comparison.v1";
pub const TRACE_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.ebpf-trace.manifest.v1\0";
pub const COMPARISON_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.ebpf-trace.comparison.v1\0";
pub const PROFILE_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.ebpf-trace.profile.v1\0";
pub const EVENT_ARTIFACT_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.ebpf-trace.events.v1\0";
pub const AGGREGATE_ARTIFACT_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.ebpf-trace.aggregate.v1\0";
pub const FILTER_SEMANTICS: &str = "exact-tgid-plus-stable-process-identity";
pub const DIGEST_PREFIX: &str = "blake3:";
pub const DIGEST_HEX_LENGTH: usize = 64;
pub const MAXIMUM_IDENTIFIER_BYTES: usize = 512;
pub const COMPILED_RING_BUFFER_BYTES: u64 = 16 * 1_024 * 1_024;
pub const SUPPORTED_MAXIMUM_RING_BYTES: u64 = 64 * 1_024 * 1_024;
pub const SUPPORTED_MAXIMUM_QUEUE_EVENTS: u64 = 1_000_000;
pub const MINIMUM_CAPTURE_POLLS: u64 = 2;
pub const SUPPORTED_MAXIMUM_POLLS: u64 = 1_000_000;
pub const SUPPORTED_MAXIMUM_EVENTS: u64 = 1_000_000;
pub const SUPPORTED_MAXIMUM_ARTIFACT_BYTES: u64 = 128 * 1_024 * 1_024;
pub const SUPPORTED_MAXIMUM_PRODUCERS: u32 = 4_096;
pub const SUPPORTED_MAXIMUM_VCPUS: u32 = 4_096;
pub const SUPPORTED_MAXIMUM_LAYOUTS: usize = 64;
pub const SUPPORTED_MAXIMUM_ENABLED_EVENTS: usize = 64;
pub const REQUIRED_NON_CLAIMS: [&str; 7] = [
    "not VM determinism proof",
    "not replay correctness proof",
    "not eBPF safety proof",
    "not kernel correctness proof",
    "not security proof",
    "not physical readiness proof",
    "not release eligibility",
];

/// r[ebpf_trace_evidence.profile]
/// r[ebpf_trace_evidence.admission]
/// r[ebpf_trace_evidence.accounting]
/// r[ebpf_trace_evidence.ordering]
/// r[ebpf_trace_evidence.comparison]
/// r[ebpf_trace_evidence.lifecycle]
/// r[ebpf_trace_evidence.evidence]
/// r[ebpf_trace_evidence.verification]
pub const REQUIREMENT_MARKERS: [&str; 8] = [
    "ebpf_trace_evidence.profile",
    "ebpf_trace_evidence.admission",
    "ebpf_trace_evidence.accounting",
    "ebpf_trace_evidence.ordering",
    "ebpf_trace_evidence.comparison",
    "ebpf_trace_evidence.lifecycle",
    "ebpf_trace_evidence.evidence",
    "ebpf_trace_evidence.verification",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceError {
    pub class: &'static str,
    pub detail: String,
}

impl EvidenceError {
    pub fn new(class: &'static str, detail: impl Into<String>) -> Self {
        Self {
            class,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.class, self.detail)
    }
}

impl std::error::Error for EvidenceError {}

pub type EvidenceResult<T> = Result<T, EvidenceError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrderingMode {
    ExactSingleProducer,
    SourcePartialOrder,
    Aggregate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureBounds {
    pub maximum_ring_bytes: u64,
    pub maximum_queue_events: u64,
    pub maximum_polls: u64,
    pub maximum_events: u64,
    pub maximum_artifact_bytes: u64,
    pub aggregate_window_events: u64,
}

impl CaptureBounds {
    fn validate(&self) -> EvidenceResult<()> {
        let values = [
            (
                "maximum_ring_bytes",
                self.maximum_ring_bytes,
                SUPPORTED_MAXIMUM_RING_BYTES,
            ),
            (
                "maximum_queue_events",
                self.maximum_queue_events,
                SUPPORTED_MAXIMUM_QUEUE_EVENTS,
            ),
            ("maximum_polls", self.maximum_polls, SUPPORTED_MAXIMUM_POLLS),
            (
                "maximum_events",
                self.maximum_events,
                SUPPORTED_MAXIMUM_EVENTS,
            ),
            (
                "maximum_artifact_bytes",
                self.maximum_artifact_bytes,
                SUPPORTED_MAXIMUM_ARTIFACT_BYTES,
            ),
            (
                "aggregate_window_events",
                self.aggregate_window_events,
                SUPPORTED_MAXIMUM_EVENTS,
            ),
        ];
        for (name, value, maximum) in values {
            if value == 0 || value > maximum {
                return Err(EvidenceError::new(
                    "profile-bound",
                    format!("{name} must be positive and no greater than {maximum}"),
                ));
            }
        }
        if self.maximum_polls < MINIMUM_CAPTURE_POLLS {
            return Err(EvidenceError::new(
                "profile-bound",
                format!("maximum_polls must be at least {MINIMUM_CAPTURE_POLLS}"),
            ));
        }
        if self.aggregate_window_events > self.maximum_events {
            return Err(EvidenceError::new(
                "profile-bound",
                "aggregate_window_events exceeds maximum_events",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TracepointFieldSignature {
    pub field: String,
    pub offset: u32,
    pub size: u32,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TracepointLayoutSignature {
    pub tracepoint: String,
    pub format_ref: String,
    pub fields: Vec<TracepointFieldSignature>,
}

pub fn parse_tracepoint_layout(
    tracepoint: &str,
    format_text: &str,
) -> EvidenceResult<TracepointLayoutSignature> {
    validate_identifier("tracepoint", tracepoint)?;
    if format_text.is_empty() {
        return Err(EvidenceError::new(
            "layout-format",
            "tracepoint format text is empty",
        ));
    }
    let mut fields = Vec::new();
    let mut names = BTreeSet::new();
    for line in format_text.lines() {
        let Some(field_start) = line.trim().strip_prefix("field:") else {
            continue;
        };
        let segments: Vec<_> = field_start.split(';').collect();
        if segments.len() < 3 {
            return Err(EvidenceError::new(
                "layout-format",
                "tracepoint field line lacks offset or size",
            ));
        }
        let declaration = segments[0].trim();
        let Some((type_name, field)) = declaration.rsplit_once(char::is_whitespace) else {
            return Err(EvidenceError::new(
                "layout-format",
                "tracepoint field declaration lacks a type",
            ));
        };
        let field = field.trim().trim_start_matches('*').to_string();
        let type_name = type_name.trim().to_string();
        let offset = parse_layout_number(segments[1], "offset:")?;
        let size = parse_layout_number(segments[2], "size:")?;
        if size == 0 || !names.insert(field.clone()) {
            return Err(EvidenceError::new(
                "layout-format",
                "tracepoint fields require positive sizes and unique names",
            ));
        }
        fields.push(TracepointFieldSignature {
            field,
            offset,
            size,
            type_name,
        });
    }
    if fields.is_empty() {
        return Err(EvidenceError::new(
            "layout-format",
            "tracepoint format contains no fields",
        ));
    }
    Ok(TracepointLayoutSignature {
        tracepoint: tracepoint.to_string(),
        format_ref: format!(
            "{DIGEST_PREFIX}{}",
            blake3::hash(format_text.as_bytes()).to_hex()
        ),
        fields,
    })
}

fn parse_layout_number(segment: &str, prefix: &str) -> EvidenceResult<u32> {
    segment
        .trim()
        .strip_prefix(prefix)
        .ok_or_else(|| EvidenceError::new("layout-format", format!("missing {prefix} segment")))?
        .trim()
        .parse::<u32>()
        .map_err(|_| EvidenceError::new("layout-format", format!("invalid {prefix} value")))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildIdentity {
    pub bpf_object_ref: String,
    pub bpf_source_ref: String,
    pub event_schema_ref: String,
    pub loader_ref: String,
    pub btf_source_ref: String,
    pub fallback_types_used: bool,
    pub compiled_layouts: Vec<TracepointLayoutSignature>,
}

impl BuildIdentity {
    pub fn compiled(compiled_layouts: Vec<TracepointLayoutSignature>) -> Self {
        Self {
            bpf_object_ref: env!("CHAOSCONTROL_TRACE_BPF_OBJECT_REF").to_string(),
            bpf_source_ref: env!("CHAOSCONTROL_TRACE_BPF_SOURCE_REF").to_string(),
            event_schema_ref: env!("CHAOSCONTROL_TRACE_EVENT_SCHEMA_REF").to_string(),
            loader_ref: env!("CHAOSCONTROL_TRACE_LOADER_REF").to_string(),
            btf_source_ref: env!("CHAOSCONTROL_TRACE_BTF_SOURCE_REF").to_string(),
            fallback_types_used: env!("CHAOSCONTROL_TRACE_FALLBACK_TYPES_USED") == "true",
            compiled_layouts,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCohort {
    pub kernel_release: String,
    pub architecture: String,
    pub btf_ref: String,
    pub runtime_layouts: Vec<TracepointLayoutSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetIdentity {
    pub run_id: String,
    pub tgid: u32,
    pub process_start_ref: String,
    pub executable_ref: String,
    pub vmm_profile_ref: String,
    pub cgroup_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerTopology {
    pub vcpu_count: u32,
    pub producer_count: u32,
    pub affinity_cpus: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureProfile {
    pub schema: String,
    pub profile_id: String,
    pub build: BuildIdentity,
    pub expected_runtime: RuntimeCohort,
    pub target: TargetIdentity,
    pub topology: ProducerTopology,
    pub enabled_event_types: Vec<u32>,
    pub filter_semantics: String,
    pub ordering_mode: OrderingMode,
    pub bounds: CaptureBounds,
    pub retention: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmittedCaptureProfile {
    pub profile_ref: String,
    pub profile: CaptureProfile,
}

pub fn admit_capture_profile(profile: &CaptureProfile) -> EvidenceResult<AdmittedCaptureProfile> {
    if profile.schema != CAPTURE_PROFILE_SCHEMA {
        return Err(EvidenceError::new(
            "profile-schema",
            "unsupported eBPF trace capture profile schema",
        ));
    }
    validate_identifier("profile_id", &profile.profile_id)?;
    profile.bounds.validate()?;
    if profile.bounds.maximum_ring_bytes != COMPILED_RING_BUFFER_BYTES {
        return Err(EvidenceError::new(
            "profile-ring",
            format!(
                "maximum_ring_bytes must equal compiled ring size {COMPILED_RING_BUFFER_BYTES}"
            ),
        ));
    }
    if profile.bounds.maximum_queue_events != profile.bounds.maximum_events {
        return Err(EvidenceError::new(
            "profile-queue",
            "direct-retention collector requires maximum_queue_events to equal maximum_events",
        ));
    }
    validate_build_identity(&profile.build)?;
    validate_runtime_cohort(&profile.expected_runtime)?;
    validate_target(&profile.target)?;
    if profile.filter_semantics != FILTER_SEMANTICS {
        return Err(EvidenceError::new(
            "profile-filter",
            "unsupported target filter semantics",
        ));
    }
    validate_identifier("retention", &profile.retention)?;
    validate_topology(&profile.topology)?;
    if profile.enabled_event_types.is_empty()
        || profile.enabled_event_types.len() > SUPPORTED_MAXIMUM_ENABLED_EVENTS
    {
        return Err(EvidenceError::new(
            "profile-events",
            "enabled event type set is empty or over the supported bound",
        ));
    }
    let mut enabled = BTreeSet::new();
    for event_type in &profile.enabled_event_types {
        let Some(event_type) = EventType::from_u32(*event_type) else {
            return Err(EvidenceError::new(
                "profile-events",
                "enabled event types must be known and unique",
            ));
        };
        if !enabled.insert(event_type as u32) {
            return Err(EvidenceError::new(
                "profile-events",
                "enabled event types must be known and unique",
            ));
        }
        let tracepoint = event_tracepoint(event_type)?;
        if !profile
            .build
            .compiled_layouts
            .iter()
            .any(|layout| layout.tracepoint == tracepoint)
        {
            return Err(EvidenceError::new(
                "profile-layout",
                format!(
                    "enabled event {} lacks a compiled tracepoint layout",
                    event_type.name()
                ),
            ));
        }
    }
    if profile.build.compiled_layouts != profile.expected_runtime.runtime_layouts {
        return Err(EvidenceError::new(
            "profile-layout",
            "compiled and expected runtime tracepoint layouts differ",
        ));
    }
    if !profile.build.fallback_types_used
        && profile.build.btf_source_ref != profile.expected_runtime.btf_ref
    {
        return Err(EvidenceError::new(
            "profile-btf",
            "non-fallback build BTF identity differs from expected runtime BTF identity",
        ));
    }
    if profile.ordering_mode == OrderingMode::ExactSingleProducer
        && (profile.topology.producer_count != 1 || profile.topology.affinity_cpus.len() != 1)
    {
        return Err(EvidenceError::new(
            "profile-ordering",
            "exact ordering requires one producer and one declared affinity CPU",
        ));
    }
    for required in REQUIRED_NON_CLAIMS {
        if !profile.non_claims.iter().any(|claim| claim == required) {
            return Err(EvidenceError::new(
                "profile-overclaim",
                format!("capture profile omits required non-claim: {required}"),
            ));
        }
    }
    let profile_ref = canonical_ref(PROFILE_IDENTITY_DOMAIN, profile)?;
    Ok(AdmittedCaptureProfile {
        profile_ref,
        profile: profile.clone(),
    })
}

fn event_tracepoint(event_type: EventType) -> EvidenceResult<&'static str> {
    match event_type {
        EventType::KvmExit => Ok("kvm:kvm_exit"),
        EventType::KvmEntry => Ok("kvm:kvm_entry"),
        EventType::KvmPio => Ok("kvm:kvm_pio"),
        EventType::KvmMmio => Ok("kvm:kvm_mmio"),
        EventType::KvmMsr => Ok("kvm:kvm_msr"),
        EventType::KvmInjVirq => Ok("kvm:kvm_inj_virq"),
        EventType::KvmPicIrq => Ok("kvm:kvm_pic_set_irq"),
        EventType::KvmSetIrq => Ok("kvm:kvm_set_irq"),
        EventType::KvmPageFault => Ok("kvm:kvm_page_fault"),
        EventType::KvmCr => Ok("kvm:kvm_cr"),
        EventType::KvmCpuid => Ok("kvm:kvm_cpuid"),
        EventType::Unknown => Err(EvidenceError::new(
            "profile-events",
            "unknown event type has no tracepoint layout",
        )),
    }
}

fn validate_build_identity(build: &BuildIdentity) -> EvidenceResult<()> {
    for (name, reference) in [
        ("bpf_object_ref", &build.bpf_object_ref),
        ("bpf_source_ref", &build.bpf_source_ref),
        ("event_schema_ref", &build.event_schema_ref),
        ("loader_ref", &build.loader_ref),
        ("btf_source_ref", &build.btf_source_ref),
    ] {
        validate_digest_ref(name, reference)?;
    }
    validate_layouts(&build.compiled_layouts)
}

fn validate_runtime_cohort(runtime: &RuntimeCohort) -> EvidenceResult<()> {
    validate_identifier("kernel_release", &runtime.kernel_release)?;
    validate_identifier("architecture", &runtime.architecture)?;
    validate_digest_ref("btf_ref", &runtime.btf_ref)?;
    validate_layouts(&runtime.runtime_layouts)
}

fn validate_layouts(layouts: &[TracepointLayoutSignature]) -> EvidenceResult<()> {
    if layouts.is_empty() || layouts.len() > SUPPORTED_MAXIMUM_LAYOUTS {
        return Err(EvidenceError::new(
            "layout-bound",
            "tracepoint layouts are empty or over the supported bound",
        ));
    }
    let mut tracepoints = BTreeSet::new();
    for layout in layouts {
        validate_identifier("tracepoint", &layout.tracepoint)?;
        validate_digest_ref("tracepoint.format_ref", &layout.format_ref)?;
        if layout.fields.is_empty() || !tracepoints.insert(&layout.tracepoint) {
            return Err(EvidenceError::new(
                "layout-shape",
                "tracepoint layouts require unique names and non-empty fields",
            ));
        }
        let mut fields = BTreeSet::new();
        for field in &layout.fields {
            validate_identifier("tracepoint.field", &field.field)?;
            validate_identifier("tracepoint.type_name", &field.type_name)?;
            if field.size == 0 || !fields.insert(&field.field) {
                return Err(EvidenceError::new(
                    "layout-field",
                    "layout fields require positive sizes and unique names",
                ));
            }
        }
    }
    Ok(())
}

fn validate_target(target: &TargetIdentity) -> EvidenceResult<()> {
    validate_identifier("target.run_id", &target.run_id)?;
    if target.tgid == 0 {
        return Err(EvidenceError::new(
            "target-tgid",
            "target TGID must be positive",
        ));
    }
    for (name, reference) in [
        ("target.process_start_ref", &target.process_start_ref),
        ("target.executable_ref", &target.executable_ref),
        ("target.vmm_profile_ref", &target.vmm_profile_ref),
    ] {
        validate_digest_ref(name, reference)?;
    }
    if let Some(cgroup_ref) = &target.cgroup_ref {
        validate_digest_ref("target.cgroup_ref", cgroup_ref)?;
    }
    Ok(())
}

fn validate_topology(topology: &ProducerTopology) -> EvidenceResult<()> {
    if topology.vcpu_count == 0
        || topology.vcpu_count > SUPPORTED_MAXIMUM_VCPUS
        || topology.producer_count == 0
        || topology.producer_count > SUPPORTED_MAXIMUM_PRODUCERS
        || topology.producer_count > topology.vcpu_count
    {
        return Err(EvidenceError::new(
            "topology-bound",
            "vCPU or producer topology is zero, inconsistent, or over the supported bound",
        ));
    }
    let affinity: BTreeSet<_> = topology.affinity_cpus.iter().copied().collect();
    if affinity.len() != topology.affinity_cpus.len()
        || topology.affinity_cpus.len()
            > usize::try_from(topology.producer_count).unwrap_or(usize::MAX)
    {
        return Err(EvidenceError::new(
            "topology-affinity",
            "affinity CPU identities must be unique and bounded by producer count",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdmissionStatus {
    Accepted,
    DebugOnly,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeAdmission {
    pub status: AdmissionStatus,
    pub blockers: Vec<String>,
}

pub fn admit_runtime_cohort(
    admitted: &AdmittedCaptureProfile,
    observed: &RuntimeCohort,
    observed_object_ref: &str,
    observed_loader_ref: &str,
) -> EvidenceResult<RuntimeAdmission> {
    validate_runtime_cohort(observed)?;
    validate_digest_ref("observed_object_ref", observed_object_ref)?;
    validate_digest_ref("observed_loader_ref", observed_loader_ref)?;
    let mut blockers = Vec::new();
    if admitted.profile.build.fallback_types_used {
        blockers.push("BPF object was compiled with fallback type stubs".to_string());
    }
    if observed_object_ref != admitted.profile.build.bpf_object_ref {
        blockers.push("BPF object identity drift".to_string());
    }
    if observed_loader_ref != admitted.profile.build.loader_ref {
        blockers.push("loader identity drift".to_string());
    }
    if observed.kernel_release != admitted.profile.expected_runtime.kernel_release {
        blockers.push("kernel release drift".to_string());
    }
    if observed.architecture != admitted.profile.expected_runtime.architecture {
        blockers.push("kernel architecture drift".to_string());
    }
    if observed.btf_ref != admitted.profile.expected_runtime.btf_ref {
        blockers.push("runtime BTF identity drift".to_string());
    }
    if observed.runtime_layouts != admitted.profile.build.compiled_layouts
        || observed.runtime_layouts != admitted.profile.expected_runtime.runtime_layouts
    {
        blockers.push("tracepoint layout signature drift".to_string());
    }
    Ok(RuntimeAdmission {
        status: if blockers.is_empty() {
            AdmissionStatus::Accepted
        } else if admitted.profile.build.fallback_types_used {
            AdmissionStatus::DebugOnly
        } else {
            AdmissionStatus::Blocked
        },
        blockers,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProducerCounters {
    pub source_cpu: u32,
    pub eligible_attempts: u64,
    pub submitted_records: u64,
    pub reservation_drops: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerAccounting {
    pub available: bool,
    pub sources: Vec<SourceProducerCounters>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UserspaceAccounting {
    pub received_records: u64,
    pub accepted_records: u64,
    pub malformed_size: u64,
    pub wrong_version: u64,
    pub unknown_discriminant: u64,
    pub parse_failed: u64,
    pub over_bound_drops: u64,
    pub callback_failures: u64,
    pub lock_failures: u64,
    pub polls: u64,
    pub poll_failures: u64,
    pub final_drain_attempted: bool,
    pub final_drain_succeeded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompletenessStatus {
    Complete,
    Partial,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingReport {
    pub status: CompletenessStatus,
    pub eligible_attempts: u64,
    pub submitted_records: u64,
    pub reservation_drops: u64,
    pub received_records: u64,
    pub accepted_records: u64,
    pub blockers: Vec<String>,
}

pub fn reconcile_accounting(
    producer: &ProducerAccounting,
    userspace: &UserspaceAccounting,
    bounds: &CaptureBounds,
) -> EvidenceResult<AccountingReport> {
    bounds.validate()?;
    if !producer.available {
        return Ok(AccountingReport {
            status: CompletenessStatus::Unsupported,
            eligible_attempts: 0,
            submitted_records: 0,
            reservation_drops: 0,
            received_records: userspace.received_records,
            accepted_records: userspace.accepted_records,
            blockers: vec!["producer accounting is unavailable".to_string()],
        });
    }
    let mut sources = BTreeSet::new();
    let mut eligible_attempts = 0_u64;
    let mut submitted_records = 0_u64;
    let mut reservation_drops = 0_u64;
    for source in &producer.sources {
        if !sources.insert(source.source_cpu) {
            return Err(EvidenceError::new(
                "accounting-source",
                "producer source counters contain a duplicate CPU identity",
            ));
        }
        eligible_attempts = eligible_attempts
            .checked_add(source.eligible_attempts)
            .ok_or_else(|| {
                EvidenceError::new("accounting-overflow", "eligible attempts overflow")
            })?;
        submitted_records = submitted_records
            .checked_add(source.submitted_records)
            .ok_or_else(|| {
                EvidenceError::new("accounting-overflow", "submitted records overflow")
            })?;
        reservation_drops = reservation_drops
            .checked_add(source.reservation_drops)
            .ok_or_else(|| {
                EvidenceError::new("accounting-overflow", "reservation drops overflow")
            })?;
    }
    let mut blockers = Vec::new();
    if submitted_records
        .checked_add(reservation_drops)
        .is_none_or(|total| total != eligible_attempts)
    {
        blockers.push("producer attempts do not reconcile with submissions and drops".to_string());
    }
    if reservation_drops != 0 {
        blockers.push("ring-buffer reservation loss is non-zero".to_string());
    }
    if submitted_records != userspace.received_records {
        blockers.push("producer submissions do not equal userspace receipts".to_string());
    }
    if userspace.received_records != userspace.accepted_records {
        blockers.push("userspace did not accept every received record".to_string());
    }
    let rejected = [
        ("malformed size", userspace.malformed_size),
        ("wrong version", userspace.wrong_version),
        ("unknown discriminant", userspace.unknown_discriminant),
        ("parse failure", userspace.parse_failed),
        ("over-bound drop", userspace.over_bound_drops),
        ("callback failure", userspace.callback_failures),
        ("lock failure", userspace.lock_failures),
        ("poll failure", userspace.poll_failures),
    ];
    for (name, count) in rejected {
        if count != 0 {
            blockers.push(format!("userspace {name} count is non-zero"));
        }
    }
    if userspace.polls > bounds.maximum_polls {
        blockers.push("poll count exceeds maximum_polls".to_string());
    }
    if userspace.accepted_records > bounds.maximum_events {
        blockers.push("accepted records exceed maximum_events".to_string());
    }
    if !userspace.final_drain_attempted || !userspace.final_drain_succeeded {
        blockers.push("required final drain did not succeed".to_string());
    }
    Ok(AccountingReport {
        status: if blockers.is_empty() {
            CompletenessStatus::Complete
        } else {
            CompletenessStatus::Partial
        },
        eligible_attempts,
        submitted_records,
        reservation_drops,
        received_records: userspace.received_records,
        accepted_records: userspace.accepted_records,
        blockers,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawRecordError {
    Size,
    Version,
    RecordSize,
    UnknownDiscriminant,
    Target,
    EventBound,
}

impl RawRecordError {
    pub fn class(&self) -> &'static str {
        match self {
            Self::Size => "raw-size",
            Self::Version => "raw-version",
            Self::RecordSize => "raw-record-size",
            Self::UnknownDiscriminant => "raw-discriminant",
            Self::Target => "raw-target",
            Self::EventBound => "raw-event-bound",
        }
    }
}

pub fn parse_raw_record(
    bytes: &[u8],
    capture_index: u64,
    expected_tgid: u32,
    maximum_events: u64,
) -> Result<TraceEvent, RawRecordError> {
    let raw_size = usize::from(RAW_EVENT_SIZE);
    if bytes.len() != raw_size {
        return Err(RawRecordError::Size);
    }
    if capture_index >= maximum_events {
        return Err(RawRecordError::EventBound);
    }
    let raw = RawEvent {
        seq: read_u64(bytes, 0),
        host_ns: read_u64(bytes, 8),
        event_type: read_u32(bytes, 16),
        pid: read_u32(bytes, 20),
        source_cpu: read_u32(bytes, 24),
        schema_version: read_u16(bytes, 28),
        record_size: read_u16(bytes, 30),
        arg0: read_u64(bytes, 32),
        arg1: read_u64(bytes, 40),
        arg2: read_u64(bytes, 48),
        arg3: read_u64(bytes, 56),
    };
    if raw.schema_version != RAW_EVENT_SCHEMA_VERSION {
        return Err(RawRecordError::Version);
    }
    if raw.record_size != RAW_EVENT_SIZE {
        return Err(RawRecordError::RecordSize);
    }
    if EventType::from_u32(raw.event_type).is_none() {
        return Err(RawRecordError::UnknownDiscriminant);
    }
    if raw.pid != expected_tgid {
        return Err(RawRecordError::Target);
    }
    let mut event = TraceEvent::from_raw(&raw);
    event.capture_index = capture_index;
    Ok(event)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_ne_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    let mut value = [0_u8; std::mem::size_of::<u32>()];
    value.copy_from_slice(&bytes[offset..offset + std::mem::size_of::<u32>()]);
    u32::from_ne_bytes(value)
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    let mut value = [0_u8; std::mem::size_of::<u64>()];
    value.copy_from_slice(&bytes[offset..offset + std::mem::size_of::<u64>()]);
    u64::from_ne_bytes(value)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceReport {
    pub continuous: bool,
    pub source_event_counts: BTreeMap<u32, u64>,
    pub first_error: Option<String>,
}

pub fn validate_source_sequences(events: &[TraceEvent]) -> SequenceReport {
    let mut by_source: BTreeMap<u32, Vec<u64>> = BTreeMap::new();
    for event in events {
        by_source
            .entry(event.source_cpu)
            .or_default()
            .push(event.seq);
    }
    let mut source_event_counts = BTreeMap::new();
    for (source, sequences) in &mut by_source {
        sequences.sort_unstable();
        source_event_counts.insert(*source, u64::try_from(sequences.len()).unwrap_or(u64::MAX));
        for (position, sequence) in sequences.iter().enumerate() {
            let expected = u64::try_from(position).unwrap_or(u64::MAX);
            if *sequence != expected {
                return SequenceReport {
                    continuous: false,
                    source_event_counts,
                    first_error: Some(format!(
                        "source {source} expected sequence {expected} but observed {sequence}"
                    )),
                };
            }
        }
    }
    SequenceReport {
        continuous: true,
        source_event_counts,
        first_error: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetObservation {
    pub run_id: String,
    pub tgid: u32,
    pub process_start_ref: String,
    pub executable_ref: String,
    pub vmm_profile_ref: String,
    pub cgroup_ref: Option<String>,
    pub exited: bool,
    pub exec_changed: bool,
}

pub fn validate_target_observation(
    expected: &TargetIdentity,
    observed: &TargetObservation,
) -> EvidenceResult<()> {
    if observed.run_id != expected.run_id
        || observed.tgid != expected.tgid
        || observed.process_start_ref != expected.process_start_ref
        || observed.executable_ref != expected.executable_ref
        || observed.vmm_profile_ref != expected.vmm_profile_ref
        || observed.cgroup_ref != expected.cgroup_ref
        || observed.exited
        || observed.exec_changed
    {
        return Err(EvidenceError::new(
            "target-drift",
            "target exited, was reused, execed, or changed its bound identity",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetBindingStatus {
    Stable,
    Drifted,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetBindingReport {
    pub status: TargetBindingStatus,
    pub blockers: Vec<String>,
}

pub fn assess_target_binding(
    expected: &TargetIdentity,
    start: &TargetObservation,
    end: &TargetObservation,
) -> TargetBindingReport {
    if let Err(error) = validate_target_observation(expected, start) {
        return TargetBindingReport {
            status: TargetBindingStatus::Blocked,
            blockers: vec![format!("start boundary: {}", error.detail)],
        };
    }
    if let Err(error) = validate_target_observation(expected, end) {
        return TargetBindingReport {
            status: TargetBindingStatus::Drifted,
            blockers: vec![format!("end boundary: {}", error.detail)],
        };
    }
    TargetBindingReport {
        status: TargetBindingStatus::Stable,
        blockers: Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CleanupStepStatus {
    Succeeded,
    Failed,
    NotRequired,
    NotAttempted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupOutcome {
    pub quiesce: CleanupStepStatus,
    pub final_poll: CleanupStepStatus,
    pub accounting_snapshot: CleanupStepStatus,
    pub detach: CleanupStepStatus,
    pub unpin: CleanupStepStatus,
    pub cleanup: CleanupStepStatus,
}

pub fn cleanup_complete(outcome: &CleanupOutcome) -> bool {
    let required = [
        outcome.quiesce,
        outcome.final_poll,
        outcome.accounting_snapshot,
        outcome.detach,
        outcome.cleanup,
    ];
    required
        .iter()
        .all(|status| *status == CleanupStepStatus::Succeeded)
        && matches!(
            outcome.unpin,
            CleanupStepStatus::Succeeded | CleanupStepStatus::NotRequired
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalClass {
    Complete,
    Partial,
    Unsupported,
    Incompatible,
    Divergent,
    Blocked,
    CleanupFailed,
}

pub fn classify_capture_terminal(
    admission: &RuntimeAdmission,
    target_binding: &TargetBindingReport,
    accounting: &AccountingReport,
    sequence: &SequenceReport,
    cleanup: &CleanupOutcome,
) -> TerminalClass {
    if !cleanup_complete(cleanup) {
        return TerminalClass::CleanupFailed;
    }
    match admission.status {
        AdmissionStatus::DebugOnly => return TerminalClass::Unsupported,
        AdmissionStatus::Blocked => return TerminalClass::Blocked,
        AdmissionStatus::Accepted => {}
    }
    match target_binding.status {
        TargetBindingStatus::Blocked => return TerminalClass::Blocked,
        TargetBindingStatus::Drifted => return TerminalClass::Partial,
        TargetBindingStatus::Stable => {}
    }
    match accounting.status {
        CompletenessStatus::Unsupported => TerminalClass::Unsupported,
        CompletenessStatus::Failed => TerminalClass::Blocked,
        CompletenessStatus::Partial => TerminalClass::Partial,
        CompletenessStatus::Complete if !sequence.continuous => TerminalClass::Partial,
        CompletenessStatus::Complete => TerminalClass::Complete,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalEvent {
    pub source_cpu: u32,
    pub source_sequence: u64,
    pub event_type: String,
    pub kind: EventKind,
}

fn canonical_event(event: &TraceEvent) -> EvidenceResult<CanonicalEvent> {
    let event_type = event.event_type();
    if event_type == EventType::Unknown || event.schema_version != RAW_EVENT_SCHEMA_VERSION {
        return Err(EvidenceError::new(
            "event-schema",
            "unknown or wrong-version event cannot enter an evidence projection",
        ));
    }
    Ok(CanonicalEvent {
        source_cpu: event.source_cpu,
        source_sequence: event.seq,
        event_type: event_type.name().to_string(),
        kind: event.kind.clone(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregateProjection {
    pub source_counts: BTreeMap<u32, u64>,
    pub event_type_counts: BTreeMap<String, u64>,
    pub windows: Vec<BTreeMap<String, u64>>,
}

pub fn aggregate_projection(
    events: &[TraceEvent],
    window_events: u64,
) -> EvidenceResult<AggregateProjection> {
    if window_events == 0 {
        return Err(EvidenceError::new(
            "aggregate-window",
            "aggregate window must be positive",
        ));
    }
    let window_size = usize::try_from(window_events).map_err(|_| {
        EvidenceError::new(
            "aggregate-window",
            "aggregate window does not fit the current platform",
        )
    })?;
    let mut source_counts = BTreeMap::new();
    let mut event_type_counts = BTreeMap::new();
    let mut canonical = Vec::with_capacity(events.len());
    for event in events {
        let projected = canonical_event(event)?;
        *source_counts.entry(projected.source_cpu).or_insert(0) += 1;
        *event_type_counts
            .entry(projected.event_type.clone())
            .or_insert(0) += 1;
        canonical.push(projected);
    }
    canonical.sort_by_key(|event| (event.source_cpu, event.source_sequence));
    let windows = canonical
        .chunks(window_size)
        .map(|window| {
            let mut counts = BTreeMap::new();
            for event in window {
                *counts.entry(event.event_type.clone()).or_insert(0) += 1;
            }
            counts
        })
        .collect();
    Ok(AggregateProjection {
        source_counts,
        event_type_counts,
        windows,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComparisonStatus {
    Match,
    Divergent,
    Partial,
    Incompatible,
    Unsupported,
    Blocked,
    CleanupFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceComparison {
    pub status: ComparisonStatus,
    pub mode: OrderingMode,
    pub matching_observations: u64,
    pub first_divergence: Option<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct TraceComparisonInput<'a> {
    pub profile: &'a AdmittedCaptureProfile,
    pub accounting: &'a AccountingReport,
    pub cleanup: &'a CleanupOutcome,
    pub events: &'a [TraceEvent],
}

pub fn compare_complete_traces(
    trace_a: TraceComparisonInput<'_>,
    trace_b: TraceComparisonInput<'_>,
) -> EvidenceResult<TraceComparison> {
    let TraceComparisonInput {
        profile: profile_a,
        accounting: accounting_a,
        cleanup: cleanup_a,
        events: events_a,
    } = trace_a;
    let TraceComparisonInput {
        profile: profile_b,
        accounting: accounting_b,
        cleanup: cleanup_b,
        events: events_b,
    } = trace_b;
    if profile_a.profile_ref != profile_b.profile_ref {
        return Ok(non_pass_comparison(
            profile_a.profile.ordering_mode,
            ComparisonStatus::Incompatible,
            "capture profile identities differ",
        ));
    }
    let profile = &profile_a.profile;
    if !cleanup_complete(cleanup_a) || !cleanup_complete(cleanup_b) {
        return Ok(non_pass_comparison(
            profile.ordering_mode,
            ComparisonStatus::CleanupFailed,
            "one or both captures lack explicit successful cleanup",
        ));
    }
    let accounting_statuses = [accounting_a.status, accounting_b.status];
    if accounting_statuses.contains(&CompletenessStatus::Failed) {
        return Ok(non_pass_comparison(
            profile.ordering_mode,
            ComparisonStatus::Blocked,
            "one or both captures have failed accounting",
        ));
    }
    if accounting_statuses.contains(&CompletenessStatus::Unsupported) {
        return Ok(non_pass_comparison(
            profile.ordering_mode,
            ComparisonStatus::Unsupported,
            "one or both captures lack supported accounting",
        ));
    }
    if accounting_statuses.contains(&CompletenessStatus::Partial) {
        return Ok(non_pass_comparison(
            profile.ordering_mode,
            ComparisonStatus::Partial,
            "one or both captures are partial",
        ));
    }
    let event_count_a = u64::try_from(events_a.len()).unwrap_or(u64::MAX);
    let event_count_b = u64::try_from(events_b.len()).unwrap_or(u64::MAX);
    if accounting_a.accepted_records != event_count_a
        || accounting_b.accepted_records != event_count_b
    {
        return Ok(non_pass_comparison(
            profile.ordering_mode,
            ComparisonStatus::Partial,
            "accepted accounting does not equal retained event count",
        ));
    }
    let sequence_a = validate_source_sequences(events_a);
    let sequence_b = validate_source_sequences(events_b);
    if !sequence_a.continuous || !sequence_b.continuous {
        return Ok(non_pass_comparison(
            profile.ordering_mode,
            ComparisonStatus::Partial,
            "source-local sequence is not continuous",
        ));
    }
    if u64::try_from(events_a.len()).unwrap_or(u64::MAX) > profile.bounds.maximum_events
        || u64::try_from(events_b.len()).unwrap_or(u64::MAX) > profile.bounds.maximum_events
    {
        return Err(EvidenceError::new(
            "comparison-bound",
            "trace events exceed maximum_events",
        ));
    }
    match profile.ordering_mode {
        OrderingMode::ExactSingleProducer => compare_exact(profile, events_a, events_b),
        OrderingMode::SourcePartialOrder => compare_partial(profile, events_a, events_b),
        OrderingMode::Aggregate => compare_aggregate(profile, events_a, events_b),
    }
}

fn compare_exact(
    profile: &CaptureProfile,
    events_a: &[TraceEvent],
    events_b: &[TraceEvent],
) -> EvidenceResult<TraceComparison> {
    if profile.topology.producer_count != 1 {
        return Ok(non_pass_comparison(
            OrderingMode::ExactSingleProducer,
            ComparisonStatus::Unsupported,
            "exact order is unsupported for multi-producer topology",
        ));
    }
    compare_canonical_streams(
        OrderingMode::ExactSingleProducer,
        canonical_stream(events_a)?,
        canonical_stream(events_b)?,
    )
}

fn compare_partial(
    _profile: &CaptureProfile,
    events_a: &[TraceEvent],
    events_b: &[TraceEvent],
) -> EvidenceResult<TraceComparison> {
    compare_canonical_streams(
        OrderingMode::SourcePartialOrder,
        canonical_stream(events_a)?,
        canonical_stream(events_b)?,
    )
}

fn canonical_stream(events: &[TraceEvent]) -> EvidenceResult<Vec<CanonicalEvent>> {
    let mut projected = events
        .iter()
        .map(canonical_event)
        .collect::<EvidenceResult<Vec<_>>>()?;
    projected.sort_by_key(|event| (event.source_cpu, event.source_sequence));
    Ok(projected)
}

fn compare_canonical_streams(
    mode: OrderingMode,
    left: Vec<CanonicalEvent>,
    right: Vec<CanonicalEvent>,
) -> EvidenceResult<TraceComparison> {
    let common = left.len().min(right.len());
    for index in 0..common {
        if left[index] != right[index] {
            return Ok(TraceComparison {
                status: ComparisonStatus::Divergent,
                mode,
                matching_observations: u64::try_from(index).unwrap_or(u64::MAX),
                first_divergence: Some(format!(
                    "canonical source-local observation differs at index {index}"
                )),
                blockers: Vec::new(),
            });
        }
    }
    if left.len() != right.len() {
        return Ok(TraceComparison {
            status: ComparisonStatus::Divergent,
            mode,
            matching_observations: u64::try_from(common).unwrap_or(u64::MAX),
            first_divergence: Some("canonical source-local lengths differ".to_string()),
            blockers: Vec::new(),
        });
    }
    Ok(TraceComparison {
        status: ComparisonStatus::Match,
        mode,
        matching_observations: u64::try_from(common).unwrap_or(u64::MAX),
        first_divergence: None,
        blockers: Vec::new(),
    })
}

fn compare_aggregate(
    profile: &CaptureProfile,
    events_a: &[TraceEvent],
    events_b: &[TraceEvent],
) -> EvidenceResult<TraceComparison> {
    let left = aggregate_projection(events_a, profile.bounds.aggregate_window_events)?;
    let right = aggregate_projection(events_b, profile.bounds.aggregate_window_events)?;
    Ok(if left == right {
        TraceComparison {
            status: ComparisonStatus::Match,
            mode: OrderingMode::Aggregate,
            matching_observations: u64::try_from(events_a.len()).unwrap_or(u64::MAX),
            first_divergence: None,
            blockers: Vec::new(),
        }
    } else {
        TraceComparison {
            status: ComparisonStatus::Divergent,
            mode: OrderingMode::Aggregate,
            matching_observations: 0,
            first_divergence: Some("bounded aggregate projection differs".to_string()),
            blockers: Vec::new(),
        }
    })
}

fn non_pass_comparison(
    mode: OrderingMode,
    status: ComparisonStatus,
    blocker: &str,
) -> TraceComparison {
    TraceComparison {
        status,
        mode,
        matching_observations: 0,
        first_divergence: None,
        blockers: vec![blocker.to_string()],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentity {
    pub reference: String,
    pub bytes: u64,
}

pub fn trace_event_artifact(events: &[TraceEvent]) -> EvidenceResult<ArtifactIdentity> {
    artifact_identity(EVENT_ARTIFACT_IDENTITY_DOMAIN, events)
}

pub fn trace_aggregate_artifact(
    projection: &AggregateProjection,
) -> EvidenceResult<ArtifactIdentity> {
    artifact_identity(AGGREGATE_ARTIFACT_IDENTITY_DOMAIN, projection)
}

fn artifact_identity<T: Serialize + ?Sized>(
    domain: &[u8],
    value: &T,
) -> EvidenceResult<ArtifactIdentity> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        EvidenceError::new(
            "artifact-serialization",
            format!("cannot serialize trace artifact: {error}"),
        )
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&length.to_be_bytes());
    hasher.update(&bytes);
    Ok(ArtifactIdentity {
        reference: format!("{DIGEST_PREFIX}{}", hasher.finalize().to_hex()),
        bytes: length,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceManifest {
    pub schema: String,
    pub profile_ref: String,
    pub build: BuildIdentity,
    pub runtime: RuntimeCohort,
    pub admission: RuntimeAdmission,
    pub target: TargetIdentity,
    pub start_target: TargetObservation,
    pub end_target: TargetObservation,
    pub target_binding: TargetBindingReport,
    pub topology: ProducerTopology,
    pub bounds: CaptureBounds,
    pub producer_accounting: ProducerAccounting,
    pub userspace_accounting: UserspaceAccounting,
    pub accounting: AccountingReport,
    pub sequence: SequenceReport,
    pub ordering_mode: OrderingMode,
    pub event_artifact_ref: String,
    pub event_artifact_bytes: u64,
    pub aggregate_artifact_ref: Option<String>,
    pub aggregate_artifact_bytes: Option<u64>,
    pub start_boundary_ref: String,
    pub end_boundary_ref: String,
    pub cleanup: CleanupOutcome,
    pub terminal_class: TerminalClass,
    pub non_claims: Vec<String>,
}

pub fn trace_manifest_ref(manifest: &TraceManifest) -> EvidenceResult<String> {
    validate_trace_manifest(manifest)?;
    canonical_ref(TRACE_IDENTITY_DOMAIN, manifest)
}

pub fn validate_trace_manifest(manifest: &TraceManifest) -> EvidenceResult<()> {
    if manifest.schema != TRACE_MANIFEST_SCHEMA {
        return Err(EvidenceError::new(
            "manifest-schema",
            "unsupported trace manifest schema",
        ));
    }
    for (name, reference) in [
        ("profile_ref", &manifest.profile_ref),
        ("event_artifact_ref", &manifest.event_artifact_ref),
        ("start_boundary_ref", &manifest.start_boundary_ref),
        ("end_boundary_ref", &manifest.end_boundary_ref),
    ] {
        validate_digest_ref(name, reference)?;
    }
    if manifest.event_artifact_bytes == 0
        || manifest.event_artifact_bytes > manifest.bounds.maximum_artifact_bytes
    {
        return Err(EvidenceError::new(
            "manifest-artifact-bound",
            "event artifact size is zero or over maximum_artifact_bytes",
        ));
    }
    match (
        &manifest.aggregate_artifact_ref,
        manifest.aggregate_artifact_bytes,
    ) {
        (Some(reference), Some(bytes)) => {
            validate_digest_ref("aggregate_artifact_ref", reference)?;
            if bytes == 0 || bytes > manifest.bounds.maximum_artifact_bytes {
                return Err(EvidenceError::new(
                    "manifest-artifact-bound",
                    "aggregate artifact size is zero or over maximum_artifact_bytes",
                ));
            }
        }
        (None, None) => {}
        _ => {
            return Err(EvidenceError::new(
                "manifest-artifact-shape",
                "aggregate artifact identity and size must be present together",
            ));
        }
    }
    validate_build_identity(&manifest.build)?;
    validate_runtime_cohort(&manifest.runtime)?;
    validate_target(&manifest.target)?;
    let target_binding = assess_target_binding(
        &manifest.target,
        &manifest.start_target,
        &manifest.end_target,
    );
    if target_binding != manifest.target_binding {
        return Err(EvidenceError::new(
            "manifest-target",
            "retained target binding report differs from boundary observations",
        ));
    }
    validate_topology(&manifest.topology)?;
    manifest.bounds.validate()?;
    if manifest.ordering_mode == OrderingMode::Aggregate
        && manifest.aggregate_artifact_ref.is_none()
    {
        return Err(EvidenceError::new(
            "manifest-aggregate",
            "aggregate ordering requires a retained aggregate artifact",
        ));
    }
    let reconciled = reconcile_accounting(
        &manifest.producer_accounting,
        &manifest.userspace_accounting,
        &manifest.bounds,
    )?;
    if reconciled != manifest.accounting {
        return Err(EvidenceError::new(
            "manifest-accounting",
            "retained accounting report does not match retained raw counters",
        ));
    }
    if manifest.ordering_mode == OrderingMode::ExactSingleProducer
        && (manifest.topology.producer_count != 1 || manifest.topology.affinity_cpus.len() != 1)
    {
        return Err(EvidenceError::new(
            "manifest-ordering",
            "exact manifest ordering requires one producer and one affinity CPU",
        ));
    }
    let observed_sources: BTreeSet<_> = manifest
        .sequence
        .source_event_counts
        .keys()
        .copied()
        .collect();
    let accounted_sources: BTreeSet<_> = manifest
        .producer_accounting
        .sources
        .iter()
        .filter(|source| source.eligible_attempts != 0)
        .map(|source| source.source_cpu)
        .collect();
    if observed_sources.len()
        > usize::try_from(manifest.topology.producer_count).unwrap_or(usize::MAX)
        || (!manifest.topology.affinity_cpus.is_empty()
            && !observed_sources
                .iter()
                .all(|source| manifest.topology.affinity_cpus.contains(source)))
        || (manifest.accounting.status == CompletenessStatus::Complete
            && observed_sources != accounted_sources)
    {
        return Err(EvidenceError::new(
            "manifest-source",
            "observed, accounted, affinity, and declared producer sources do not reconcile",
        ));
    }
    let classified = classify_capture_terminal(
        &manifest.admission,
        &manifest.target_binding,
        &manifest.accounting,
        &manifest.sequence,
        &manifest.cleanup,
    );
    if manifest.terminal_class != classified {
        return Err(EvidenceError::new(
            "manifest-terminal",
            "terminal class does not match admission, accounting, sequence, and cleanup facts",
        ));
    }
    if manifest.terminal_class == TerminalClass::Complete
        && (manifest.admission.status != AdmissionStatus::Accepted
            || manifest.target_binding.status != TargetBindingStatus::Stable
            || manifest.accounting.status != CompletenessStatus::Complete
            || !manifest.sequence.continuous
            || !cleanup_complete(&manifest.cleanup))
    {
        return Err(EvidenceError::new(
            "manifest-completeness",
            "complete manifest requires complete accounting and cleanup",
        ));
    }
    for required in REQUIRED_NON_CLAIMS {
        if !manifest.non_claims.iter().any(|claim| claim == required) {
            return Err(EvidenceError::new(
                "manifest-overclaim",
                format!("trace manifest omits required non-claim: {required}"),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonReceipt {
    pub schema: String,
    pub trace_a_ref: String,
    pub trace_b_ref: String,
    pub profile_ref: String,
    pub comparison: TraceComparison,
    pub non_claims: Vec<String>,
}

pub fn validate_trace_capture(
    profile: &AdmittedCaptureProfile,
    manifest: &TraceManifest,
    events: &[TraceEvent],
) -> EvidenceResult<TerminalClass> {
    validate_manifest_against_profile(profile, manifest)?;
    validate_enabled_events(profile, events)?;
    validate_manifest_events(manifest, events)?;
    Ok(manifest.terminal_class)
}

pub fn compare_trace_manifests(
    profile: &AdmittedCaptureProfile,
    manifest_a: &TraceManifest,
    manifest_b: &TraceManifest,
    events_a: &[TraceEvent],
    events_b: &[TraceEvent],
) -> EvidenceResult<ComparisonReceipt> {
    validate_manifest_against_profile(profile, manifest_a)?;
    validate_manifest_against_profile(profile, manifest_b)?;
    validate_enabled_events(profile, events_a)?;
    validate_enabled_events(profile, events_b)?;
    validate_manifest_events(manifest_a, events_a)?;
    validate_manifest_events(manifest_b, events_b)?;
    let comparison_status =
        manifest_terminal_comparison_status(manifest_a.terminal_class, manifest_b.terminal_class);
    let comparison = if let Some(status) = comparison_status {
        TraceComparison {
            status,
            mode: profile.profile.ordering_mode,
            matching_observations: 0,
            first_divergence: None,
            blockers: vec!["one or both manifests are not complete".to_string()],
        }
    } else {
        compare_complete_traces(
            TraceComparisonInput {
                profile,
                accounting: &manifest_a.accounting,
                cleanup: &manifest_a.cleanup,
                events: events_a,
            },
            TraceComparisonInput {
                profile,
                accounting: &manifest_b.accounting,
                cleanup: &manifest_b.cleanup,
                events: events_b,
            },
        )?
    };
    Ok(ComparisonReceipt {
        schema: COMPARISON_RECEIPT_SCHEMA.to_string(),
        trace_a_ref: trace_manifest_ref(manifest_a)?,
        trace_b_ref: trace_manifest_ref(manifest_b)?,
        profile_ref: profile.profile_ref.clone(),
        comparison,
        non_claims: REQUIRED_NON_CLAIMS
            .iter()
            .map(|claim| (*claim).to_string())
            .collect(),
    })
}

fn manifest_terminal_comparison_status(
    terminal_a: TerminalClass,
    terminal_b: TerminalClass,
) -> Option<ComparisonStatus> {
    let terminals = [terminal_a, terminal_b];
    if terminals.contains(&TerminalClass::CleanupFailed) {
        Some(ComparisonStatus::CleanupFailed)
    } else if terminals.contains(&TerminalClass::Blocked) {
        Some(ComparisonStatus::Blocked)
    } else if terminals.contains(&TerminalClass::Unsupported) {
        Some(ComparisonStatus::Unsupported)
    } else if terminals.contains(&TerminalClass::Incompatible) {
        Some(ComparisonStatus::Incompatible)
    } else if terminals.contains(&TerminalClass::Divergent) {
        Some(ComparisonStatus::Divergent)
    } else if terminals.contains(&TerminalClass::Partial) {
        Some(ComparisonStatus::Partial)
    } else {
        None
    }
}

pub fn verify_comparison_receipt(
    receipt: &ComparisonReceipt,
    profile: &AdmittedCaptureProfile,
    manifest_a: &TraceManifest,
    manifest_b: &TraceManifest,
    events_a: &[TraceEvent],
    events_b: &[TraceEvent],
) -> EvidenceResult<()> {
    comparison_receipt_ref(receipt)?;
    let expected = compare_trace_manifests(profile, manifest_a, manifest_b, events_a, events_b)?;
    if *receipt != expected {
        return Err(EvidenceError::new(
            "comparison-drift",
            "comparison receipt differs from recomputed manifests and event artifacts",
        ));
    }
    Ok(())
}

fn validate_enabled_events(
    profile: &AdmittedCaptureProfile,
    events: &[TraceEvent],
) -> EvidenceResult<()> {
    for event in events {
        let event_type = event.event_type() as u32;
        if !profile.profile.enabled_event_types.contains(&event_type) {
            return Err(EvidenceError::new(
                "event-filter-drift",
                format!("retained event type {event_type} was not enabled by the profile"),
            ));
        }
    }
    Ok(())
}

fn validate_manifest_events(manifest: &TraceManifest, events: &[TraceEvent]) -> EvidenceResult<()> {
    let event_artifact = trace_event_artifact(events)?;
    if event_artifact.reference != manifest.event_artifact_ref
        || event_artifact.bytes != manifest.event_artifact_bytes
        || validate_source_sequences(events) != manifest.sequence
    {
        return Err(EvidenceError::new(
            "manifest-event-drift",
            "retained event artifact identity, size, or sequence report differs from events",
        ));
    }
    if manifest.ordering_mode == OrderingMode::Aggregate {
        let projection = aggregate_projection(events, manifest.bounds.aggregate_window_events)?;
        let aggregate_artifact = trace_aggregate_artifact(&projection)?;
        if Some(&aggregate_artifact.reference) != manifest.aggregate_artifact_ref.as_ref()
            || Some(aggregate_artifact.bytes) != manifest.aggregate_artifact_bytes
        {
            return Err(EvidenceError::new(
                "manifest-aggregate-drift",
                "retained aggregate identity or size differs from recomputed projection",
            ));
        }
    }
    Ok(())
}

fn validate_manifest_against_profile(
    profile: &AdmittedCaptureProfile,
    manifest: &TraceManifest,
) -> EvidenceResult<()> {
    validate_trace_manifest(manifest)?;
    if manifest.profile_ref != profile.profile_ref
        || manifest.build != profile.profile.build
        || manifest.target != profile.profile.target
        || manifest.topology != profile.profile.topology
        || manifest.bounds != profile.profile.bounds
        || manifest.ordering_mode != profile.profile.ordering_mode
    {
        return Err(EvidenceError::new(
            "manifest-profile-drift",
            "manifest facts differ from the admitted capture profile",
        ));
    }
    let expected_admission = admit_runtime_cohort(
        profile,
        &manifest.runtime,
        &manifest.build.bpf_object_ref,
        &manifest.build.loader_ref,
    )?;
    if expected_admission != manifest.admission {
        return Err(EvidenceError::new(
            "manifest-admission-drift",
            "manifest admission differs from recomputed runtime admission",
        ));
    }
    Ok(())
}

pub fn comparison_receipt_ref(receipt: &ComparisonReceipt) -> EvidenceResult<String> {
    if receipt.schema != COMPARISON_RECEIPT_SCHEMA {
        return Err(EvidenceError::new(
            "comparison-schema",
            "unsupported comparison receipt schema",
        ));
    }
    for (name, reference) in [
        ("trace_a_ref", &receipt.trace_a_ref),
        ("trace_b_ref", &receipt.trace_b_ref),
        ("profile_ref", &receipt.profile_ref),
    ] {
        validate_digest_ref(name, reference)?;
    }
    for required in REQUIRED_NON_CLAIMS {
        if !receipt.non_claims.iter().any(|claim| claim == required) {
            return Err(EvidenceError::new(
                "comparison-overclaim",
                format!("comparison receipt omits required non-claim: {required}"),
            ));
        }
    }
    canonical_ref(COMPARISON_IDENTITY_DOMAIN, receipt)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivilegedPrerequisites {
    pub root_capability: bool,
    pub kvm: bool,
    pub btf: bool,
    pub tracepoints: bool,
    pub pinned_loader: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivilegedPreflight {
    pub status: AdmissionStatus,
    pub missing: Vec<String>,
    pub remediation: Vec<String>,
}

pub fn privileged_preflight(prerequisites: &PrivilegedPrerequisites) -> PrivilegedPreflight {
    let checks = [
        (
            prerequisites.root_capability,
            "root capability",
            "run the privileged lane with the declared BPF capability",
        ),
        (
            prerequisites.kvm,
            "KVM",
            "provide the supported /dev/kvm host",
        ),
        (
            prerequisites.btf,
            "BTF",
            "provide the exact running-kernel BTF artifact",
        ),
        (
            prerequisites.tracepoints,
            "KVM tracepoints",
            "enable every required KVM tracepoint",
        ),
        (
            prerequisites.pinned_loader,
            "pinned loader",
            "use the profile-bound loader artifact",
        ),
    ];
    let mut missing = Vec::new();
    let mut remediation = Vec::new();
    for (available, name, fix) in checks {
        if !available {
            missing.push(name.to_string());
            remediation.push(fix.to_string());
        }
    }
    PrivilegedPreflight {
        status: if missing.is_empty() {
            AdmissionStatus::Accepted
        } else {
            AdmissionStatus::Blocked
        },
        missing,
        remediation,
    }
}

pub fn source_conformance_guard(collector_source: &str, bpf_source: &str) -> EvidenceResult<()> {
    let forbidden = ["Box::leak", "std::mem::transmute"];
    for marker in forbidden {
        if collector_source.contains(marker) {
            return Err(EvidenceError::new(
                "source-ownership",
                format!("collector source contains forbidden ownership marker {marker}"),
            ));
        }
    }
    for marker in [
        "eligible_attempts",
        "submitted_records",
        "reservation_drops",
        "enabled_event_types",
        "source_cpu",
        "schema_version",
        "record_size",
    ] {
        if !bpf_source.contains(marker) {
            return Err(EvidenceError::new(
                "source-accounting",
                format!("BPF source omits required marker {marker}"),
            ));
        }
    }
    let lifecycle_markers = [
        "cleanup.quiesce",
        "final_drain_attempted",
        "read_producer_accounting",
        "drop(ring_buffer)",
        "drop(skel)",
        "cleanup.detach",
    ];
    let mut previous = 0;
    for marker in lifecycle_markers {
        let position = collector_source.find(marker).ok_or_else(|| {
            EvidenceError::new(
                "source-lifecycle",
                format!("collector source omits required lifecycle marker {marker}"),
            )
        })?;
        if position < previous {
            return Err(EvidenceError::new(
                "source-lifecycle",
                "collector lifecycle markers are not in the required shutdown order",
            ));
        }
        previous = position;
    }
    Ok(())
}

fn canonical_ref<T: Serialize>(domain: &[u8], value: &T) -> EvidenceResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        EvidenceError::new(
            "canonical-serialization",
            format!("cannot serialize canonical evidence value: {error}"),
        )
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&length.to_be_bytes());
    hasher.update(&bytes);
    Ok(format!("{DIGEST_PREFIX}{}", hasher.finalize().to_hex()))
}

fn validate_digest_ref(name: &str, value: &str) -> EvidenceResult<()> {
    let Some(hex) = value.strip_prefix(DIGEST_PREFIX) else {
        return Err(EvidenceError::new(
            "identity-ref",
            format!("{name} must use BLAKE3"),
        ));
    };
    if hex.len() != DIGEST_HEX_LENGTH
        || !hex
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(EvidenceError::new(
            "identity-ref",
            format!("{name} is not canonical lowercase BLAKE3 hex"),
        ));
    }
    Ok(())
}

fn validate_identifier(name: &str, value: &str) -> EvidenceResult<()> {
    if value.is_empty()
        || value.len() > MAXIMUM_IDENTIFIER_BYTES
        || value.bytes().any(|byte| byte.is_ascii_control())
        || value.contains('/')
    {
        return Err(EvidenceError::new(
            "identifier",
            format!("{name} is empty, unbounded, path-shaped, or contains control bytes"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFERENCE: &str =
        "blake3:0000000000000000000000000000000000000000000000000000000000000000";
    const ALTERNATE_REFERENCE: &str =
        "blake3:1111111111111111111111111111111111111111111111111111111111111111";
    const TARGET_TGID: u32 = 4_242;
    const EVENT_LIMIT: u64 = 128;
    const POLL_LIMIT: u64 = 64;
    const RING_BYTES: u64 = COMPILED_RING_BUFFER_BYTES;
    const ARTIFACT_BYTES: u64 = 4_194_304;
    const AGGREGATE_WINDOW: u64 = 16;
    const FIRST_SOURCE: u32 = 2;
    const SECOND_SOURCE: u32 = 3;

    fn field() -> TracepointFieldSignature {
        TracepointFieldSignature {
            field: "exit_reason".to_string(),
            offset: 8,
            size: 4,
            type_name: "unsigned int".to_string(),
        }
    }

    fn layout() -> TracepointLayoutSignature {
        TracepointLayoutSignature {
            tracepoint: "kvm:kvm_exit".to_string(),
            format_ref: REFERENCE.to_string(),
            fields: vec![field()],
        }
    }

    fn profile(mode: OrderingMode, producers: u32) -> CaptureProfile {
        CaptureProfile {
            schema: CAPTURE_PROFILE_SCHEMA.to_string(),
            profile_id: "kvm-trace-test".to_string(),
            build: BuildIdentity {
                bpf_object_ref: REFERENCE.to_string(),
                bpf_source_ref: REFERENCE.to_string(),
                event_schema_ref: REFERENCE.to_string(),
                loader_ref: REFERENCE.to_string(),
                btf_source_ref: REFERENCE.to_string(),
                fallback_types_used: false,
                compiled_layouts: vec![layout()],
            },
            expected_runtime: RuntimeCohort {
                kernel_release: "kernel-test".to_string(),
                architecture: "x86_64".to_string(),
                btf_ref: REFERENCE.to_string(),
                runtime_layouts: vec![layout()],
            },
            target: TargetIdentity {
                run_id: "run-test".to_string(),
                tgid: TARGET_TGID,
                process_start_ref: REFERENCE.to_string(),
                executable_ref: REFERENCE.to_string(),
                vmm_profile_ref: REFERENCE.to_string(),
                cgroup_ref: None,
            },
            topology: ProducerTopology {
                vcpu_count: producers,
                producer_count: producers,
                affinity_cpus: if producers == 1 {
                    vec![FIRST_SOURCE]
                } else {
                    vec![FIRST_SOURCE, SECOND_SOURCE]
                },
            },
            enabled_event_types: vec![EventType::KvmExit as u32],
            filter_semantics: FILTER_SEMANTICS.to_string(),
            ordering_mode: mode,
            bounds: CaptureBounds {
                maximum_ring_bytes: RING_BYTES,
                maximum_queue_events: EVENT_LIMIT,
                maximum_polls: POLL_LIMIT,
                maximum_events: EVENT_LIMIT,
                maximum_artifact_bytes: ARTIFACT_BYTES,
                aggregate_window_events: AGGREGATE_WINDOW,
            },
            retention: "bounded-local".to_string(),
            non_claims: REQUIRED_NON_CLAIMS
                .iter()
                .map(|claim| (*claim).to_string())
                .collect(),
        }
    }

    fn producer(events: u64) -> ProducerAccounting {
        ProducerAccounting {
            available: true,
            sources: vec![SourceProducerCounters {
                source_cpu: FIRST_SOURCE,
                eligible_attempts: events,
                submitted_records: events,
                reservation_drops: 0,
            }],
        }
    }

    fn userspace(events: u64) -> UserspaceAccounting {
        UserspaceAccounting {
            received_records: events,
            accepted_records: events,
            polls: 1,
            final_drain_attempted: true,
            final_drain_succeeded: true,
            ..UserspaceAccounting::default()
        }
    }

    fn accounting(events: u64) -> AccountingReport {
        reconcile_accounting(
            &producer(events),
            &userspace(events),
            &profile(OrderingMode::ExactSingleProducer, 1).bounds,
        )
        .expect("accounting")
    }

    fn cleanup() -> CleanupOutcome {
        CleanupOutcome {
            quiesce: CleanupStepStatus::Succeeded,
            final_poll: CleanupStepStatus::Succeeded,
            accounting_snapshot: CleanupStepStatus::Succeeded,
            detach: CleanupStepStatus::Succeeded,
            unpin: CleanupStepStatus::NotRequired,
            cleanup: CleanupStepStatus::Succeeded,
        }
    }

    struct TestTrace<'a> {
        profile: &'a AdmittedCaptureProfile,
        accounting: AccountingReport,
        cleanup: CleanupOutcome,
        events: &'a [TraceEvent],
    }

    impl<'a> TestTrace<'a> {
        fn complete(profile: &'a AdmittedCaptureProfile, events: &'a [TraceEvent]) -> Self {
            Self {
                profile,
                accounting: accounting(u64::try_from(events.len()).unwrap_or(u64::MAX)),
                cleanup: cleanup(),
                events,
            }
        }

        fn input(&self) -> TraceComparisonInput<'_> {
            TraceComparisonInput {
                profile: self.profile,
                accounting: &self.accounting,
                cleanup: &self.cleanup,
                events: self.events,
            }
        }
    }

    fn event(source_cpu: u32, seq: u64, capture_index: u64, host_ns: u64) -> TraceEvent {
        TraceEvent {
            schema_version: RAW_EVENT_SCHEMA_VERSION,
            source_cpu,
            capture_index,
            seq,
            host_ns,
            pid: TARGET_TGID,
            kind: EventKind::KvmExit {
                reason: 12,
                guest_rip: 0x1_000,
                info1: 0,
                info2: 0,
            },
        }
    }

    fn raw_bytes(event_type: u32, schema_version: u16, record_size: u16) -> Vec<u8> {
        let mut bytes = vec![0_u8; usize::from(RAW_EVENT_SIZE)];
        bytes[16..20].copy_from_slice(&event_type.to_ne_bytes());
        bytes[20..24].copy_from_slice(&TARGET_TGID.to_ne_bytes());
        bytes[24..28].copy_from_slice(&FIRST_SOURCE.to_ne_bytes());
        bytes[28..30].copy_from_slice(&schema_version.to_ne_bytes());
        bytes[30..32].copy_from_slice(&record_size.to_ne_bytes());
        bytes
    }

    #[test]
    fn tracepoint_layout_parser_retains_exact_fields_and_rejects_malformed_input() {
        let text = "name: kvm_exit\nformat:\n\tfield:unsigned short common_type; offset:0; size:2; signed:0;\n\tfield:unsigned int exit_reason; offset:8; size:4; signed:0;\n";
        let parsed = parse_tracepoint_layout("kvm:kvm_exit", text).expect("layout");
        assert_eq!(parsed.fields.len(), 2);
        assert_eq!(parsed.fields[1].field, "exit_reason");
        assert_eq!(parsed.fields[1].offset, 8);
        assert_eq!(parsed.fields[1].size, 4);
        assert!(parsed.format_ref.starts_with(DIGEST_PREFIX));
        assert_eq!(
            parse_tracepoint_layout(
                "kvm:kvm_exit",
                "field:unsigned int exit_reason; offset:no; size:4;",
            )
            .expect_err("malformed")
            .class,
            "layout-format"
        );
        assert_eq!(
            parse_tracepoint_layout("kvm:kvm_exit", "name: kvm_exit")
                .expect_err("empty")
                .class,
            "layout-format"
        );
    }

    #[test]
    fn profile_and_runtime_admission_are_exact_and_fail_closed() {
        let authored: CaptureProfile = serde_json::from_str(include_str!(
            "../../../contracts/evidence/fixtures/valid/ebpf-trace-capture-profile.valid.json"
        ))
        .expect("Nickel-authored JSON profile");
        admit_capture_profile(&authored).expect("Rust admits Nickel-authored profile");

        let admitted =
            admit_capture_profile(&profile(OrderingMode::ExactSingleProducer, 1)).expect("profile");
        let disabled_event = TraceEvent {
            schema_version: RAW_EVENT_SCHEMA_VERSION,
            source_cpu: FIRST_SOURCE,
            capture_index: 0,
            seq: 0,
            host_ns: 0,
            pid: TARGET_TGID,
            kind: EventKind::KvmEntry { vcpu_id: 0, rip: 0 },
        };
        assert_eq!(
            validate_enabled_events(&admitted, &[disabled_event])
                .expect_err("disabled event")
                .class,
            "event-filter-drift"
        );

        let runtime = admit_runtime_cohort(
            &admitted,
            &admitted.profile.expected_runtime,
            REFERENCE,
            REFERENCE,
        )
        .expect("runtime");
        assert_eq!(runtime.status, AdmissionStatus::Accepted);

        let mut fallback = admitted.profile.clone();
        fallback.build.fallback_types_used = true;
        let fallback = admit_capture_profile(&fallback).expect("debug profile");
        assert_eq!(
            admit_runtime_cohort(
                &fallback,
                &fallback.profile.expected_runtime,
                REFERENCE,
                REFERENCE,
            )
            .expect("fallback result")
            .status,
            AdmissionStatus::DebugOnly
        );

        let mut bad_layout = admitted.profile.expected_runtime.clone();
        bad_layout.runtime_layouts[0].fields[0].offset += 1;
        assert_eq!(
            admit_runtime_cohort(&admitted, &bad_layout, REFERENCE, REFERENCE)
                .expect("layout result")
                .status,
            AdmissionStatus::Blocked
        );

        let mut drifted_runtime = admitted.profile.expected_runtime.clone();
        drifted_runtime.kernel_release = "different-kernel".to_string();
        drifted_runtime.btf_ref = ALTERNATE_REFERENCE.to_string();
        let drifted = admit_runtime_cohort(
            &admitted,
            &drifted_runtime,
            ALTERNATE_REFERENCE,
            ALTERNATE_REFERENCE,
        )
        .expect("drift result");
        assert_eq!(drifted.status, AdmissionStatus::Blocked);
        assert!(drifted.blockers.iter().any(|item| item.contains("kernel")));
        assert!(drifted.blockers.iter().any(|item| item.contains("BTF")));
        assert!(drifted.blockers.iter().any(|item| item.contains("object")));
        assert!(drifted.blockers.iter().any(|item| item.contains("loader")));

        let mut missing_layout = admitted.profile.clone();
        missing_layout.enabled_event_types = vec![EventType::KvmEntry as u32];
        assert_eq!(
            admit_capture_profile(&missing_layout)
                .expect_err("missing enabled layout")
                .class,
            "profile-layout"
        );
    }

    #[test]
    fn raw_parser_rejects_malformed_version_size_discriminant_and_target() {
        let valid = raw_bytes(
            EventType::KvmExit as u32,
            RAW_EVENT_SCHEMA_VERSION,
            RAW_EVENT_SIZE,
        );
        assert!(parse_raw_record(&valid, 0, TARGET_TGID, EVENT_LIMIT).is_ok());
        assert_eq!(
            parse_raw_record(&valid[..valid.len() - 1], 0, TARGET_TGID, EVENT_LIMIT)
                .expect_err("short")
                .class(),
            "raw-size"
        );
        assert_eq!(
            parse_raw_record(
                &raw_bytes(EventType::KvmExit as u32, 1, RAW_EVENT_SIZE),
                0,
                TARGET_TGID,
                EVENT_LIMIT,
            )
            .expect_err("version")
            .class(),
            "raw-version"
        );
        assert_eq!(
            parse_raw_record(
                &raw_bytes(
                    EventType::KvmExit as u32,
                    RAW_EVENT_SCHEMA_VERSION,
                    RAW_EVENT_SIZE - 1,
                ),
                0,
                TARGET_TGID,
                EVENT_LIMIT,
            )
            .expect_err("record size")
            .class(),
            "raw-record-size"
        );
        assert_eq!(
            parse_raw_record(
                &raw_bytes(u32::MAX, RAW_EVENT_SCHEMA_VERSION, RAW_EVENT_SIZE),
                0,
                TARGET_TGID,
                EVENT_LIMIT,
            )
            .expect_err("discriminant")
            .class(),
            "raw-discriminant"
        );
        assert_eq!(
            parse_raw_record(&valid, 0, TARGET_TGID + 1, EVENT_LIMIT)
                .expect_err("target")
                .class(),
            "raw-target"
        );
    }

    #[test]
    fn accounting_detects_loss_mismatch_overflow_and_unavailable_counters() {
        assert_eq!(accounting(1).status, CompletenessStatus::Complete);
        let loss = reconcile_accounting(
            &ProducerAccounting {
                available: true,
                sources: vec![SourceProducerCounters {
                    source_cpu: FIRST_SOURCE,
                    eligible_attempts: 2,
                    submitted_records: 1,
                    reservation_drops: 1,
                }],
            },
            &UserspaceAccounting {
                received_records: 1,
                accepted_records: 1,
                polls: 1,
                final_drain_attempted: true,
                final_drain_succeeded: true,
                ..UserspaceAccounting::default()
            },
            &profile(OrderingMode::ExactSingleProducer, 1).bounds,
        )
        .expect("loss");
        assert_eq!(loss.status, CompletenessStatus::Partial);

        let overflow = ProducerAccounting {
            available: true,
            sources: vec![
                SourceProducerCounters {
                    source_cpu: FIRST_SOURCE,
                    eligible_attempts: u64::MAX,
                    submitted_records: 0,
                    reservation_drops: 0,
                },
                SourceProducerCounters {
                    source_cpu: SECOND_SOURCE,
                    eligible_attempts: 1,
                    submitted_records: 0,
                    reservation_drops: 0,
                },
            ],
        };
        assert_eq!(
            reconcile_accounting(
                &overflow,
                &UserspaceAccounting::default(),
                &profile(OrderingMode::SourcePartialOrder, 2).bounds,
            )
            .expect_err("overflow")
            .class,
            "accounting-overflow"
        );
        assert_eq!(
            reconcile_accounting(
                &ProducerAccounting {
                    available: false,
                    sources: Vec::new(),
                },
                &UserspaceAccounting::default(),
                &profile(OrderingMode::ExactSingleProducer, 1).bounds,
            )
            .expect("unavailable")
            .status,
            CompletenessStatus::Unsupported
        );
    }

    #[test]
    fn source_sequences_detect_duplicates_and_gaps() {
        assert!(
            validate_source_sequences(&[
                event(FIRST_SOURCE, 0, 0, 20),
                event(FIRST_SOURCE, 1, 1, 10),
            ])
            .continuous
        );
        assert!(
            !validate_source_sequences(&[
                event(FIRST_SOURCE, 0, 0, 0),
                event(FIRST_SOURCE, 0, 1, 1),
            ])
            .continuous
        );
        assert!(
            !validate_source_sequences(&[
                event(FIRST_SOURCE, 0, 0, 0),
                event(FIRST_SOURCE, 2, 1, 1),
            ])
            .continuous
        );
    }

    #[test]
    fn exact_and_multi_producer_modes_preserve_ordering_limits() {
        let exact_profile = admit_capture_profile(&profile(OrderingMode::ExactSingleProducer, 1))
            .expect("exact profile");
        let left = vec![event(FIRST_SOURCE, 0, 0, 10)];
        let right = vec![event(FIRST_SOURCE, 0, 0, 999)];
        let exact_a = TestTrace::complete(&exact_profile, &left);
        let exact_b = TestTrace::complete(&exact_profile, &right);
        assert_eq!(
            compare_complete_traces(exact_a.input(), exact_b.input())
                .expect("exact")
                .status,
            ComparisonStatus::Match
        );

        let partial_profile = admit_capture_profile(&profile(OrderingMode::SourcePartialOrder, 2))
            .expect("partial profile");
        let interleaved_a = vec![
            event(FIRST_SOURCE, 0, 0, 100),
            event(SECOND_SOURCE, 0, 1, 10),
        ];
        let interleaved_b = vec![
            event(SECOND_SOURCE, 0, 0, 999),
            event(FIRST_SOURCE, 0, 1, 1),
        ];
        let partial_a = TestTrace::complete(&partial_profile, &interleaved_a);
        let partial_b = TestTrace::complete(&partial_profile, &interleaved_b);
        assert_eq!(
            compare_complete_traces(partial_a.input(), partial_b.input())
                .expect("partial")
                .status,
            ComparisonStatus::Match
        );

        let aggregate_profile =
            admit_capture_profile(&profile(OrderingMode::Aggregate, 2)).expect("aggregate profile");
        let aggregate_a = TestTrace::complete(&aggregate_profile, &interleaved_a);
        let aggregate_b = TestTrace::complete(&aggregate_profile, &interleaved_b);
        assert_eq!(
            compare_complete_traces(aggregate_a.input(), aggregate_b.input())
                .expect("aggregate")
                .status,
            ComparisonStatus::Match
        );
        let aggregate_drift = vec![event(FIRST_SOURCE, 0, 0, 1)];
        let aggregate_a = TestTrace::complete(&aggregate_profile, &interleaved_a);
        let aggregate_b = TestTrace::complete(&aggregate_profile, &aggregate_drift);
        assert_eq!(
            compare_complete_traces(aggregate_a.input(), aggregate_b.input())
                .expect("aggregate divergence")
                .status,
            ComparisonStatus::Divergent
        );

        assert_eq!(
            admit_capture_profile(&profile(OrderingMode::ExactSingleProducer, 2))
                .expect_err("multi-producer exact")
                .class,
            "profile-ordering"
        );
    }

    #[test]
    fn incompatible_incomplete_and_cleanup_failed_traces_never_match() {
        let admitted =
            admit_capture_profile(&profile(OrderingMode::ExactSingleProducer, 1)).expect("profile");
        let mut other_profile = admitted.profile.clone();
        other_profile.profile_id = "other-profile".to_string();
        let other = admit_capture_profile(&other_profile).expect("other");
        let events = vec![event(FIRST_SOURCE, 0, 0, 0)];
        let admitted_trace = TestTrace::complete(&admitted, &events);
        let other_trace = TestTrace::complete(&other, &events);
        assert_eq!(
            compare_complete_traces(admitted_trace.input(), other_trace.input())
                .expect("incompatible")
                .status,
            ComparisonStatus::Incompatible
        );

        let mut partial_trace = TestTrace::complete(&admitted, &events);
        partial_trace.accounting.status = CompletenessStatus::Partial;
        let complete_trace = TestTrace::complete(&admitted, &events);
        assert_eq!(
            compare_complete_traces(partial_trace.input(), complete_trace.input())
                .expect("partial")
                .status,
            ComparisonStatus::Partial
        );

        let mut unsupported_trace = TestTrace::complete(&admitted, &events);
        unsupported_trace.accounting.status = CompletenessStatus::Unsupported;
        let complete_trace = TestTrace::complete(&admitted, &events);
        assert_eq!(
            compare_complete_traces(unsupported_trace.input(), complete_trace.input())
                .expect("unsupported")
                .status,
            ComparisonStatus::Unsupported
        );

        let mut blocked_trace = TestTrace::complete(&admitted, &events);
        blocked_trace.accounting.status = CompletenessStatus::Failed;
        let complete_trace = TestTrace::complete(&admitted, &events);
        assert_eq!(
            compare_complete_traces(blocked_trace.input(), complete_trace.input())
                .expect("blocked")
                .status,
            ComparisonStatus::Blocked
        );

        let mut cleanup_trace = TestTrace::complete(&admitted, &events);
        cleanup_trace.cleanup.detach = CleanupStepStatus::Failed;
        let complete_trace = TestTrace::complete(&admitted, &events);
        assert_eq!(
            compare_complete_traces(cleanup_trace.input(), complete_trace.input())
                .expect("cleanup")
                .status,
            ComparisonStatus::CleanupFailed
        );
    }

    #[test]
    fn target_manifest_receipt_and_privileged_preflight_are_claim_scoped() {
        let admitted =
            admit_capture_profile(&profile(OrderingMode::ExactSingleProducer, 1)).expect("profile");
        let observed = TargetObservation {
            run_id: admitted.profile.target.run_id.clone(),
            tgid: TARGET_TGID,
            process_start_ref: REFERENCE.to_string(),
            executable_ref: REFERENCE.to_string(),
            vmm_profile_ref: REFERENCE.to_string(),
            cgroup_ref: None,
            exited: false,
            exec_changed: false,
        };
        validate_target_observation(&admitted.profile.target, &observed).expect("target");
        let mut reused = observed.clone();
        reused.process_start_ref =
            "blake3:1111111111111111111111111111111111111111111111111111111111111111".to_string();
        assert_eq!(
            validate_target_observation(&admitted.profile.target, &reused)
                .expect_err("reuse")
                .class,
            "target-drift"
        );
        assert_eq!(
            assess_target_binding(&admitted.profile.target, &observed, &reused).status,
            TargetBindingStatus::Drifted
        );
        assert_eq!(
            assess_target_binding(&admitted.profile.target, &reused, &observed).status,
            TargetBindingStatus::Blocked
        );
        let mut exec_drift = observed.clone();
        exec_drift.exec_changed = true;
        assert_eq!(
            assess_target_binding(&admitted.profile.target, &observed, &exec_drift).status,
            TargetBindingStatus::Drifted
        );
        let mut exited = observed.clone();
        exited.exited = true;
        assert_eq!(
            assess_target_binding(&admitted.profile.target, &observed, &exited).status,
            TargetBindingStatus::Drifted
        );

        let events = vec![event(FIRST_SOURCE, 0, 0, 0)];
        let event_artifact = trace_event_artifact(&events).expect("event artifact");
        let manifest = TraceManifest {
            schema: TRACE_MANIFEST_SCHEMA.to_string(),
            profile_ref: admitted.profile_ref.clone(),
            build: admitted.profile.build.clone(),
            runtime: admitted.profile.expected_runtime.clone(),
            admission: RuntimeAdmission {
                status: AdmissionStatus::Accepted,
                blockers: Vec::new(),
            },
            target: admitted.profile.target.clone(),
            start_target: observed.clone(),
            end_target: observed.clone(),
            target_binding: assess_target_binding(&admitted.profile.target, &observed, &observed),
            topology: admitted.profile.topology.clone(),
            bounds: admitted.profile.bounds.clone(),
            producer_accounting: producer(1),
            userspace_accounting: userspace(1),
            accounting: accounting(1),
            sequence: validate_source_sequences(&[event(FIRST_SOURCE, 0, 0, 0)]),
            ordering_mode: OrderingMode::ExactSingleProducer,
            event_artifact_ref: event_artifact.reference,
            event_artifact_bytes: event_artifact.bytes,
            aggregate_artifact_ref: None,
            aggregate_artifact_bytes: None,
            start_boundary_ref: REFERENCE.to_string(),
            end_boundary_ref: REFERENCE.to_string(),
            cleanup: cleanup(),
            terminal_class: TerminalClass::Complete,
            non_claims: REQUIRED_NON_CLAIMS
                .iter()
                .map(|claim| (*claim).to_string())
                .collect(),
        };
        let trace_ref = trace_manifest_ref(&manifest).expect("manifest");
        let mut target_partial = manifest.clone();
        target_partial.end_target.exited = true;
        target_partial.target_binding = assess_target_binding(
            &target_partial.target,
            &target_partial.start_target,
            &target_partial.end_target,
        );
        target_partial.terminal_class = TerminalClass::Partial;
        trace_manifest_ref(&target_partial).expect("target partial manifest");
        assert_eq!(
            compare_trace_manifests(&admitted, &target_partial, &manifest, &events, &events,)
                .expect("target partial comparison")
                .comparison
                .status,
            ComparisonStatus::Partial
        );

        let computed = compare_trace_manifests(&admitted, &manifest, &manifest, &events, &events)
            .expect("computed receipt");
        verify_comparison_receipt(&computed, &admitted, &manifest, &manifest, &events, &events)
            .expect("verified receipt");
        let mut drifted = computed;
        drifted.comparison.matching_observations = 0;
        assert_eq!(
            verify_comparison_receipt(&drifted, &admitted, &manifest, &manifest, &events, &events,)
                .expect_err("receipt drift")
                .class,
            "comparison-drift"
        );

        let receipt = ComparisonReceipt {
            schema: COMPARISON_RECEIPT_SCHEMA.to_string(),
            trace_a_ref: trace_ref.clone(),
            trace_b_ref: trace_ref,
            profile_ref: admitted.profile_ref.clone(),
            comparison: TraceComparison {
                status: ComparisonStatus::Match,
                mode: OrderingMode::ExactSingleProducer,
                matching_observations: 1,
                first_divergence: None,
                blockers: Vec::new(),
            },
            non_claims: REQUIRED_NON_CLAIMS
                .iter()
                .map(|claim| (*claim).to_string())
                .collect(),
        };
        comparison_receipt_ref(&receipt).expect("receipt");
        let mut overclaim = receipt;
        overclaim.non_claims.pop();
        assert_eq!(
            comparison_receipt_ref(&overclaim)
                .expect_err("overclaim")
                .class,
            "comparison-overclaim"
        );
        assert_eq!(
            privileged_preflight(&PrivilegedPrerequisites {
                root_capability: false,
                kvm: true,
                btf: false,
                tracepoints: true,
                pinned_loader: true,
            })
            .status,
            AdmissionStatus::Blocked
        );
    }

    #[test]
    fn source_guard_rejects_leaks_and_missing_producer_accounting() {
        let collector = include_str!("collector.rs");
        let good_bpf = "eligible_attempts submitted_records reservation_drops enabled_event_types source_cpu schema_version record_size";
        source_conformance_guard(collector, good_bpf).expect("guard");
        let leaked = format!("{}\n{collector}", ["Box", "::", "leak"].concat());
        assert_eq!(
            source_conformance_guard(&leaked, good_bpf)
                .expect_err("leak")
                .class,
            "source-ownership"
        );
        assert_eq!(
            source_conformance_guard(collector, "source_cpu")
                .expect_err("accounting")
                .class,
            "source-accounting"
        );
    }
}
