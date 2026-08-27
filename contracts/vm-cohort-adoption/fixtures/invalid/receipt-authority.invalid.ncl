let Adoption = import "../../profile.ncl" in
let valid = import "../../adoption.ncl" in
{ authority = { release_authority_granted = true } } & valid | Adoption
