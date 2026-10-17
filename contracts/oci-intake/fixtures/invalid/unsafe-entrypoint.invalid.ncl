let Topology = import "../../topology.ncl" in
let fixture = import "../valid/multi-service.json" in
(std.record.update "services" (std.array.map (fun service =>
  if service.role == "database" then std.record.update "entrypoint" "../escape" service
  else service
) fixture.services) fixture) | Topology
