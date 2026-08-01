let RunProfile = import "../run-config.ncl" in
{
  schema = "chaoscontrol.run-profile.v2",
  profile_id = "raft-vm-exploration",
  mode = "vm-exploration",
  artifacts = {
    kernel = {
      kind = "absolute-path",
      path = "/nix/store/example-chaoscontrol-vmlinux/vmlinux",
      identity = "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    },
    initrd = {
      kind = "absolute-path",
      path = "/nix/store/example-chaoscontrol-initrd-raft/initrd",
      identity = "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    },
    disk = null,
  },
  guest_cmdline = "raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=0",
  seed = 42,
  topology = {
    num_vms = 3,
    num_vcpus = 1,
    memory_mib = 128,
    scheduling = {
      strategy = "round-robin",
      minimum_quantum = 100,
      maximum_quantum = 100,
      diversity = false,
    },
  },
  exploration = {
    mode = "hybrid",
    branch_factor = 2,
    ticks_per_branch = 500,
    max_rounds = 3,
    max_frontier = 10000,
    quantum = 100,
    bootstrap_budget = 10000,
    stale_round_limit = 10,
  },
  coverage = { mode = "blind", bitmap_gpa = 0 },
  logging = {
    raw_log_policy = "debug-only-excluded-from-git",
    determinism_log_policy = "disabled",
    determinism_log_output = null,
    register_interval = 0,
    memory_hash = false,
  },
  scope = "pre-run VM exploration intent; not KVM availability, guest correctness, deterministic replay, fault effect, campaign completion, or accepted evidence",
} | RunProfile
