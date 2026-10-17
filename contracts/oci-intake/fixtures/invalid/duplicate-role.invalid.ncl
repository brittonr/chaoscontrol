let Topology = import "../../topology.ncl" in
let fixture = import "../valid/multi-service.json" in
(std.record.update "services" (std.array.map (fun service =>
  std.record.update "role" "database" service
) fixture.services) fixture) | Topology
