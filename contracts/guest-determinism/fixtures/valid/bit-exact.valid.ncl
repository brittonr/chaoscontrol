let Receipt = import "../../drift-receipt.ncl" in
{
  schema = "chaoscontrol.guest-determinism-drift.v1",
  profile_id = "b3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  profile = {
    schema = "chaoscontrol.guest-determinism-profile.v1",
    profile_id = "b3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    input = {
      run_seed = 47, vm_id = 0, vcpu_count = 1, tsc_khz = 3000000,
      clock_mode = "deterministic-jiffies",
    },
    boot_entropy_seed_blake3 = "b3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    layout_binding_blake3 = "b3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
    layout_policy = "linux-randomization-disabled-v1",
    clock_policy = "deterministic-jiffies-from-vmm-pit-v1",
    signal_policy = "serialized-vcpu-schedule-observed-v1",
    required_cmdline = [
      "clocksource=jiffies", "notsc", "nokaslr", "randomize_kstack_offset=off",
      "norandmaps", "random.trust_cpu=off", "random.trust_bootloader=off",
      "nohpet", "kfence.sample_interval=0", "no_hash_pointers",
    ],
    admitted_surfaces = ["boot-entropy", "monotonic-clock", "process-layout", "signal-order"],
    non_claims = [
      "no independent fresh-boot CRNG equality claim",
      "no host-side or cross-machine replay claim",
      "no reads outside the admitted surface list",
      "no arbitrary closed-binary syscall-interception claim",
      "no host signal-timing claim",
    ],
  },
  left_probe_blake3 = "b3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  right_probe_blake3 = "b3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  accepted = true,
  drifted_surfaces = [],
  non_claims = [
    "no independent fresh-boot CRNG equality claim",
    "no host-side or cross-machine replay claim",
    "no reads outside the admitted surface list",
    "no arbitrary closed-binary syscall-interception claim",
    "no host signal-timing claim",
  ],
} | Receipt
