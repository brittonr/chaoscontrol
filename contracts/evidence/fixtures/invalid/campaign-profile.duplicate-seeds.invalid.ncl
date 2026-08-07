let Contract = import "../../campaign-profile.ncl" in
let base = import "../../examples/raft-campaign-profile.ncl" in
(std.record.update "seeds" [42, 42] base) | Contract
