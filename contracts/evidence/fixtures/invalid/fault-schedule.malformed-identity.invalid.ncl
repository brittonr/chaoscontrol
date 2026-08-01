let Contract = import "../../fault-schedule-profile.ncl" in
let base = import "../../examples/raft-fault-schedule-profile.ncl" in
(std.record.update "source_identity" "blake3:BAD" base) | Contract
