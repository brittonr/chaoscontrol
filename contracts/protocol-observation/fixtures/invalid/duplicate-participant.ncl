let Profile = import "../../profile.ncl" in
let base = import "../profile.ncl" in
(base & { required_participants | force = [std.array.first base.required_participants, std.array.first base.required_participants] }) | Profile
