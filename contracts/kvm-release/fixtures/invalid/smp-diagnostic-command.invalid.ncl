let KvmReleaseMatrix = import "../../profile.ncl" in
let matrix = import "../../matrix.ncl" in
let rows = std.array.map (fun row =>
  if row.kind == "deterministic-smp"
  then std.record.update "command" {
    program = "cargo",
    args = ["test", "-p", "chaoscontrol-vmm", "--test", "deterministic_smp_kvm"],
  } row
  else row
) matrix.rows in
(std.record.update "rows" rows matrix | KvmReleaseMatrix)
