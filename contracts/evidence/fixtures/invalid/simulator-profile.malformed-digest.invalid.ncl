let Contract = import "../../simulator-profile.ncl" in
let base = import "../../examples/register-simulator-profile.ncl" in
let schedule = std.record.update "digest" "sha256:BAD" base.fault_schedule in
(std.record.update "fault_schedule" schedule base) | Contract
