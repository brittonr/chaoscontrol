let Contract = import "../../run-config.ncl" in
let base = import "../../examples/raft-run-config.ncl" in
let exploration = std.record.update "max_rounds" 0 base.exploration in
(std.record.update "exploration" exploration base) | Contract
