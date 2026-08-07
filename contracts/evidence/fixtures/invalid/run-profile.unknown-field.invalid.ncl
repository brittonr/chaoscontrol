let Contract = import "../../run-config.ncl" in
let base = import "../../examples/raft-run-config.ncl" in
(base & { elapsed_seconds = 1 }) | Contract
