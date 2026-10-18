let P = import "../../contracts/evidence/profile-primitives.ncl" in
let Prior = import "../production-readiness-continuation-20261017/attempts.ncl" in
let Attempt = {
  number | P.integer_between 41 45,
  correction | String,
  status | [| 'planned, 'passed, 'failed, 'blocked |],
  evidence | Array String,
} in
{
  schema = "chaoscontrol.production-readiness-attempts41-45.v1",
  owner = "chaoscontrol-vmm",
  starting_revision = "c3a14cb6962d7734ebfff86bf4088d1edcad3573",
  previous_ledger = "dogfood-results/production-readiness-continuation-20261017/attempts.ncl",
  previous_used_attempts | std.contract.Equal 40 = Prior.used_implementation_attempts,
  original_maximum_implementation_attempts | std.contract.Equal 20 = Prior.original_maximum_implementation_attempts,
  maximum_implementation_attempts | std.contract.Equal 45 = 45,
  authorization = {
    source = "Explicit user selection for additional_corrective_authority in the active goal conversation",
    selection = "Authorize five more attempts",
    additional_attempts | std.contract.Equal 5 = 5,
    additional_scope = "ChaosControl-owned corrections and supported dependency integration, starting with Frontier BoundExceeded",
    unchanged_boundaries = [
      "Every original acceptance criterion remains required",
      "No sibling edits, host reconfiguration, manual credential access, deployment, or unrelated changes",
      "No weakened checks, campaign bounds, strict memory/assertion comparisons, source cleanliness, or passing-report requirements",
      "Use existing native Cairn changes and the repository-owned dependency-update workflow; commit verified changes and push main without force",
      "External Cairn producer and host-owned Nix prerequisites remain required; additional attempts do not supply their missing verdicts",
    ],
  },
  counting_rule = Prior.counting_rule,
  historical_boundary = "All previous ledgers, receipts, source identities, raw artifacts and verdicts remain unchanged. This prospective authorization is not implementation or release evidence.",
  attempts | Array Attempt = [
    {
      number = 41,
      correction = "Correct actual merged branch-marker/frontier resource accounting and run ownership behind Frontier BoundExceeded; retain unchanged campaign and replay admission bounds.",
      status = 'blocked,
      evidence = ["frontier41-regression-before retains the initial failing resource-accounting path; frontier44-regression-after retains its 300-second operational timeout", "integrated45-workspace-precommit passes the complete frontier regression corpus, including marker ownership, per-VM merged capacity and marker-only selection after exhaustion", "frontier41-actual-control reaches seven rounds and fourteen branches without Frontier BoundExceeded, then times out at the unchanged 600-second bound; this is not campaign completion, a guest bug, or depth-four replay proof"],
    },
    {
      number = 42,
      correction = "Complete genuine owner-local CPU/complete-VM architectural preflight and snapshot continuation acceptance gaps without reimplementing the existing runtime proof or weakening state comparisons.",
      status = 'passed,
      evidence = ["runtime42-owner-paths and architecture45-runtime-after exercise cached pure preflight with unavailable-descriptor isolation, whole-VM nondefault state and all-backend continuation, partial quantum with real EINTR, and required PIT apply failure after memory replacement with permanent poison", "cohort42-consumer-migration preserves the actual adapter consumer after cached capability ownership", "integrated45-workspace-precommit and architecture45-final-vmm-library pass; broader unsupported architecture and whole-profile requirements remain open"],
    },
    {
      number = 43,
      correction = "Complete genuine owner-local virtio lifecycle, request and descriptor admission plus bounded-shell and malicious-guest acceptance gaps; retain strict failure-state carriers and pure allocation-free planning.",
      status = 'passed,
      evidence = ["virtio43-owner-paths exercises production descriptor, request, status, failure and backend consumers", "Generated bounded-shell cases observe actual request-family reachability and allocations; the malformed-guest fixture uses production VM MMIO dispatch and the same KVM guest memory", "integrated45-workspace-precommit passes allocation-free planner, adversarial guest and bounded consumer behavior; this is not exhaustive input safety or whole-system readiness"],
    },
    {
      number = 44,
      correction = "Integrate the observed missing adapter test import and diagnose/correct the strengthened raw-CPU fixture's failing uninterrupted NMI/MSR/XMM control before claiming restore equivalence; retain the exact intended output sequence.",
      status = 'failed,
      evidence = ["cpu44-diagnostic records XCR0=1 and inactive XSAVE SSE state despite KVM_GET_FPU returning the marker", "cpu44-architectural-continuation-after preserves the exact expected trace and fixes duplicate NMI delivery through guest IRET, but uninterrupted XMM output remains zero", "The RETENTION_RANK_ORDER integration import is corrected; no architectural success is inferred from that compiler correction"],
    },
    {
      number = 45,
      correction = "Stage actual active SSE state through the selected KVM XSAVE representation, reject nonzero reserved debug-register flags before restore effects, and admit the architecture's valid two-pending-NMI boundary rather than misclassifying its count as a boolean.",
      status = 'passed,
      evidence = ["architecture45-admission-before retains the failing reserved-debug-register rejection", "architecture45-cpu-after observes the unchanged uninterrupted/restored NMI, required-MSR, active-XMM and subsequent-output sequence", "architecture45-runtime-after and architecture45-final-vmm-library pass reserved-field preflight, valid two-pending-NMI admission and composite continuation", "integrated45-workspace-precommit and integrated45-clippy-precommit pass; all45 authorized slots are consumed and unavailable gates do not authorize another correction"],
    },
  ],
  allocation_boundary = "All45 authorized slots are allocated. Read-only diagnosis, unchanged-source gates and evidence closure do not authorize another product correction. Every initial integration failure remains retained.",
  used_implementation_attempts = previous_used_attempts + std.array.length attempts,
  within_limit | std.contract.Equal true = used_implementation_attempts <= maximum_implementation_attempts,
}
