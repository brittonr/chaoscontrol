let Contract = import "../../kernel-bundle-smoke-profile.ncl" in
let base = import "../../examples/kernel-bundle-smoke-profile.ncl" in
(std.record.update "boot" (std.record.update "observed_kernel_release" "wrong-release" base.boot) base) | Contract
