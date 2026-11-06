# Require Event-Entry Exact SMP Capability

## Why

The accepted deterministic SMP contract requires exact guest-instruction boundaries, including IRQ, NMI and synchronous exception entry. KVM SET_GUEST_DEBUG accepting single-step controls does not establish that contract. The retained local diagnostic `/tmp/cc-persisted-debugger-0tw7wr4k/pueue-35-irq-state.log` observed injected IRQ 81, RIP 512 and AX/BX 0 before entry, then one debug exit at RIP 513 with AX/BX 1: handler INC, IRET and interrupted INC retired before control returned. Rearming before each KVM_RUN repairs POPF/IRET returns but cannot repair TF lost inside event entry.

## What Changes

- Reject production SMP for both ExactSingleStep and PMU with exact remainder unless the backend establishes event-entry exactness. The portable KVM provider supplies no such capability. r[chaoscontrol.event_entry_exact.admission]
- Reject before KVM creation, guest loading, memory allocation or disk/log mutation. Keep single-vCPU operation and pure scheduler transitions unchanged. r[chaoscontrol.event_entry_exact.early]
- Separate partial KVM control diagnostics from production admission; preserve capability-negative IRQ evidence without repinning one-instruction expectations to an overshot RIP. No public bypass, operator override or test-only runtime fallback. r[chaoscontrol.event_entry_exact.evidence]

## Impact

- **Files**: VMM admission and affected VMM, sim-core and consumer tests; this package documents the corrected support boundary. Parent-owned README and existing debugger receipts remain separate.
- **Testing**: positive single-vCPU construction; negative exact and PMU SMP construction before side effects; pure capability cases; partial diagnostic fixtures explicitly do not establish production support. Parent runs builds/tests/formatters after integration, not during implementation.
- **Non-goals**: no coarse-boundary implementation, host kernel installation, new behavioral capability probe, timer fallback, support promotion, commit or publication.
