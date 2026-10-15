let Blake3Digest = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^blake3:[0-9a-f]{64}$" value
) in
let BoundFile = { path | String, digest | Blake3Digest } in
{
  schema = "chaoscontrol.working-tree-source-binding.v1",
  git_revision = "413713245198b19a80a62f0bcd4498480fd0d984",
  working_tree_clean = false,
  scope = "Changed Rust sources plus Cargo manifest and lockfile at successful proof build task 31. The Git revision identifies unchanged tracked inputs. This is not a whole-working-tree or whole-host digest.",
  files | Array BoundFile = [
    { path = "Cargo.toml", digest = "blake3:bb560f68478df2e4f5dfe2184454ba23a676fcdab5ad6ae76078d4d3fea670cc" },
    { path = "Cargo.lock", digest = "blake3:a753865bde2f4374e36c5cd1a05b1b7233edc5abe1da29ca1c117fbe27c5158d" },
    { path = "crates/chaoscontrol-vmm/src/vm.rs", digest = "blake3:0e91d414e8c7ef1d133c3a7e41b7b1652d1527b1a50a5ad2d9e47c7584bce0e6" },
    { path = "crates/chaoscontrol-vmm/src/vm_core.rs", digest = "blake3:1fb0e21019ceed2e99a3907a44cf3b52d041e3a7959586e13471635c44defbe5" },
    { path = "crates/chaoscontrol-vmm/src/snapshot.rs", digest = "blake3:c1e155b7c801ced7eb91f8596a6ce593a92d4bc69fabaaab04ac021d207f1104" },
    { path = "crates/chaoscontrol-vmm/src/vm/incremental_lineage.rs", digest = "blake3:2916f5edc9f36b32b8c1ffeb16e9450682b5040e0f3a9f57707210f77945ec58" },
    { path = "crates/chaoscontrol-explore/examples/raft_lineage_proof.rs", digest = "blake3:a97d4322b9e5299f4873e2fc9b9ff2df5c1d7f95cc1615c5f25f2aed4c03a11b" },
  ],
  proof_executable | BoundFile = {
    path = "/home/brittonr/.cargo-target/debug/examples/raft_lineage_proof",
    digest = "blake3:cd3dfd04d88edd25407a1c3757404b8dc56e267bc6ef971051cfa5a69e34e993",
  },
  build_command = "nix develop -c cargo build --offline --locked -p chaoscontrol-explore --example raft_lineage_proof",
  pueue_build_task = 31,
  explorer_executable | BoundFile = {
    path = "/home/brittonr/.cargo-target/debug/chaoscontrol-explore",
    digest = "blake3:0487b4e980ee95c0933fdf41b11c963da93725a5a6249f3fc27d746a52283e35",
  },
  explorer_build_task = 18,
  nix_rust_source = "/nix/store/gr3sdn5narncdgdvg30vdqhlm6h2whfh-source",
  nix_source_check = "b3sum confirmed that the Nix-admitted proof example, incremental-lineage regression module, and VM implementation match the corresponding file digests above",
  nix_source_boundary = "The Nix store source is the Cargo/BPF/fixture-filtered source, not a whole-working-tree snapshot. Receipt/configuration validation is recorded separately.",
  rustc = "rustc 1.98.0 (88d9e12ae 2026-08-18)",
  non_claim = "Uncommitted source is intentionally not release-eligible under the required clean-source KVM matrix. No commit or publication was authorized.",
}
