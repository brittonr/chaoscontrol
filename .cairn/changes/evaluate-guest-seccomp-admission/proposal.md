## Why

Capsem runs each OCI workload through a guest launcher that applies an OCI
user-namespace map (container ids 0–65535 map to VM ids 100000–165535, never VM
root) and the vendored moby default seccomp allowlist, with the image share
read-only. ChaosControl lowers reviewed OCI layers into a guest process bundle and
deliberately has no container runtime; lowered processes inherit the guest's
namespace and syscall surface. The [Capsem comparison](../../../docs/capsem-comparison.md)
identified bounded uid-map/seccomp admission as adoptable; issue
[#8](https://github.com/brittonr/chaoscontrol/issues/8) tracks the evaluation.

## What Changes

Record the evaluation and a gated design for an optional admission layer on the
bundle contract:

1. An explicit uid/gid map per lowered process group, with container root never
   equal to guest root.
2. An optional syscall policy identified by a stable policy id, resolved from a
   vendored allowlist and applied by the guest launcher.
3. A typed rejection for unsupported maps, policies, or interactions with
   deterministic replay and assertion instrumentation.

No runtime behavior, bundle field, CLI, or filter is added by this change. The
product-scope decision ("no container runtime" stays a non-goal; whether a
namespace/syscall policy crosses that line) must come first.

## Consumer and Ownership

Consumer: workloads that want defense-in-depth beyond the VM boundary for lowered
OCI processes. Durable capability: an evaluated, bounded admission contract and
its gating questions. Adoption path: a follow-up profile field and pure admission
core after the scope decision, then a KVM proof on the rust-workload fixture.
Maintenance owner: chaoscontrol-evidence (intake/bundle) with chaoscontrol-vmm for
launcher-side application. Repeatability: typed admission tests and a KVM bundle
fixture.

## Non-Goals

No container runtime, image puller, daemon, CRI/OCI-runtime compatibility,
namespace beyond what the bundle already declares, seccomp filter authored by
hand, or claim that a syscall filter proves isolation. No change to replay
semantics, assertion instrumentation, or existing bundle artifacts. No support
promotion.
