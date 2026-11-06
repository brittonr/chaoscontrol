# Event-Entry Exact Admission Design

## Context

The accepted deterministic-smp-scheduling capability requires exact instruction boundaries and typed unavailable-capability rejection. Portable KVM debug-control acceptance proves mechanism configuration only, not exact stopping through IRQ/NMI/exception entry. A finite probe cannot establish the full contract.

## Decisions

### Decision: Separate debug controls from event-entry exactness

**Choice:** Name the pure capability contract explicitly and require it for exact mode and PMU exact remainder. The portable production provider reports it unavailable unconditionally; no runtime sample can manufacture support. Keep scheduler arithmetic, transitions and serialized modes unchanged.

**Rationale:** PMU remainder depends on the same exact primitive and cannot evade its missing event-entry guarantee.

### Decision: Reject at the construction boundary

**Choice:** Validate SMP admission at the first production VM constructor step, before capacity allocations, KVM ioctls and guest/disk/log mutation. Retain typed missing-capability errors. Single-vCPU construction does not require SMP stepping capability.

**Rationale:** Unsupported configurations must not mutate an attempted guest or produce deterministic success artifacts.

### Decision: Keep diagnostics explicitly separate

**Choice:** Low-level tests may manually assemble partial KVM fixtures from an admitted single-vCPU VM and explicit vCPU/scheduler state. Such fixtures exist only in test modules and do not bypass the production constructor, expose public overrides, or assert a true backend capability. Keep the IRQ overshoot as negative evidence; production tests assert rejection rather than skip or claim execution.

**Rationale:** Partial control/state-machine regressions remain useful without constituting universal event-entry support.

## Discovery and Verification

Relevant local guidance, README and accepted SMP specification were read before source edits. Repository-wide specialized grep discovered capability symbols and SMP constructor consumers before exported changes. Descriptive find failed because its judging provider rejected requests, so literal searches located implementation and callers. The worker's `read xd://lsp` returned `No such tool` (mounted devices: ast_grep, ast_edit, debug, github); the parent separately used its available rust-analyzer LSP references on the current declarations. Parent observed ProgressCapabilities references in core validator/test and vm import/constructor, validate_progress_capabilities references in core tests and vm import/admission, and event_entry_exact_single_step references in core validation/test and vm admission. The removed old field could not be queried after rename; its pre-edit grep references were retained in discovery. No source build, test or formatter is run by this worker; parent owns final verification.

## Capability-Negative IRQ Diagnostic

`diagnostic_irq_entry_does_not_meet_exact_instruction_contract` retains the expected first handler instruction boundary, not the observed overshot RIP. It is explicitly ignored with a known portable KVM IRQ-entry overshoot reason, not silently skipped production acceptance. The routine suite exercises early strict rejection and single-vCPU construction; a passing partial diagnostic suite is not a release execution proof. To deliberately examine the known failing diagnostic, run `cargo test -p chaoscontrol-vmm --lib diagnostic_irq_entry_does_not_meet_exact_instruction_contract -- --ignored --nocapture`; failure at the handler boundary is capability-negative evidence, not a product claim to repair by repinning expectations. The historical observation is not rerun merely to confirm it.


## Risks / Trade-offs

- Production SMP becomes unavailable on the current portable provider; historical passing bounded spin-loop and release receipts remain historical, not current support authority.
- Re-enabling SMP requires a real backend event-entry exact capability, not an operator switch, accepted controls or a finite passing sample.
- Existing partial diagnostics can remain green while strict production admission rejects; neither result proves the other.
