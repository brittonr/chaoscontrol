let Contract = import "../../campaign-profile.ncl" in
let base = import "../valid/campaign-profile.coarse-randomized.valid.ncl" in
(std.record.update "schema" "chaoscontrol.campaign-profile.v1" base) | Contract
