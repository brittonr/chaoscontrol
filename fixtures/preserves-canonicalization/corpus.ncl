# ChaosControl-owned values. Export with the exact Nickel cohort declared by flake.nix.
# Hashes are produced/reviewed with the compatible Cairn preserves-corpus update rail.
# This corpus tests canonicalization, not the truth or availability of runtime evidence.
[
  {
    id = "raft-parent-reference",
    description = "A retained ChaosControl schema-v2 snapshot parent reference; canonicalization only, not a replay or artifact-availability claim.",
    value = {
      replay_parent_depth = 1,
      replay_parent_snapshot_ref = {
        store = "file-content-addressed",
        digest = "sha256:e092ddba0dbec515a03090455c65538f681cf147b4e7b5052cad14ffa9423991",
        codec = "simulation-snapshot-cbor-zstd-v2",
        schema_version = 2,
        path = "snapshots/e092ddba0dbec515a03090455c65538f681cf147b4e7b5052cad14ffa9423991.snapshot.bin",
      },
    },
    expected_hash = "1272c72c1b2c76e5ce1fc255b40d455a953031d382c73139e1d7d9ea55e431f8",
  },
  {
    id = "raft-clock-jump-schedule",
    description = "The retained selected Raft finding's schedule exercises ordered faults, a negative clock delta, and an absent label.",
    value = {
      faults = [
        {
          time_ns = 879110718,
          fault = { ClockJump = { target = 1, delta_ns = -57235731 } },
          label = null,
        },
      ],
    },
    expected_hash = "694826271c5eeb1a0cdd4bd880baa23c23f4de3cb028e51e9d631ff717f407e6",
  },
  {
    id = "operational-watchdog-classification",
    description = "Observed host-watchdog recovery facts preserve the distinction between operational interruption and guest-bug evidence.",
    value = {
      schema = "chaoscontrol.watchdog-recovery-event.v1",
      classification = "operational-host-watchdog-timeout",
      guest_bug = false,
      watchdog_observed = true,
      timer_signal_code = "SI_TIMER",
      replacement_created = true,
      restored_parent_memory_equal = true,
      immediate_incremental_recapture = { dirty_pages = 3605, memory_equal = true },
    },
    expected_hash = "6ded9612cf97427e25cf5a6ee750f6af328303162cf339b5bb239e59cae1a06e",
  },
  {
    id = "bounded-raft-campaign-input",
    description = "Consumer-owned reviewed control/planted campaign bounds, optional disk, and command-line selection; not a campaign success claim.",
    value = {
      schema = "chaoscontrol.production-lineage-campaign-inputs.v1",
      owner = "chaoscontrol-vmm",
      timeout_seconds = 600,
      workers = 1,
      max_rounds = 10,
      max_frontier = 50,
      disk = null,
      guest_cmdlines = ["raft_bug=none", "raft_bug=fig8_commit"],
    },
    expected_hash = "c04f0af824dec697b151c386590139309697f0f1c296df553247fdeac4ad50cf",
  },
]
