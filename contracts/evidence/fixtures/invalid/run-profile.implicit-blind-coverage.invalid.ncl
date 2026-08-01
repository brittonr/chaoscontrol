let Contract = import "../../run-config.ncl" in
let base = import "../../examples/raft-run-config.ncl" in
let coverage = { mode = "blind", bitmap_gpa = 4096 } in
(std.record.update "coverage" coverage base) | Contract
