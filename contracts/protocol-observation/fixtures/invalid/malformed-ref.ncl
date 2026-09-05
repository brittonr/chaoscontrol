let Profile = import "../../profile.ncl" in
((import "../profile.ncl") & { protocol_ref | force = "protocol:invalid" }) | Profile
