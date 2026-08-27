let Adoption = import "../../profile.ncl" in
let valid = import "../../adoption.ncl" in
{ verification = { cleanup_uncertainty = "unknown" } } & valid | Adoption
