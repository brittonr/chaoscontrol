# Focused Nickel toolchain validation receipt.
# r[impl chaoscontrol.nickel_toolchain.validation]
# r[impl chaoscontrol.nickel_toolchain.validation.scenario.passed]
# r[verify chaoscontrol.nickel_toolchain.validation]
# r[verify chaoscontrol.nickel_toolchain.validation.scenario.passed]
{
  schema = "chaoscontrol.nickel-toolchain-validation.v1",
  source_revision = "7594be4e034ea757ad5d132596a3b5475b88008d",
  commands = [
    "cargo fmt --all -- --check",
    "cargo clippy --offline --locked --workspace --all-targets -- -D warnings",
    "cargo test --offline --locked --workspace --all-targets --no-fail-fast",
    "check-evidence-contracts --root .",
    "nix build .#checks.x86_64-linux.nickel-cohort-exact --no-link -L",
    "cairn validate --root . --policy cairn-policy/generated/cairn-policy.json --strict",
  ],
  results = {
    formatting = "passed",
    clippy = "passed",
    workspace_tests = { passed = 2223, failed = 0, ignored = 16, suites = 168 },
    evidence_contracts = "passed",
    nickel_cohort_exact = "passed",
    cairn_validate = "valid",
  },
  scope = "Exact Nickel 1.17.0 evaluator, profile fixtures, negative profile cases, evidence contracts, formatting, denied-warning Clippy, workspace tests, and strict Cairn lifecycle validation on the bound source revision.",
  non_claims = [
    "This receipt does not claim universal determinism, guest correctness, KVM behavior, or release eligibility.",
    "The Nickel check proves profile and evidence-configuration shape, not schedule, fault, guest, replay, or evidence meaning.",
  ],
}
