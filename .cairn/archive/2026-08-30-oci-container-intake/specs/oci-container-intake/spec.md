# OCI Container Intake Specification

## Purpose

Admit existing image-based multi-service topologies into deterministic guest campaigns without a container runtime.

## ADDED Requirements

### Requirement: Intake lowers a declared topology

r[chaoscontrol.oci_intake.bundle_lowering] An intake tool MUST lower a declared multi-service topology into a guest bundle containing one process-manifest entry per service, a service root, and shared working directory bindings.

#### Scenario: Multi-service topology
- GIVEN a declared topology with two services and one shared directory
- WHEN intake runs
- THEN a bundle MUST be produced with two manifest entries and the shared binding.

#### Scenario: Single service
- GIVEN a declared topology with one service
- WHEN intake runs
- THEN the bundle MUST degrade cleanly to the single-process path.

### Requirement: Topology is Nickel-owned

r[chaoscontrol.oci_intake.nickel_topology] The service topology MUST be a typed Nickel contract that binds images, command overrides, environment, shared directories, and capability admits, and MUST export a deterministic bundle manifest.

#### Scenario: Projection validates
- GIVEN a conformant topology
- WHEN the projection runs
- THEN the bundle manifest MUST validate against its contract.

#### Scenario: Malformed topology
- GIVEN a topology that violates the contract
- WHEN the projection runs
- THEN it MUST fail with a typed diagnostic.

### Requirement: Image support is bounded

r[chaoscontrol.oci_intake.image_boundary] Intake MUST extract an admitted image format into the bundle root in a declared, recorded order, MUST support a directory or non-OCI archive fallback, and MUST fail with a typed diagnostic on an unsupported format.

#### Scenario: Admitted image
- GIVEN an image of an admitted format
- WHEN intake runs
- THEN every layer MUST be extracted in the declared order.

#### Scenario: Unsupported format
- GIVEN an image of an unsupported format
- WHEN intake runs
- THEN intake MUST fail with a typed diagnostic naming the format.

### Requirement: Provenance binds images and bundles

r[chaoscontrol.oci_intake.provenance] A receipt MUST bind each service's image identity, layer order, service root identity, and process manifest identity, and MUST fail closed when an identity does not match.

#### Scenario: Identity mismatch
- GIVEN a bundle whose image identity differs from the receipt
- WHEN receipt validation runs
- THEN the receipt MUST fail closed.

### Requirement: Intake validation is adversarial

r[chaoscontrol.oci_intake.validation] Validation MUST pair a positive multi-image topology fixture with negative fixtures for malformed topology, unsupported image format, conflicting roots, and provenance mismatch.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to enable container intake
- WHEN intake, projection, evidence, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.

### Requirement: Intake claims stay bounded

r[chaoscontrol.oci_intake.claim_boundary] Container intake MUST NOT claim namespace isolation, image registry behavior, Kubernetes semantics, or cross-machine scheduling.

#### Scenario: Overclaim rejected
- GIVEN a report that claims an image-sourced bundle provides container isolation
- WHEN claim validation runs
- THEN the claim MUST be rejected.
