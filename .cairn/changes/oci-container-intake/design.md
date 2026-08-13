# Design: OCI Container Intake

## Context

The guest pipeline builds kernels and initrds through Nix. The multiprocess topology change introduces a deterministic supervisor and shared storage. This change sits on top of both to admit existing service images.

## Decisions

### 1. Intake produces a guest bundle

An intake tool reads a declared topology and produces a bundle the existing guest pipeline can boot: process manifest entries, an image or directory root per service, and shared working directory bindings. The bundle is the durable artifact, not the Docker files.

### 2. Nickel owns the topology

The topology contract binds images, command overrides, environment, shared directories, and capability admits. Export produces the deterministic bundle manifest under the existing projection rules.

### 3. Image format support is bounded

An admitted OCI example is supported when its layers can be extracted into the bundle root in a deterministic order. Directories and non-OCI archives fall back to the same manifestation path. An unsupported format fails the intake with a typed diagnostic.

### 4. Runtime is unchanged

The VMM still boots the same kernel and initrd shape. The supervisor from the multiprocess change runs the manifest. Shared working directories use the admitted deterministic device surface.

### 5. Evidence tracks image provenance

The bundle records the image identity and layer order for each service. Receipts bind image identity, bundle identity, and process manifest identity. An unmatched image identity fails the receipt.

## Risks

Images can carry their own init systems and assumptions about cgroup namespaces. The supervisor contract must document that service processes run under the manifest without container namespaces. Layer determinism depends on extraction order, so the order must be an admitted, recorded input.
