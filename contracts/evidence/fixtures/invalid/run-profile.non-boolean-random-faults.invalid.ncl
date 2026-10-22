let Contract = import "../../run-config.ncl" in
let base = import "../../examples/raft-run-config.ncl" in
let exploration = std.record.insert "random_faults" "off" base.exploration in
(std.record.update "exploration" exploration base) | Contract
