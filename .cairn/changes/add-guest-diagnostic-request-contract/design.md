## Context

The SDK/VMM hypercall protocol carries command IDs and shared-page payloads. The
guest supervisor already polls process-fault commands. A diagnostic request needs
a stable, reviewable encoding before host dispatch or supervisor execution can be
added.

## Decisions

### Decision: Canonical binary request with lexical admission

**Choice:** Encode version, executed identity, argument vector, and timeout in
little-endian canonical form with fixed limits (16 arguments, 1024 bytes each,
4096 total, timeout 1-30000 ms), and reject any request outside those bounds.

**Rationale:** Fixed-width canonical fields make review and negative-test coverage
straightforward and avoid JSON ambiguity at the guest boundary.

### Decision: Contract-only slice

**Choice:** Ship the protocol contract without supervisor execution, host dispatch,
or result receipts.

**Rationale:** Execution requires guest-side process handling, result transport,
and evidence binding that need their own change. This slice keeps the published
contract honest and testable.

## Risks / Trade-offs

- A request contract without execution cannot be exercised end-to-end; the caller
  must treat admission as a necessary, not sufficient, condition.
