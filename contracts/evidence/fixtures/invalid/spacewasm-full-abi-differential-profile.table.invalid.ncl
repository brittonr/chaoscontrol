# The ABI admits no table.
let Contract = import "../../spacewasm-full-abi-differential-profile.ncl" in
let Example = import "../../examples/spacewasm-full-abi-differential-profile.json" in
(Example & {
  abi.tables | force = 1,
}) | Contract
