let SmrWorkloadProfile = import "../smr-workload-profile.ncl" in
{
  schema = "chaoscontrol.smr-workload-profile.v1",
  profile_id = "raft-smr-chain-v1",
  initial_state_ref = "blake3:0000000000000000000000000000000000000000000000000000000000000000",
  observation_mode = "lossless",
  bounds = {
    maximum_commands = 128,
    maximum_command_bytes = 4096,
    maximum_clients = 8,
    maximum_concurrency = 4,
    maximum_virtual_progress = 100000,
    maximum_trace_events = 4096,
    maximum_fault_actions = 256,
    maximum_replay_events = 4096,
    maximum_evidence_bytes = 1048576,
    maximum_reduction_attempts = 512,
  },
  liveness = {
    profile_id = "recovered-quorum-v1",
    required_progress = 1,
    virtual_progress_horizon = 10000,
  },
} | SmrWorkloadProfile
