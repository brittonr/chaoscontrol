let Contract = import "../../simulator-profile.ncl" in
let base = import "../../examples/register-simulator-profile.ncl" in
(std.record.update "artifacts" [] base) | Contract
