# A reply at offset 4096 cannot hold 64 KiB inside a one-page memory.
let Contract = import "../../spacewasm-full-abi-differential-profile.ncl" in
let Example = import "../../examples/spacewasm-full-abi-differential-profile.json" in
(Example & {
  bounds.maximum_hostcall_bytes | force = 65536,
}) | Contract
