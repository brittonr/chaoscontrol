let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let gap = {
  fingerprint = "0303030303030303030303030303030303030303030303030303030303030303",
  kind = "sometimes",
  message = "leader elected",
  hit_count = 2,
} in
(std.record.update "liveness_gaps" [gap, gap] base) | Contract
