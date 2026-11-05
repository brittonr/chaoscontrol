# Guest seccomp and id-map admission evaluation

Evaluation for issue [#8](https://github.com/brittonr/chaoscontrol/issues/8),
following the [Capsem comparison](capsem-comparison.md).

Capsem applies an OCI user-namespace map and the vendored moby seccomp allowlist to
the workload inside its guest. ChaosControl lowers reviewed OCI layers into a guest
process bundle and deliberately has no container runtime; lowered processes inherit
the guest's namespace and syscall surface. This document records what a bounded
admission layer would mean, what must be decided first, and why nothing is
implemented yet.

## Capsem reference

At `google/capsem@c1018eac`:

- [`crates/capsem-core/src/container.rs`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/crates/capsem-core/src/container.rs)
  defines the workload id map (container 0 → VM 100000, size 65536) and rejects
  maps below VM id 65536.
- [`crates/capsem-core/src/container/seccomp.rs`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/crates/capsem-core/src/container/seccomp.rs)
  vendors moby's default allowlist unchanged, resolves conditions on the host, adds
  Capsem denials, and never grants `CAP_SYS_ADMIN`.
- [`guest/artifacts/container/launch.py`](https://github.com/google/capsem/blob/c1018eacbcfb1de12aaadb56b1a388d107ef074d/guest/artifacts/container/launch.py)
  runs the workload with the image share read-only and the workspace writable.

## Proposed ChaosControl contract

A pure admission core in `chaoscontrol-evidence` would accept or reject a declared
`GuestAdmission { uid_map, gid_map, syscall_policy }`:

- Maps must not translate container root to guest root; ids must be non-overlapping
  and within declared size limits.
- A syscall policy is a stable id resolved from a small vendored set; unknown or
  unenforceable ids reject with distinct typed outcomes.
- The intake shell rejects declarations it cannot enforce; the guest launcher
  applies them. The core never executes anything.

## Blocking questions

1. **Product scope.** "No container runtime" is an explicit non-goal. Whether a
   uid map plus syscall filter crosses that line is a maintainer decision.
2. **Determinism.** A filter that returns `EPERM` changes control flow. The applied
   policy id must be part of the deterministic replay configuration; replaying under
   a different policy must be rejected, not compared.
3. **Assertion transport.** SDK, supervisor, and assertion paths may use syscalls a
   workload filter denies. The filter must be scoped to the workload process only.
4. **File ownership.** Workspace and shared-directory bindings assume stable VM
   ownership; a map must not turn shared files into guest-root owned.
5. **Outcome classes.** Unsupported policy, unenforceable map, missing kernel
   seccomp support, and a runtime denial are distinct outcomes; none may be
   reported as a pass.

## Planned bounded verification (not run)

- Pure tests for map validation, policy resolution, and typed rejection.
- Bundle fixtures: one policy that lets the workload reach its assertion, one that
  denies a required syscall and must report denial rather than hang.
- KVM rust-workload proof with the map/policy applied, a same-policy replay match,
  and a cross-policy replay rejection.

## Decision

Design-only. No bundle field, CLI, filter, or launcher change is made until the
scope decision and replay-config binding exist. A uid map and syscall filter are
defense in depth inside the guest; they are not a sandbox, isolation proof, or a
replacement for the VM boundary or kernel hardening.

## Non-claims

No admission code, KVM run, replay behavior, or isolation claim is produced.
Capsem's mechanisms are a reference, not an endorsement.
