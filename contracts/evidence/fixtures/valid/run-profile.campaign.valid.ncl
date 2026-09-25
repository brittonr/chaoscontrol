let Contract = import "../../run-config.ncl" in
let base = import "../../examples/raft-run-config.ncl" in
let exploration = std.record.insert "frontier_policy" "campaign" base.exploration in
(std.record.update "exploration" exploration base) | Contract
