let Contract = import "../../run-config.ncl" in
let base = import "../valid/run-profile.coarse-round-robin.valid.ncl" in
(std.record.remove "progress_mode" base) | Contract
