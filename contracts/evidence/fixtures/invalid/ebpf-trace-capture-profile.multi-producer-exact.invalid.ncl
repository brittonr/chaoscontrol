let CaptureProfile = import "../../ebpf-trace-capture-profile.ncl" in
let Ref = "blake3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" in
let Layout = {
  tracepoint = "kvm:kvm_exit",
  format_ref = Ref,
  fields = [{
    field = "exit_reason",
    offset = 8,
    size = 4,
    type_name = "unsigned int",
  }],
} in
{
  schema = "chaoscontrol.ebpf-trace-capture-profile.v1",
  profile_id = "invalid-multi-producer-exact",
  build = {
    bpf_object_ref = Ref,
    bpf_source_ref = Ref,
    event_schema_ref = Ref,
    loader_ref = Ref,
    btf_source_ref = Ref,
    fallback_types_used = false,
    compiled_layouts = [Layout],
  },
  expected_runtime = {
    kernel_release = "6.12.0-onix",
    architecture = "x86_64",
    btf_ref = Ref,
    runtime_layouts = [Layout],
  },
  target = {
    run_id = "invalid-run",
    tgid = 4242,
    process_start_ref = Ref,
    executable_ref = Ref,
    vmm_profile_ref = Ref,
  },
  topology = {
    vcpu_count = 2,
    producer_count = 2,
    affinity_cpus = [2, 3],
  },
  enabled_event_types = [1],
  filter_semantics = "exact-tgid-plus-stable-process-identity",
  ordering_mode = "exact-single-producer",
  bounds = {
    maximum_ring_bytes = 16777216,
    maximum_queue_events = 100000,
    maximum_polls = 100000,
    maximum_events = 100000,
    maximum_artifact_bytes = 16777216,
    aggregate_window_events = 1024,
  },
  retention = "retain-manifest-and-bounded-event-artifact",
  non_claims = [
    "not VM determinism proof",
    "not replay correctness proof",
    "not eBPF safety proof",
    "not kernel correctness proof",
    "not security proof",
    "not physical readiness proof",
    "not release eligibility",
  ],
} | CaptureProfile
