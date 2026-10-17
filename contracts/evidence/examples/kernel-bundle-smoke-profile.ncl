# Synthetic authoring fixture only: not producer or guest execution evidence.
let Profile = import "../kernel-bundle-smoke-profile.ncl" in
let digest = "0000000000000000000000000000000000000000000000000000000000000000" in
{
  schema_version = 1,
  role = "kernel-bundle/vm-compat-smoke",
  campaign_id = "kernel-profile-wiring-only",
  onix = {
    architecture = "x86_64", kernel_release = "6.18.20",
    kernel_build_identity = "onix:blake3:kernel-build:%{digest}",
    bundle_identity = "onix:blake3:bundle:%{digest}",
    manifest_identity = "onix:blake3:manifest:%{digest}",
    module_pack_identity = "onix:blake3:module-pack:%{digest}",
    bpf_pack_identity = "onix:blake3:bpf-pack:%{digest}",
  },
  mantle = {
    observation_blake3 = digest, module_blake3 = digest,
    module_object_ref = "mantle://blake3/%{digest}",
    bpf_object_blake3 = digest, bpf_object_ref = "mantle://blake3/%{digest}",
  },
  runner = {
    runner = "wiring-only", runner_receipt_blake3 = digest,
    behavior_status = "passed", evidence_class = "disposable-vm-smoke",
  },
  boot = {
    observed_architecture = "x86_64", observed_kernel_release = "6.18.20",
    readiness_observation = "uname-r-matched",
  },
  module = {
    pack_identity = "onix:blake3:module-pack:%{digest}",
    member_path = "modules/private_kfunc.mod.ko", member_blake3 = digest,
    load_observation = "insmod-private-kfunc-succeeded",
    unload_observation = "rmmod-private-kfunc-attempted", cleanup_class = "succeeded",
  },
  bpf = {
    pack_identity = "onix:blake3:bpf-pack:%{digest}",
    object_path = "bpf/private_kfunc.ebpf.o", object_blake3 = digest,
    section = "xdp", attach = "xdp", attach_target = "lo",
    verifier_observation = "bpftool-prog-load-xdp-succeeded",
    attach_observation = "private-kfunc-loader-attached",
    detach_observation = "private-kfunc-loader-detached",
    cleanup_class = "succeeded", required_kfuncs = ["process_value"],
  },
  bounds = {
    max_boot_seconds = 120, max_module_seconds = 120,
    max_bpf_seconds = 120, max_observations = 16,
  },
  non_claims = [
    "not universal bootability", "not module safety proof", "not eBPF safety proof",
    "not build correctness proof", "not physical readiness", "not snapshot replay",
    "not Onix lifecycle replay", "not security proof", "not release eligibility",
  ],
} | Profile
