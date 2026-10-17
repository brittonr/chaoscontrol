let Contract = import "../../kernel-bundle-smoke-profile.ncl" in
let base = import "../../examples/kernel-bundle-smoke-profile.ncl" in
(std.record.update "module" (std.record.update "pack_identity" base.onix.bpf_pack_identity base.module) base) | Contract
