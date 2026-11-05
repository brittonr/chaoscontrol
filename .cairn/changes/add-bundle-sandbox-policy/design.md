## Context

The lowered bundle contract does not carry namespace or syscall policy today. Issue
#8 requests evaluation as an optional bounded admission surface, explicitly without a
container runtime, image puller, or daemon.

## Decisions

### Decision: Pure policy admission only

**Choice:** Model the uid/gid map and syscall rules as data with fixed bounds and
typed rejection, without installing namespaces, filters, or affecting replay.

**Rationale:** Admission can be reviewed and tested deterministically now; install
and KVM proof need their own change and evidence.

### Decision: Policy identity binds into run profiles; no filter is installed

**Choice:** Callers bind `policy_digest` into a run or replay profile when sandbox
policy is selected. Assertion instrumentation uses hypercalls and port I/O, not guest
syscalls, so a syscall policy does not cover that transport by construction. No
namespace or filter installation is implemented under this change.

**Rationale:** This answers the replay/assertion-interaction question without claiming
enforcement evidence that does not exist. The owner selected pure admission only.

### Decision: Root and container-zero guards

**Choice:** Reject any map that includes guest root outside id 0, and require an
entry mapping inside id 0.

**Rationale:** These are the Capsem-comparison safety properties that distinguish an
explicit map from inheriting the guest's own namespace.

## Risks / Trade-offs

- A passed policy does not enforce anything; callers must not treat it as isolation.
- Filter behavior, syscall-policy meaning, and replay equivalence remain unmeasured
  because no installation is implemented.
- If installation is later authorized, the policy digest must be part of the admitted
  run identity and the KVM proof must revalidate assertion and coverage paths.
- Syscall names are lexical; kernel availability and ABI compatibility remain
  workload-contract questions.
