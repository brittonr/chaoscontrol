let Contract = import "../../assertion-summary.ncl" in
let base = import "assertions.identity.valid.json" in
std.record.update "distinct_failing_assertions" 4 (
  std.record.update "liveness_gaps" [
    {
      fingerprint = "0303030303030303030303030303030303030303030303030303030303030303",
      kind = "sometimes",
      message = "leader elected",
      hit_count = 2,
    },
    {
      fingerprint = "0404040404040404040404040404040404040404040404040404040404040404",
      kind = "reachable",
      message = "recovery reached",
      hit_count = 0,
    },
  ] base
) | Contract
