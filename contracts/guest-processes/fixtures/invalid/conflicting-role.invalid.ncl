let Manifest = import "../../process-manifest.ncl" in
let fixture = import "../valid/cooperating-processes.json" in
(fixture & {
  processes = std.array.map (fun process => process & { role = "writer" }) fixture.processes,
}) | Manifest
