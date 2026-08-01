let Contract = import "../../fault-schedule-profile.ncl" in
let base = import "../../examples/raft-fault-schedule-profile.ncl" in
let bad = { kind = "process-kill", time_ns = 500000, label = null, target = 1 } in
(std.record.update "faults" (base.faults @ [bad]) base) | Contract
