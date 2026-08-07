let Contract = import "../../run-config.ncl" in
let base = import "../../examples/raft-run-config.ncl" in
let kernel = {
  kind = "relative-artifact",
  path = "../escape/vmlinux",
  identity = "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
} in
let artifacts = std.record.update "kernel" kernel base.artifacts in
(std.record.update "artifacts" artifacts base) | Contract
