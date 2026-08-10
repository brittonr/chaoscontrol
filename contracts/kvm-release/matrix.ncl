# Required bounded KVM release matrix. Runtime observations remain Rust-owned.
let KvmReleaseMatrix = import "profile.ncl" in
let BuildTimeoutSeconds = 1800 in
let FocusedKvmTimeoutSeconds = 300 in
let DriftTimeoutSeconds = 1800 in
let FreshReplayTimeoutSeconds = 3600 in
let MaximumReceiptAgeSeconds = 86400 in
let MaximumRowArtifacts = 4096 in
let MaximumFocusedArtifactBytes = 67108864 in
let MaximumCohortArtifactBytes = 1073741824 in
let RequiredCapabilities = ["kvm", "x86_64-linux"] in
({
  schema_version = 1,
  profile_id = "required-kvm-release-v1",
  required_worker_arch = "x86_64",
  required_worker_capabilities = RequiredCapabilities,
  max_receipt_age_seconds = MaximumReceiptAgeSeconds,
  claims_pmu_support = false,
  bounded_claim = "This receipt establishes that the exact source revision passed the required matrix on the recorded admitted worker with the recorded commands, bounds, and artifact identities.",
  non_claims = [
    "The receipt does not prove worker integrity.",
    "The receipt does not prove all-host equivalence.",
    "The receipt does not prove universal determinism.",
    "The receipt does not prove workload correctness.",
    "The receipt does not prove production availability.",
  ],
  rows = [
    {
      id = "build-release-binaries",
      kind = "cohort-binary-build",
      required = true,
      required_capabilities = RequiredCapabilities,
      command = {
        program = "nix",
        args = ["build", ".#chaoscontrol", "--out-link", "%ROW_OUT%/chaoscontrol"],
      },
      limits = {
        timeout_seconds = BuildTimeoutSeconds,
        max_artifacts = MaximumRowArtifacts,
        max_artifact_bytes = MaximumCohortArtifactBytes,
      },
      retained_artifacts = ["stdout.log", "stderr.log", "chaoscontrol/**"],
    },
    {
      id = "build-raft-guest",
      kind = "cohort-guest-build",
      required = true,
      required_capabilities = RequiredCapabilities,
      command = {
        program = "nix",
        args = ["build", ".#initrd-raft", "--out-link", "%ROW_OUT%/initrd-raft"],
      },
      limits = {
        timeout_seconds = BuildTimeoutSeconds,
        max_artifacts = MaximumRowArtifacts,
        max_artifact_bytes = MaximumCohortArtifactBytes,
      },
      retained_artifacts = ["stdout.log", "stderr.log", "initrd-raft/**"],
    },
    {
      id = "deterministic-smp",
      kind = "deterministic-smp",
      required = true,
      required_capabilities = RequiredCapabilities,
      command = {
        program = "cargo",
        args = ["test", "-p", "chaoscontrol-vmm", "--test", "deterministic_smp_kvm"],
      },
      limits = {
        timeout_seconds = FocusedKvmTimeoutSeconds,
        max_artifacts = MaximumRowArtifacts,
        max_artifact_bytes = MaximumFocusedArtifactBytes,
      },
      retained_artifacts = ["stdout.log", "stderr.log"],
    },
    {
      id = "serialized-snapshot-replay",
      kind = "serialized-snapshot-replay",
      required = true,
      required_capabilities = RequiredCapabilities,
      command = {
        program = "cargo",
        args = ["test", "-p", "chaoscontrol-vmm", "--test", "exact_snapshot_kvm"],
      },
      limits = {
        timeout_seconds = FocusedKvmTimeoutSeconds,
        max_artifacts = MaximumRowArtifacts,
        max_artifact_bytes = MaximumFocusedArtifactBytes,
      },
      retained_artifacts = ["stdout.log", "stderr.log"],
    },
    {
      id = "virtio-malformed-input",
      kind = "virtio-malformed-input",
      required = true,
      required_capabilities = RequiredCapabilities,
      command = {
        program = "cargo",
        args = ["test", "-p", "chaoscontrol-vmm", "--test", "virtio_kvm_smoke", "--", "--ignored"],
      },
      limits = {
        timeout_seconds = FocusedKvmTimeoutSeconds,
        max_artifacts = MaximumRowArtifacts,
        max_artifact_bytes = MaximumFocusedArtifactBytes,
      },
      retained_artifacts = ["stdout.log", "stderr.log"],
    },
    {
      id = "admitted-drift",
      kind = "admitted-drift",
      required = true,
      required_capabilities = RequiredCapabilities,
      command = {
        program = "nix",
        args = ["run", ".#vm-determinism-drift", "--", "--out", "%ROW_OUT%/drift", "--runs", "2"],
      },
      limits = {
        timeout_seconds = DriftTimeoutSeconds,
        max_artifacts = MaximumRowArtifacts,
        max_artifact_bytes = MaximumCohortArtifactBytes,
      },
      retained_artifacts = ["stdout.log", "stderr.log", "drift/**"],
    },
    {
      id = "fresh-workload-replay",
      kind = "fresh-workload-replay",
      required = true,
      required_capabilities = RequiredCapabilities,
      command = {
        program = "nix",
        args = [
          "run",
          ".#raft-accepted-verdict-dogfood",
          "--",
          "--output",
          "%ROW_OUT%/fresh",
          "--max-attempts",
          "1",
          "--start-seed",
          "0",
        ],
      },
      limits = {
        timeout_seconds = FreshReplayTimeoutSeconds,
        max_artifacts = MaximumRowArtifacts,
        max_artifact_bytes = MaximumCohortArtifactBytes,
      },
      retained_artifacts = ["stdout.log", "stderr.log", "fresh/**"],
    },
  ],
} | KvmReleaseMatrix)
