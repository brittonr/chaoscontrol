# ChaosControl-owned completion obligations; not execution evidence.
# Exact portable commands are retained in the clean-gate plan and CI contract.
# KVM/native verdicts remain independently required release obligations.
{
  repository_id = "onixresearch/chaoscontrol",
  registry_id = "chaoscontrol-required-portable-checks-v1",
  authority = ["AGENTS.md", "README.md", ".github/workflows/ci.yml", "docs/kvm-release-evidence.md"],
  required_external_commands = [
    "cargo test --workspace --all-targets --locked --no-fail-fast",
    "cargo clippy --workspace --all-targets --locked -- -D warnings",
    "cargo fmt --all -- --check",
    "nix flake check -L",
  ],
  checks = [
    {
      check_id = "clean-workspace-tests",
      kind = "cargo_workspace_test",
      tool = "cargo",
      args = ["test", "--workspace", "--all-targets", "--locked", "--no-fail-fast"],
      deadline_ms = 3600000,
      stdout_bytes = 8388608,
      stderr_bytes = 8388608,
    },
    {
      check_id = "clean-clippy",
      kind = "cargo_clippy",
      tool = "cargo",
      args = ["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
      deadline_ms = 3600000,
      stdout_bytes = 1048576,
      stderr_bytes = 8388608,
    },
    {
      check_id = "clean-formatting",
      kind = "cargo_fmt",
      tool = "cargo",
      args = ["fmt", "--all", "--", "--check"],
      deadline_ms = 300000,
      stdout_bytes = 1048576,
      stderr_bytes = 1048576,
    },
    {
      check_id = "clean-nix-flake-check",
      kind = "nix_flake_check",
      tool = "nix",
      args = ["flake", "check", "-L"],
      deadline_ms = 10800000,
      stdout_bytes = 1048576,
      stderr_bytes = 67108864,
    },
  ],
  # These are NOT waived by portable completion and are NOT fabricated rows.
  separate_required_release_obligations = [
    "strict-native-validation",
    "native-traceability-coverage",
    "policy-projection-freshness",
    "native-release-readiness",
    "seven-row-kvm-release-matrix",
    "rust-owned-kvm-release-receipt-validation",
    "historical-retention-and-replay-acceptance",
    "remaining-product-requirement-evidence",
  ],
}
