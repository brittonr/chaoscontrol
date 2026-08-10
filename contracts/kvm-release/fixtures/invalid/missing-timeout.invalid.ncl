let KvmReleaseMatrix = import "../../profile.ncl" in
let matrix = import "../../matrix.ncl" in
let rows = std.array.map (fun row =>
  if row.id == "deterministic-smp"
  then { limits = { timeout_seconds = 0 } } & row
  else row
) matrix.rows in
({ rows = rows } & matrix | KvmReleaseMatrix)
