# Event Entry Exact SMP Capability

## ADDED Requirements

### Requirement: Production SMP requires event-entry exactness

r[chaoscontrol.event_entry_exact.admission] Production deterministic SMP MUST reject ExactSingleStep and PmuAccelerated exact remainder unless a backend capability establishes the exact guest-instruction contract across ordinary execution, IRQ, NMI and synchronous exception entry. Portable KVM control acceptance and finite behavioral samples MUST NOT establish that capability; the current portable provider MUST report it unavailable. No operator bypass or coarse-boundary fallback is permitted.

#### Scenario: Portable KVM receives an SMP request

- GIVEN either exact single-step or PMU with exact remainder and more than one vCPU
- WHEN production admission evaluates the request
- THEN it MUST return a typed missing event-entry exact capability error, even when debug controls or a sample loop work.

### Requirement: Unsupported SMP fails before mutation

r[chaoscontrol.event_entry_exact.early] Admission MUST reject unsupported SMP before KVM creation, guest loading, memory allocation and disk/log mutation. Single-vCPU operation and pure scheduler semantics MUST remain unchanged.

#### Scenario: Unsupported SMP also requests a log path

- GIVEN unsupported SMP and an absent output log path
- WHEN VM construction runs
- THEN the capability error MUST precede any output creation or guest mutation.

#### Scenario: Single-vCPU construction

- GIVEN one vCPU and otherwise valid inputs on an available KVM host
- WHEN VM construction runs
- THEN missing SMP event-entry exactness MUST NOT reject it.

### Requirement: Diagnostics do not confer production capability

r[chaoscontrol.event_entry_exact.evidence] Tests and evidence MUST distinguish partial KVM-control fixtures from strict production admission. The observed IRQ overshoot MUST remain capability-negative evidence; one-instruction expectations MUST NOT be repinned to the overshot RIP. Production tests MUST NOT silently skip, fabricate capability or use a public bypass or cfg-test runtime fallback. KVM release/readiness rails MUST NOT classify passing capability-rejection tests or partial debug-control diagnostics as positive strict SMP execution. Their SMP actual-run authority MUST attempt production admission and keep unsupported strict SMP blocked/nonpassing; historical receipts MUST remain immutable.

#### Scenario: Diagnostic observes more than one instruction at a debug exit

- GIVEN the retained IRQ observation at RIP 512 with zero AX/BX followed by one debug exit at RIP 513 with AX/BX one
- WHEN evidence is classified
- THEN it MUST be classified as a failed exact boundary, not a new definition of one instruction or a supported capability.

#### Scenario: Negative admission suite passes

- GIVEN portable negative-admission tests and partial spin-loop diagnostics pass
- WHEN a current strict SMP release row is evaluated
- THEN only its separately bound production actual-run command MAY establish execution evidence
- AND unavailable strict SMP MUST remain nonpassing and MUST NOT yield ReleaseEligible.

