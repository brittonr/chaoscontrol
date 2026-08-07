let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let identity = std.record.update "fingerprint" "F3417AD3636538C1E5F211E5FAE5ECB2BD96562E33A1C4705518191AFD85FE41" assertion.identity in
let changed = std.record.update "identity" identity assertion in
(std.record.update "assertions" [changed] base) | Contract
