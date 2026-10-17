let Contract = import "../../kernel-bundle-smoke-profile.ncl" in
let base = import "../../examples/kernel-bundle-smoke-profile.ncl" in
(std.record.update "bpf" (std.record.update "cleanup_class" "failed" base.bpf) base) | Contract
