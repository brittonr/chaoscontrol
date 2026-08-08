# Human-authored bounded eBPF KVM trace capture profile.
# Raw records, producer counters, events, and manifests remain Rust-owned.
let P = import "profile-primitives.ncl" in
let OrderingMode = P.closed_enum [
  "exact-single-producer",
  "source-partial-order",
  "aggregate",
] in
let NonClaim = P.closed_enum [
  "not VM determinism proof",
  "not replay correctness proof",
  "not eBPF safety proof",
  "not kernel correctness proof",
  "not security proof",
  "not physical readiness proof",
  "not release eligibility",
] in
let CompiledRingBytes = 16777216 in
let MaximumQueueEvents = 1000000 in
let MinimumPolls = 2 in
let MaximumPolls = 1000000 in
let MaximumEvents = 1000000 in
let MaximumArtifactBytes = 134217728 in
let MaximumProducers = 4096 in
let MaximumVcpus = 4096 in
let MaximumLayouts = 64 in
let MaximumEnabledEvents = 64 in
let Bounds = {
  maximum_ring_bytes | P.exact_value CompiledRingBytes,
  maximum_queue_events | P.bounded_positive MaximumQueueEvents,
  maximum_polls | P.integer_between MinimumPolls MaximumPolls,
  maximum_events | P.bounded_positive MaximumEvents,
  maximum_artifact_bytes | P.bounded_positive MaximumArtifactBytes,
  aggregate_window_events | P.bounded_positive MaximumEvents,
} in
let LayoutField = {
  field | P.non_empty_string,
  offset | P.u32,
  size | P.positive_u32,
  type_name | P.non_empty_string,
} in
let Layout = {
  tracepoint | P.non_empty_string,
  format_ref | P.blake3,
  fields | Array LayoutField,
} in
let Build = {
  bpf_object_ref | P.blake3,
  bpf_source_ref | P.blake3,
  event_schema_ref | P.blake3,
  loader_ref | P.blake3,
  btf_source_ref | P.blake3,
  fallback_types_used | Bool,
  compiled_layouts | Array Layout,
} in
let Runtime = {
  kernel_release | P.non_empty_string,
  architecture | P.identifier,
  btf_ref | P.blake3,
  runtime_layouts | Array Layout,
} in
let Target = {
  run_id | P.identifier,
  tgid | P.positive_u32,
  process_start_ref | P.blake3,
  executable_ref | P.blake3,
  vmm_profile_ref | P.blake3,
  cgroup_ref | P.blake3 | optional,
} in
let Topology = {
  vcpu_count | P.bounded_positive MaximumVcpus,
  producer_count | P.bounded_positive MaximumProducers,
  affinity_cpus | Array P.u32,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.ebpf-trace-capture-profile.v1",
  profile_id | P.identifier,
  build | Build,
  expected_runtime | Runtime,
  target | Target,
  topology | Topology,
  enabled_event_types | Array P.positive_u32,
  filter_semantics | P.exact_string "exact-tgid-plus-stable-process-identity",
  ordering_mode | OrderingMode,
  bounds | Bounds,
  retention | P.non_empty_string,
  non_claims | Array NonClaim,
} in
let Coherent = fun value =>
  std.array.length value.build.compiled_layouts > 0
  && std.array.length value.build.compiled_layouts <= MaximumLayouts
  && value.build.compiled_layouts == value.expected_runtime.runtime_layouts
  && (value.build.fallback_types_used
    || value.build.btf_source_ref == value.expected_runtime.btf_ref)
  && std.array.all (fun layout =>
    std.array.length layout.fields > 0
    && P.unique_by (fun field => field.field) layout.fields
  ) value.build.compiled_layouts
  && P.unique_by (fun layout => layout.tracepoint) value.build.compiled_layouts
  && std.array.length value.enabled_event_types > 0
  && std.array.length value.enabled_event_types <= MaximumEnabledEvents
  && std.array.length value.non_claims == 7
  && P.unique_by (fun claim => claim) value.non_claims
  && P.unique_by (fun event => event) value.enabled_event_types
  && P.unique_by (fun cpu => cpu) value.topology.affinity_cpus
  && value.topology.producer_count <= value.topology.vcpu_count
  && std.array.length value.topology.affinity_cpus <= value.topology.producer_count
  && value.bounds.aggregate_window_events <= value.bounds.maximum_events
  && value.bounds.maximum_queue_events == value.bounds.maximum_events
  && (value.ordering_mode != "exact-single-producer"
    || (value.topology.producer_count == 1
      && std.array.length value.topology.affinity_cpus == 1))
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
