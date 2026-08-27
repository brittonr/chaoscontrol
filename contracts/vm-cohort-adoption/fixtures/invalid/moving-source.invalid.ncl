let Adoption = import "../../profile.ncl" in
let valid = import "../../adoption.ncl" in
{ source = { moving_ref = true } } & valid | Adoption
