let Adoption = import "../../profile.ncl" in
let valid = import "../../adoption.ncl" in
{ source = { sibling_path_fallback = true } } & valid | Adoption
