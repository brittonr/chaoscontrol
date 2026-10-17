let Contract = import "../../kernel-bundle-smoke-profile.ncl" in
let base = import "../../examples/kernel-bundle-smoke-profile.ncl" in
(std.record.update "non_claims" [] base) | Contract
