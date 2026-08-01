let Contract = import "../../simulator-profile.ncl" in
let base = import "../../examples/register-simulator-profile.ncl" in
let rng = std.record.update "algorithm" "host-random" base.rng in
(std.record.update "rng" rng base) | Contract
