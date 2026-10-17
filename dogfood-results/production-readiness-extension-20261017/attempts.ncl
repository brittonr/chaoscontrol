let P = import "../../contracts/evidence/profile-primitives.ncl" in
let Prior = import "../production-readiness-cairn-20261017/attempts.ncl" in
let Attempt = {
  number | P.integer_between 31 35,
  correction | String,
  status | [| 'planned, 'passed, 'failed, 'blocked |],
  evidence | Array String,
} in
{
  schema = "chaoscontrol.production-readiness-authorized-extension-attempts.v1",
  owner = "chaoscontrol-vmm",
  starting_revision = "f18e49f7f3eeebdd282868355b4564f9ddb971a0",
  cairn_starting_revision = "35e6bd4c4d6e9d5a3639a0e70ca922faf3b94a48",
  previous_ledger = "dogfood-results/production-readiness-cairn-20261017/attempts.ncl",
  previous_used_attempts | std.contract.Equal 30 = Prior.used_implementation_attempts,
  original_maximum_implementation_attempts | std.contract.Equal 20 = Prior.original_maximum_implementation_attempts,
  maximum_implementation_attempts | std.contract.Equal 35 = 35,
  authorization = {
    source = "Explicit user selection in the active goal conversation",
    selection = "Authorize five more attempts",
    additional_attempts | std.contract.Equal 5 = 5,
    additional_scope = "Necessary ChaosControl requirement work and Cairn source-gate/integration corrections",
    unchanged_boundaries = [
      "Every original acceptance criterion remains required",
      "No unrelated sibling edits, host reconfiguration, manual credential access, deployment, or unrelated changes",
      "No weakened checks, campaign bounds, strict memory/assertion comparisons, source cleanliness, dependency observations, or passing-report requirements",
      "Use repository-owned native lifecycle and dependency-update workflows; commit and push verified changes without force",
    ],
  },
  counting_rule = Prior.counting_rule,
  historical_boundary = "All earlier ledgers, receipts, source revisions, artifacts and verdicts remain unchanged; this prospective extension supplies no passing result",
  attempts | Array Attempt = [
    {
      number = 31,
      correction = "Correct genuine Cairn source-gate prerequisites: exact policy license grant/inventory evidence, canonical Octet source paths without suppressing findings, and required unchanged producer/source verification.",
      status = 'blocked,
      evidence = [
        "extension31-license-tests/stdout.log: five genuine license inventory mutation cases passed; the actual inventory command passed separately.",
        "extension31-octet-lock/execution.json: supported Nix lock update admitted Octet 620833ca621efd3908ae4f3d76c3975effa8bd1a without suppressing findings.",
        "extension31-octet-integration/execution.json: required Nix integration exhausted the unchanged 600-second bound waiting for the global garbage-collector lock; no aggregate producer verdict exists.",
      ],
    },
    {
      number = 32,
      correction = "Complete accepted VM runtime admission and continuation boundaries for residual snapshot, SMP, virtio safety, runtime capacity and simulation-core requirements with actual positive/negative behavioral evidence.",
      status = 'blocked,
      evidence = [
        "The prior exact 33-ID runtime audit remains authoritative; source implementation is not whole-requirement completion.",
        "extension32-runtime-tests/{stdout,stderr}.log: source compilation was attempted, but /tmp exhausted bytes and inodes. Pueue task4 records wrapper failure1 while writing execution.json with EDQUOT; the child exit and test verdict are unavailable, not passing.",
      ],
    },
    {
      number = 33,
      correction = "Complete genuine non-VM accepted product/evidence boundary gaps, including protocol and guest intake plus explorer campaign/event/schedule replay carriers, using the existing ownership and shared-core contracts.",
      status = 'failed,
      evidence = [
        "extension33-semantic-tests/{execution.json,stdout.log}: SMR10/10 passed; Wasm25/33 passed. Eight admission regressions failed on stale Bounds fixtures missing maximum_functions.",
        "extension33-profile-generation-command/stderr.log: exact Nickel exported the current MVP profile but rejected the kernel positive example's unbound import contract syntax. This is not a runtime or release verdict.",
        "All accepted residual requirements remain required; no full implementation closure marker was added merely from these source edits.",
      ],
    },
    {
      number = 34,
      correction = "Complete the exercised integration cutover: current bounded Wasm fixture, let-bound kernel contract authoring, nested v2 resume-probe consumers, and remaining real compiler/caller prerequisites discovered at the shared verification barrier. Preserve every mandatory runtime scenario and evidence boundary.",
      status = 'failed,
      evidence = [
        "The eight stale Bounds failures and exact Nickel syntax failure are retained unchanged in attempt33 observations; owner corrections precede the new verification run.",
        "Owned disposable dev cache only was cleaned with patched Cargo:1637 files/2.4GiB. Logs, VM artifacts, historical receipts and unrelated files were preserved; no host configuration changed. Future diagnostic compiler outputs use the original repository's ignored target directory.",
        "extension34-profile-generation/summary.json: exact Nickel accepted the current MVP, corrected kernel, and OCI projection examples, and rejected all five kernel negatives plus post-MVP drift.",
        "extension34-minimal-lock/summary.json: committed dependency identities were preserved; only wasmparser0.236.0 and its three absent dependencies were added.",
        "extension34-workspace-compile/stderr.log: all-target compile failed on the nonexistent generated link trace_kvm_pic_irq; existing BPF/prog owner names trace_kvm_pic_set_irq. VMM also reports a test-only GuestMemoryMmap import in the production library.",
      ],
    },
    {
      number = 35,
      correction = "Finish the final cohesive compiler/consumer/contract integration: actual generated trace link naming, test-only imports, complete runtime marker contract/allowlist cutover and genuine producer blocked-evidence accounting. Exercise affected consumers plus required fresh gates without weakening acceptance or adding unrelated features.",
      status = 'blocked,
      evidence = [
        "This is the final authorized corrective implementation batch. Every failed command remains recorded; unchanged-source diagnostic and release invocations do not create passing evidence.",
        "Cairn native traceability now resolves both license requirements but honestly reports missing cairn.octet_integration.evidence.blocked. Producer aggregate Nix and source gates remain nonpassing.",
        "extension35-final-source-workspace/execution.json: workspace all-target tests passed with no timeout. The earlier quota-exhausted recorder output remains unavailable, not reconstructed as passing.",
        "extension35-final-source-clippy/execution.json: the genuine GNU same-type statfs cast warning failed denied-warning Clippy. The portable GNU/musl source correction stays within this compiler/consumer batch; no lint was suppressed.",
        "extension35-portable-abi-{clippy,musl-clippy,musl,format}/execution.json: workspace denied-warning Clippy, musl supervisor Clippy/build and formatting all passed.",
        "extension35-portable-abi-guest/execution.json: the rebuilt actual guest PID1 mounted tmpfs and retained its measured directory identity/filesystem type. Earlier init-argument/operational watchdog failures remain separately retained and are not guest findings.",
        "extension35-{directory-forgery-before,replay-bound-before,allocation-bound-before}/execution.json retain failing-before observations; the corrected actual OCI CLI, concrete retained SMR consumer and real allocation-bound smokes passed without expected-record echoes.",
        "extension35-final-contracts/summary.json: all six public descriptor/evidence/matrix/registry/profile/trace fixture commands passed. Latest native traceability still has 118 mandatory missing requirements and zero dangling references; no full readiness or immutable Cairn cohort is inferred.",
      ],
    },
  ],
  used_implementation_attempts = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true = used_implementation_attempts <= maximum_implementation_attempts,
}
