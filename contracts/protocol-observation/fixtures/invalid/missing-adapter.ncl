let Profile = import "../../profile.ncl" in
((import "../profile.ncl") & { oracle.adapter_ref | force = "" }) | Profile
