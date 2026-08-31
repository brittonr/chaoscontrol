let Topology = import "../../topology.ncl" in
let fixture = import "../valid/multi-service.json" in
(fixture & {
  services = std.array.map (fun service => service & { role = "database" }) fixture.services,
}) | Topology
