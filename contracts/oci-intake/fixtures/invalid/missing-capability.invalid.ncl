let Topology = import "../../topology.ncl" in
let source = import "../valid/multi-service.json" in
(std.record.update "capability_admits" [] source) | Topology
