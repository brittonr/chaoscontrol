let Contract = import "../../campaign-profile.ncl" in
let base = import "../../examples/raft-campaign-profile.ncl" in
let run = import "run-profile.coarse-randomized.valid.ncl" in
(std.record.update "run" run base) | Contract
