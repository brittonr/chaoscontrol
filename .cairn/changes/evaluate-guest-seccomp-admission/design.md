## Decision

Evaluate, but do not implement yet. The design target is a pure admission core in
`chaoscontrol-evidence` that accepts or rejects a declared process admission:

```text
GuestAdmission {
  uid_map: Option<IdMap>,        // container 0 -> guest id >= 65536, size <= 65536
  gid_map: Option<IdMap>,
  syscall_policy: Option<PolicyId>, // stable id for a vendored allowlist
}
```

`IdMap` follows the existing Capsem-style rule: container id 0 maps to a guest id
at or above 65536, so container root is never guest root; malformed, overlapping,
or root-mapping entries reject. `PolicyId` resolves to a vendored, reviewed
allowlist (for example the moby default filter) and a filter cannot request
`CAP_SYS_ADMIN`.

The bundle contract would carry the optional declaration; the launcher (guest
side) would apply the map and filter, and the intake shell would reject unknown
policy ids or declarations it cannot enforce. The pure core owns identity, bounds,
and typed outcomes; the shell owns application.

### Interactions that must be resolved first

1. **Product scope.** "No container runtime" is an explicit non-goal. A uid map
   and a syscall filter may or may not cross that line; the maintainer decides
   before any field is added.
2. **Determinism and replay.** A filter that returns `EPERM` for a syscall the
   workload uses changes control flow. Replay must record the applied policy id
   and treat a policy change as a different deterministic configuration; otherwise
   a replay under a different policy would silently diverge.
3. **Assertion instrumentation.** The SDK and guest supervisor may use syscalls a
   workload-only filter denies. The policy must be scoped to the lowered workload
   process, not the supervisor, or the filter will break assertion transport.
4. **UID mapping and file ownership.** The workspace and shared directories use
   stable VM ownership. A map changes the caller-relative view; intake must prove
   shared-directory bindings still resolve and no file becomes guest-root owned.
5. **Failure classes.** Unsupported policy, unenforceable map, kernel without
   seccomp filter support, and denial at runtime are distinct outcomes; none may be
   reported as a workload pass.

### Bounded verification plan

- Pure tests: map validation (root mapping, overlap, size, overflow), policy id
  resolution, unknown/unenforceable declarations, and typed rejection.
- Bundle fixtures: a workload declaring a map and a policy that still reaches its
  assertion; a policy that denies a required syscall and must report a distinct
  denial, not a hang.
- KVM proof on the rust-workload fixture with the policy applied, recording the
  boot command, policy id, map, and observed outcome; replay under the same
  policy must match, and a policy change must invalidate reuse.
- Determinism check: run the same bundle twice with the same policy and compare the
  recorded event boundary.

## Verification

This change is design-only: proposal/design/tasks gates and a scope review. No
runtime code, bundle field, KVM run, or filter is claimed.

## Boundaries

A uid map and syscall filter are defense in depth against confused-deputy and
broad-syscall abuse inside the guest; they are not a sandbox, isolation proof, or
replacement for the VM boundary or kernel hardening. Capsem's vendored allowlist in
this repository's comparison is a reference, not an endorsement.
