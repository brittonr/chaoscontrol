let Contract = import "../../run-config.ncl" in
let base = import "../valid/run-profile.coarse-round-robin.valid.ncl" in
(std.record.update "guest_cmdline" "clocksource=tsc" base) | Contract
