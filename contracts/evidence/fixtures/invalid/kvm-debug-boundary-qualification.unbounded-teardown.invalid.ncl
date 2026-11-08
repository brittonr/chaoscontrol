let Contract = import "../../kvm-debug-boundary-qualification.ncl" in
let base = import "../valid/kvm-debug-boundary-qualification.valid.ncl" in
let bounds = std.record.update "teardown_deadline_ms" 0 base.bounds in
(std.record.update "bounds" bounds base) | Contract
