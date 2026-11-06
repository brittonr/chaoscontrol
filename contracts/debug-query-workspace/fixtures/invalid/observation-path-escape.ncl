let base = import "../valid/observation-profile.ncl" in
std.record.update "executable" "../init" base | (import "../../observation-profile.ncl")
