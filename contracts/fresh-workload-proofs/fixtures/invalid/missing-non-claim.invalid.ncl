let FreshWorkloadProofProfile = import "../../profile.ncl" in
let cohort = import "../../cohort.ncl" in
{ non_claims = ["A promoted row does not prove workload correctness."] } & cohort | FreshWorkloadProofProfile
