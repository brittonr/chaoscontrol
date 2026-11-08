let Contract = import "../../run-config.ncl" in
let base = import "../valid/run-profile.coarse-round-robin.valid.ncl" in
let scheduling = std.record.remove "entry_watchdog_microseconds" base.topology.scheduling in
let topology = std.record.update "scheduling" scheduling base.topology in
(std.record.update "topology" topology base) | Contract
