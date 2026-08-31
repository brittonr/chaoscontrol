# OCI Container Intake

## Why

ChaosControl documents container and OCI intake as a non-goal. The consequence is that existing systems packaged as images cannot be admitted, and service topologies with several cooperating services and shared volumes have no onboarding path. The gap analysis ranks target admission as the first blocker to comparable capability. This change reopens that non-goal within the existing single-machine scope.

## What Changes

- Lower a declared multi-service topology into a ChaosControl guest bundle without requiring Docker or Kubernetes at runtime.
- Reuse the deterministic guest supervisor and shared-storage admission from `guest-multiprocess-topology`.
- Accept an OCI image as one service root where the image format is available, falling back to a directory root when it is not.
- Keep the deterministic device, schedule, fault, and evidence surfaces unchanged.

## Impact

- **Packaging**: an intake tool that consumes image and topology metadata and emits a guest bundle.
- **Configuration**: Nickel-owned topology and image registry.
- **Lifecycle**: readiness and evidence rails recognize image-sourced workloads.
- **Testing**: positive multi-image topology and negative missing-image, conflicting-root, and unsupported-format cases.

## Non-Goals

- No Kubernetes control plane.
- No image registry service.
- No cross-machine scheduling.
- No change to how the VMM executes a booted guest bundle.
