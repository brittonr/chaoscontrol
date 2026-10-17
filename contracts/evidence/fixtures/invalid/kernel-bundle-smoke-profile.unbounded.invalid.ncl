let Contract = import "../../kernel-bundle-smoke-profile.ncl" in
let base = import "../../examples/kernel-bundle-smoke-profile.ncl" in
(std.record.update "bounds" (std.record.update "max_boot_seconds" 0 base.bounds) base) | Contract
