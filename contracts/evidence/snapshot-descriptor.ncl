# Rust-owned public exact-snapshot descriptor review contract.
let P = import "profile-primitives.ncl" in
let MaxTextBytes = 256 in
let MaxKvmOperations = 32 in
let MaxMsrIndices = 4096 in
let MaxVcpus = 256 in
let MaxDevices = 64 in
let MaxDeviceQueues = 64 in
let MaxStateOwners = 1024 in
let MaxGuestArtifacts = 16 in
let MaxClosureMembers = 4096 in
let BoundedText = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value > 0
  && std.string.length value <= MaxTextBytes
  && !(std.string.is_match "[[:cntrl:]]" value)
) in
let LowerHexDigest = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value
) in
let BoundedArray = fun maximum contract => std.contract.Sequence [
  std.contract.from_predicate (fun values =>
    std.is_array values
    && std.array.length values > 0
    && std.array.length values <= maximum
  ),
  Array contract,
] in
let TaggedDigest = {
  algorithm | P.closed_enum ["blake3", "sha256"],
  hex | LowerHexDigest,
} in
let ContentIdentity = {
  digest | TaggedDigest,
  length_bytes | P.positive_u64,
  codec | BoundedText,
} in
let NullableContentIdentity = std.contract.custom (fun label value =>
  if value == null then 'Ok value
  else std.contract.check ContentIdentity label value
) in
let RuntimeCohort = {
  runtime_build | TaggedDigest,
  kvm_operations | BoundedArray MaxKvmOperations BoundedText,
  scheduler_profile | BoundedText,
  time_profile | BoundedText,
  entropy_profile | BoundedText,
} in
let DeviceId = std.contract.from_predicate (fun value =>
  value == 1 || value == 2 || value == 4
) in
let DeviceIdentity = {
  base_address | P.u64,
  irq | P.u32,
  device_id | DeviceId,
} in
let DeviceCohort = {
  identity | DeviceIdentity,
  queue_count | P.integer_between 1 MaxDeviceQueues,
  backend | P.closed_enum ["network", "block", "entropy"],
} in
let Topology = {
  vcpu_count | P.integer_between 1 MaxVcpus,
  memory_bytes | P.positive_u64,
  msr_indices | BoundedArray MaxMsrIndices P.u32,
  devices | Array DeviceCohort,
} in
let StateOwner = {
  owner_id | BoundedText,
  schema_id | P.exact_string "chaoscontrol-state-owner-v1",
} in
let GuestArtifact = {
  role | P.closed_enum ["kernel", "initrd", "disk-image", "guest-binary"],
  content | ContentIdentity,
} in
let ClosureMember = {
  order | P.u32,
  role | P.closed_enum ["snapshot-payload", "snapshot-chunk"],
  content | ContentIdentity,
} in
let PayloadClosure = {
  kind | P.closed_enum ["monolithic", "chunked"],
  logical_payload | ContentIdentity,
  manifest | NullableContentIdentity,
  members | BoundedArray MaxClosureMembers ClosureMember,
} in
{
  schema | P.exact_string "chaoscontrol-snapshot-descriptor-v1",
  descriptor_version | P.exact_value 1,
  completeness_profile | P.exact_string "exact-x86-kvm-v1",
  state_schema_version | P.exact_value 2,
  architecture | P.exact_string "x86_64",
  runtime | RuntimeCohort,
  topology | Topology,
  state_owners | BoundedArray MaxStateOwners StateOwner,
  guest_artifacts | BoundedArray MaxGuestArtifacts GuestArtifact,
  payload | PayloadClosure,
}
