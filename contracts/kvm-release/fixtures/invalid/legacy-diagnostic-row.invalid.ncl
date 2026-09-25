let KvmReleaseMatrix = import "../../profile.ncl" in
let matrix = import "../../matrix.ncl" in
let rows = std.array.map (fun row =>
  if row.id == "fresh-workload-replay"
  then std.record.update "command" {
    program = row.command.program,
    args = row.command.args @ ["--frontier-policy", "legacy-diagnostic"],
  } row
  else row
) matrix.rows in
(std.record.update "rows" rows matrix | KvmReleaseMatrix)
