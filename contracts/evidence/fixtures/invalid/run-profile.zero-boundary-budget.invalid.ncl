let Contract = import "../../run-config.ncl" in
let base = import "../valid/run-profile.coarse-round-robin.valid.ncl" in
let exploration = std.record.update "progress_budget" { KvmBoundaries = { profile_id = "KvmDebugExitBoundaryV1", count = 0 } } base.exploration in
(std.record.update "exploration" exploration base) | Contract
