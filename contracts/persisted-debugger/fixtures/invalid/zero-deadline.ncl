let Contract = import "../../session-limits.ncl" in
let base = import "../../exact-local-v1.ncl" in
(std.record.update "host_deadline_ms" 0 base) | Contract
