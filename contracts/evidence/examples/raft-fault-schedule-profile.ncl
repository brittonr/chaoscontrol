let FaultScheduleProfile = import "../fault-schedule-profile.ncl" in
{
  schema = "chaoscontrol.fault-schedule-profile.v1",
  schedule_id = "raft-partition-recovery-v1",
  source_identity = "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  num_vms = 3,
  faults = [
    {
      kind = "network-partition",
      time_ns = 1000000,
      label = "isolate-third-node",
      side_a = [0, 1],
      side_b = [2],
    },
    {
      kind = "process-kill",
      time_ns = 2000000,
      label = "stop-current-leader",
      target = 0,
    },
  ],
  scope = "finite pre-run fault intent; not attempted, applied, observed, or effective fault evidence",
} | FaultScheduleProfile
