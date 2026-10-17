# Authoring boundary for the existing bounded smoke profile. This validates
# profile shape/roles only; canonical upstream artifact admission is separate.
let P = import "profile-primitives.ncl" in
let Hex = std.contract.from_predicate (fun value => std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value) in
let RoleRef = fun role => std.contract.from_predicate (fun value => std.is_string value && std.string.is_match "^onix:blake3:%{role}:[0-9a-f]{64}$" value) in
let MantleRef = std.contract.from_predicate (fun value => std.is_string value && std.string.is_match "^mantle://blake3/[0-9a-f]{64}$" value) in
let PublicToken = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value <= 512
  && std.string.is_match "^[A-Za-z0-9._+-]+$" value && value != "." && value != ".."
) in
let MemberPath = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value <= 512
  && std.string.is_match "^[A-Za-z0-9._+-]+(/[A-Za-z0-9._+-]+)*$" value
  && !(std.string.is_match "(^|/)\\.\\.?($|/)" value)
) in
let NonClaim = P.closed_enum [
  "not universal bootability", "not module safety proof", "not eBPF safety proof",
  "not build correctness proof", "not physical readiness", "not snapshot replay",
  "not Onix lifecycle replay", "not security proof", "not release eligibility",
] in
let Shape = {
  schema_version | P.exact_value 1,
  role | P.exact_string "kernel-bundle/vm-compat-smoke",
  campaign_id | PublicToken,
  onix | {
    architecture | P.exact_string "x86_64",
    kernel_release | PublicToken,
    kernel_build_identity | RoleRef "kernel-build",
    bundle_identity | RoleRef "bundle",
    manifest_identity | RoleRef "manifest",
    module_pack_identity | RoleRef "module-pack",
    bpf_pack_identity | RoleRef "bpf-pack",
  },
  mantle | {
    observation_blake3 | Hex,
    module_blake3 | Hex,
    module_object_ref | MantleRef,
    bpf_object_blake3 | Hex,
    bpf_object_ref | MantleRef,
  },
  runner | {
    runner | PublicToken,
    runner_receipt_blake3 | Hex,
    behavior_status | P.exact_string "passed",
    evidence_class | P.exact_string "disposable-vm-smoke",
  },
  boot | {
    observed_architecture | P.exact_string "x86_64",
    observed_kernel_release | PublicToken,
    readiness_observation | PublicToken,
  },
  module | {
    pack_identity | RoleRef "module-pack",
    member_path | MemberPath,
    member_blake3 | Hex,
    load_observation | PublicToken,
    unload_observation | PublicToken,
    cleanup_class | P.exact_string "succeeded",
  },
  bpf | {
    pack_identity | RoleRef "bpf-pack",
    object_path | MemberPath,
    object_blake3 | Hex,
    section | P.exact_string "xdp",
    attach | P.exact_string "xdp",
    attach_target | PublicToken,
    verifier_observation | PublicToken,
    attach_observation | PublicToken,
    detach_observation | PublicToken,
    cleanup_class | P.exact_string "succeeded",
    required_kfuncs | P.exact_value ["process_value"],
  },
  bounds | {
    max_boot_seconds | P.integer_between 1 600,
    max_module_seconds | P.integer_between 1 600,
    max_bpf_seconds | P.integer_between 1 600,
    max_observations | P.integer_between 12 16,
  },
  non_claims | Array NonClaim,
} in
std.contract.custom (fun label value =>
  std.contract.check Shape label value |> match {
    'Ok checked =>
      if checked.boot.observed_kernel_release == checked.onix.kernel_release
        && checked.module.pack_identity == checked.onix.module_pack_identity
        && checked.bpf.pack_identity == checked.onix.bpf_pack_identity
        && checked.module.member_blake3 == checked.mantle.module_blake3
        && checked.bpf.object_blake3 == checked.mantle.bpf_object_blake3
        && checked.mantle.module_object_ref == "mantle://blake3/%{checked.mantle.module_blake3}"
        && checked.mantle.bpf_object_ref == "mantle://blake3/%{checked.mantle.bpf_object_blake3}"
        && std.array.length checked.non_claims == 9
        && P.unique_by (fun claim => claim) checked.non_claims
      then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
