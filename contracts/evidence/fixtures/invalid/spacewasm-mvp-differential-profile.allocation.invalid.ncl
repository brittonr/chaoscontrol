let Contract = import "../../spacewasm-mvp-differential-profile.ncl" in
let Example = import "../../examples/spacewasm-mvp-differential-profile.json" in
(Example & { bounds.maximum_interpreter_allocation_bytes | force = 16777217 }) | Contract
