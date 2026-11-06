let Contract = import "../../session-limits.ncl" in
let base = import "../../exact-local-v1.ncl" in
(std.record.update "max_decoded_snapshot_bytes" 2147483649 base) | Contract
