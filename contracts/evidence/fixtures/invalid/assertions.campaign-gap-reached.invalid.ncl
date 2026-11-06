let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
(std.record.update "liveness_gaps" [{
  fingerprint = "0303030303030303030303030303030303030303030303030303030303030303",
  kind = "reachable",
  message = "already reached",
  hit_count = 1,
}] base) | Contract
