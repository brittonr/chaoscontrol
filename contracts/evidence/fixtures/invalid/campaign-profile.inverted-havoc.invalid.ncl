let Contract = import "../../campaign-profile.ncl" in
let base = import "../../examples/raft-campaign-profile.ncl" in
let mutation = base.mutation
  & { havoc_minimum_mutations = 17, havoc_maximum_mutations = 16 }
in
(std.record.update "mutation" mutation base) | Contract
