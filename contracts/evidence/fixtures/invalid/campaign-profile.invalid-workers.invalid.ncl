let Contract = import "../../campaign-profile.ncl" in
let base = import "../../examples/raft-campaign-profile.ncl" in
let workers = std.record.update "count" 4 base.workers in
(std.record.update "workers" workers base) | Contract
