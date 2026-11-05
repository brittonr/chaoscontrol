## Tasks

- [x] [serial] Record the evaluated contract, Capsem reference mechanisms, determinism/assertion interactions, and gating questions. r[chaoscontrol.guest_seccomp_admission.typed_admission]
- [ ] [serial] Obtain the maintainer product-scope decision on a uid-map/syscall policy for lowered bundles. r[chaoscontrol.guest_seccomp_admission.gated_scope]
- [x] [serial] Implement the pure evaluation model for map/policy admission and typed failures (detached; not wired). r[chaoscontrol.guest_seccomp_admission.typed_admission]
- [ ] [serial] Add bundle fixtures for a passing policy and a denied-required-syscall policy with distinct outcomes. r[chaoscontrol.guest_seccomp_admission.typed_admission]
- [ ] [serial] Bind the policy id into the deterministic replay configuration and reject cross-policy reuse. r[chaoscontrol.guest_seccomp_admission.gated_scope]
- [ ] [serial] Run the KVM rust-workload proof with map/policy applied and retain bounded evidence. r[chaoscontrol.guest_seccomp_admission.gated_scope]

## Evidence

Detached model `tools/guest-admission-model` implements the pure map/policy
validation with six passing tests; it is not wired into intake and enforces
nothing. Capsem mechanisms reviewed at
`google/capsem@c1018eac` (id map in `capsem-core/src/container.rs`, seccomp in
`capsem-core/src/container/seccomp.rs`, launcher in
`guest/artifacts/container/runtime_userns.py`). No bundle field, CLI, filter, KVM
run, or replay behavior changed. The scope decision and replay-configuration
binding are prerequisites, not assumptions.
