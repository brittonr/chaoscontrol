# Deterministic lowering of an admitted topology into the existing guest manifest.
let Topology = import "topology.ncl" in
let Manifest = import "../guest-processes/process-manifest.ncl" in
fun source =>
  let topology = source | Topology in
  {
    schema = "chaoscontrol.process-manifest.v1",
    guest = topology.topology_id,
    shared_directories = topology.shared_directories,
    processes = std.array.map (fun service => {
      role = service.role,
      executable = "/services/%{service.role}/root/%{service.entrypoint}",
      arguments = service.arguments,
      environment = service.environment,
      shared_directories = service.shared_directories,
      restart = service.restart,
      instrumented = service.instrumented,
      transport_slot = service.transport_slot,
    }) topology.services,
  } | Manifest
