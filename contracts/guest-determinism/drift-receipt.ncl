# Bounded guest operating-system determinism drift receipt.
let P = import "../evidence/profile-primitives.ncl" in
let B3 = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^b3:[0-9a-f]{64}$" value
) in
let Surface = P.closed_enum [
  "boot-entropy",
  "monotonic-clock",
  "process-layout",
  "signal-order",
] in
let Input = {
  run_seed | P.u64,
  vm_id | P.u32,
  vcpu_count | P.positive_u32,
  tsc_khz | P.positive_u32,
  clock_mode | P.closed_enum ["virtual-tsc", "deterministic-jiffies"],
} in
let Profile = {
  schema | P.exact_string "chaoscontrol.guest-determinism-profile.v1",
  profile_id | B3,
  input | Input,
  boot_entropy_seed_blake3 | B3,
  layout_binding_blake3 | B3,
  layout_policy | P.exact_string "linux-randomization-disabled-v1",
  clock_policy | P.closed_enum [
    "virtual-tsc-reset-before-entry-v1",
    "deterministic-jiffies-from-vmm-pit-v1",
  ],
  signal_policy | P.exact_string "serialized-vcpu-schedule-observed-v1",
  required_cmdline | Array P.non_empty_string,
  admitted_surfaces | Array Surface,
  non_claims | Array P.non_empty_string,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.guest-determinism-drift.v1",
  profile_id | B3,
  profile | Profile,
  left_probe_blake3 | B3,
  right_probe_blake3 | B3,
  accepted | Bool,
  drifted_surfaces | Array Surface,
  non_claims | Array P.non_empty_string,
} in
let Coherent = fun value =>
  value.profile_id == value.profile.profile_id
  && ((value.profile.input.clock_mode == "virtual-tsc"
      && value.profile.clock_policy == "virtual-tsc-reset-before-entry-v1")
    || (value.profile.input.clock_mode == "deterministic-jiffies"
      && value.profile.clock_policy == "deterministic-jiffies-from-vmm-pit-v1"))
  && std.array.length value.profile.required_cmdline == 10
  && std.array.length value.profile.admitted_surfaces == 4
  && std.array.length value.profile.non_claims == 5
  && std.array.length value.drifted_surfaces <= 4
  && P.unique_by (fun surface => surface) value.drifted_surfaces
  && std.array.length value.non_claims == 5
  && P.unique_by (fun claim => claim) value.non_claims
  && (value.accepted == (std.array.length value.drifted_surfaces == 0))
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
