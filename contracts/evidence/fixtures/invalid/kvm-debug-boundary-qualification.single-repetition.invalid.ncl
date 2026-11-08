let Contract = import "../../kvm-debug-boundary-qualification.ncl" in
let base = import "../valid/kvm-debug-boundary-qualification.valid.ncl" in
let bounds = std.record.update "identical_repetitions" 1 base.bounds in
(std.record.update "bounds" bounds base) | Contract
