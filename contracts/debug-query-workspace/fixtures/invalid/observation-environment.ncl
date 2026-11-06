let base = import "../valid/observation-profile.ncl" in
std.record.update "environment" { LD_PRELOAD = "unadmitted" } base | (import "../../observation-profile.ncl")
