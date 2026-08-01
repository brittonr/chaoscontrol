let Contract = import "../../campaign-profile.ncl" in
let base = import "../../examples/raft-campaign-profile.ncl" in
let metrics = {
  enabled = false,
  output = {
    kind = "relative-artifact",
    path = "metrics.jsonl",
    identity = "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  },
} in
(std.record.update "metrics" metrics base) | Contract
