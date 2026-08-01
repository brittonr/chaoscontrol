let Contract = import "../../campaign-profile.ncl" in
let base = import "../../examples/raft-campaign-profile.ncl" in
let scheduling = std.record.update "strategy" "randomized" base.run.topology.scheduling in
let topology = std.record.update "scheduling" scheduling base.run.topology in
let run = std.record.update "topology" topology base.run in
(std.record.update "run" run base) | Contract
