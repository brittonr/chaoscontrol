let base = import "../valid/observation-profile.ncl" in
std.record.update "stdout_limit" 65537 base | (import "../../observation-profile.ncl")
