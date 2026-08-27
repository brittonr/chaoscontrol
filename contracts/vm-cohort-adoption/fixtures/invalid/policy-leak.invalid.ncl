let Adoption = import "../../profile.ncl" in
let valid = import "../../adoption.ncl" in
{ authority = { consumer_policy_leak = true } } & valid | Adoption
