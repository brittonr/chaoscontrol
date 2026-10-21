# Supply a reviewed sorted unique repository-relative regular-file inventory.
# Cairn's source/dependencies commands remain the bounded filesystem authority.
# Include the build/check closure and these authoring inputs; exclude producer
# outputs, cancellation controls, build directories, and historical VM blobs.
# Review exclusions against every full flake-check input before accepting it.
let RequiredFiles = [
  ".cargo/config.toml",
  "Cargo.lock",
  "Cargo.toml",
  "flake.lock",
  "flake.nix",
  "crates/chaoscontrol-dashboard/Cargo.toml",
  "crates/chaoscontrol-evidence/Cargo.toml",
  "crates/chaoscontrol-explore/Cargo.toml",
  "crates/chaoscontrol-fault/Cargo.toml",
  "crates/chaoscontrol-guest/Cargo.toml",
  "crates/chaoscontrol-guest-determinism-probe/Cargo.toml",
  "crates/chaoscontrol-guest-net/Cargo.toml",
  "crates/chaoscontrol-net-guest/Cargo.toml",
  "crates/chaoscontrol-property-suite/Cargo.toml",
  "crates/chaoscontrol-protocol/Cargo.toml",
  "crates/chaoscontrol-raft-guest/Cargo.toml",
  "crates/chaoscontrol-redb-guest/Cargo.toml",
  "crates/chaoscontrol-replay/Cargo.toml",
  "crates/chaoscontrol-replay-evidence-core/Cargo.toml",
  "crates/chaoscontrol-rust-workload-guest/Cargo.toml",
  "crates/chaoscontrol-sdk/Cargo.toml",
  "crates/chaoscontrol-sim-core/Cargo.toml",
  "crates/chaoscontrol-smr/Cargo.toml",
  "crates/chaoscontrol-snapshot-descriptor/Cargo.toml",
  "crates/chaoscontrol-trace/Cargo.toml",
  "crates/chaoscontrol-vm-cohort-adapter/Cargo.toml",
  "crates/chaoscontrol-vmm/Cargo.toml",
  "crates/chaoscontrol-wasm-differential/Cargo.toml",
  "crates/chaoscontrol-wasm-exploration-core/Cargo.toml",
  "cairn-policy/default.ncl",
  "cairn-policy/release-readiness-selection.ncl",
  "cairn-policy/generated/cairn-policy.json",
  "contracts/kvm-release/matrix.ncl",
  "contracts/kvm-release/matrix.json",
  "fixtures/agent-completion/registry.ncl",
  "fixtures/agent-completion/inputs.ncl",
  "fixtures/agent-completion/projection.ncl",
] in
let ReviewedFiles = std.contract.from_predicate (fun files =>
  std.array.all (fun path => std.array.elem path files) RequiredFiles
) in
fun raw =>
let files = raw | Array String | ReviewedFiles in
{
  source = files,
  # Locks bind transitive dependency selections; manifests and transport/build
  # declarations bind the maintained source acquisition and integration policy.
  dependencies = std.array.filter (fun path =>
    std.array.elem path [".cargo/config.toml", "Cargo.lock", "Cargo.toml", "flake.lock", "flake.nix"]
    || std.string.is_match "^crates/[^/]+/Cargo[.]toml$" path
  ) files,
}
