let Topology = import "../../topology.ncl" in
let fixture = import "../valid/multi-service.json" in
(fixture & {
  services = std.array.map (fun service =>
    if service.role == "database" then service & { entrypoint = "../escape" }
    else service
  ) fixture.services,
}) | Topology
