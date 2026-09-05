let Profile = import "../../profile.ncl" in
((import "../profile.ncl") & { schema | force = "chaoscontrol.protocol-observation-profile.v0" }) | Profile
