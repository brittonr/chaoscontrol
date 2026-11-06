let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
(std.record.update "liveness_gaps" [{
  fingerprint = "0303030303030303030303030303030303030303030303030303030303030303",
  kind = "sometimes",
  message = "not exercised",
  hit_count = 0,
}] base) | Contract
