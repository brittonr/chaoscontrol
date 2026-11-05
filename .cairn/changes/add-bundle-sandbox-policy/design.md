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

### Decision: Root and container-zero guards

**Choice:** Reject any map that includes guest root outside id 0, and require an
entry mapping inside id 0.

**Rationale:** These are the Capsem-comparison safety properties that distinguish an
explicit map from inheriting the guest's own namespace.

## Risks / Trade-offs

- A passed policy does not enforce anything; callers must not treat it as isolation.
- Syscall names are lexical; kernel availability and ABI compatibility remain
  workload-contract questions.
