let Contract = import "../../fault-schedule-profile.ncl" in
let base = import "../../examples/raft-fault-schedule-profile.ncl" in
let bad = {
  kind = "network-partition",
  time_ns = 500000,
  label = null,
  side_a = [0, 1],
  side_b = [1, 2],
} in
(std.record.update "faults" [bad] base) | Contract
