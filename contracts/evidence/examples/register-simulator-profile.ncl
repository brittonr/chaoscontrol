let SimulatorProfile = import "../simulator-profile.ncl" in
{
  schema_version = 1,
  run_id = "register-simulator-42",
  seed = 42,
  workload = {
    name = "register-model",
    adapter_version = "v1",
    scenario_id = "single-writer-recovery",
  },
  scheduler = { name = "round-robin-v1", max_steps = 1000 },
  virtual_clock = { start_tick = 0, tick_quantum = 5 },
  rng = { algorithm = "xorshift64-v1", seed_derivation = "config.seed" },
  network = { profile_id = "simulated-network-v1", simulated = true },
  disk = { profile_id = "simulated-disk-v1", simulated = true },
  fault_schedule = {
    schedule_id = "register-faults-v1",
    digest = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  },
  artifacts = [
    {
      name = "adapter",
      digest = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    },
  ],
  scope = "adapter-based in-process simulator evidence; not VM replay proof, not arbitrary binary support, not full FoundationDB parity",
} | SimulatorProfile
