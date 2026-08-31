let Manifest = import "../../process-manifest.ncl" in
let fixture = import "../valid/cooperating-processes.json" in
(fixture & {
  processes = std.array.map (fun process =>
    if process.role == "writer" then process & { shared_directories = ["missing"] }
    else process
  ) fixture.processes,
}) | Manifest
