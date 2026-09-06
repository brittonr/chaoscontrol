{
  pkgs,
  filter,
  root,
}:
assert filter "${root}/crates/chaoscontrol-protocol/src/lib.rs" "regular";
assert filter "${root}/contracts/protocol-observation/fixtures/valid.json" "regular";
assert filter "${root}/.pirate/probe.rs" "regular";
assert filter "${root}/.cairnish/probe.rs" "regular";
assert !(filter "${root}/.cairn" "directory");
assert !(filter "${root}/.pi" "directory");
assert !(filter "${root}/.cairn/probe.rs" "regular");
assert !(filter "${root}/.pi/probe.rs" "regular");
pkgs.runCommand "chaoscontrol-source-filter-tests" { } ''touch "$out"''
