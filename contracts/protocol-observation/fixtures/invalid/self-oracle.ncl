let Profile = import "../../profile.ncl" in
((import "../profile.ncl") & { oracle.authority | force = "runtime-self-report" }) | Profile
