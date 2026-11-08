let Contract = import "../../kvm-debug-boundary-qualification.ncl" in
let base = import "../valid/kvm-debug-boundary-qualification.valid.ncl" in
let kernel = std.record.update "identity" "blake3:0000000000000000000000000000000000000000000000000000000000000000" base.runs.round_robin.artifacts.kernel in
let artifacts = std.record.update "kernel" kernel base.runs.round_robin.artifacts in
let rr = std.record.update "artifacts" artifacts base.runs.round_robin in
let random = std.record.update "artifacts" artifacts base.runs.randomized in
(std.record.update "runs" { round_robin = rr, randomized = random } base) | Contract
