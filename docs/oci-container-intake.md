# OCI container intake

ChaosControl can convert a bounded image-sourced topology into the existing
guest process bundle. Docker, Kubernetes, and an image registry are not present
at runtime.

## Topology

`contracts/oci-intake/topology.ncl` owns the human-authored contract. Each
service declares a role, source format, source identity, entrypoint, process
settings, and shared directory membership. The pure Rust lowerer repeats all
bounds and emits the admitted process manifest from the guest multiprocess
contract.

Supported source formats are:

- `oci_layers`: a directory plus an ordered list of tar layers.
- `directory`: a bounded directory tree.
- `tar_archive`: one bounded non-OCI tar archive.

Any other format returns `UnsupportedFormat`. Intake does not fall back to a
container daemon.

## Extraction

The shell verifies every declared BLAKE3 identity before it changes the bundle.
OCI layers are applied in the declared order. Regular files, directories, and
OCI whiteouts are supported. Symlink, device, socket, FIFO, absolute path, and
parent traversal archive entries fail closed.

Directory admission and copying use the pinned Bounded Tree component. The
intake shell retains topology meaning, image and layer policy, bundle layout,
and receipt authority. Bounded Tree supplies deterministic tree observation,
revalidation, and copy mechanics only.

The current limits are 32 services, 64 layers per service, 8,192 tree members,
128 MiB per file, and 1 GiB per admitted tree. Bundle publication uses a
same-parent staging directory and one rename. An existing output path is never
replaced.

## Output

The bundle contains:

- `process-manifest.json`: input for `chaoscontrol-guest-supervisor`.
- `bundle-plan.json`: deterministic lowering and source order.
- `receipt.json`: image, layer, root, process-manifest, and bundle identities.
- `services/<role>/root/`: the materialized service roots.

The receipt claim scope is exactly `image-to-guest-bundle-only`. Identity drift,
layer-order drift, root drift, service substitution, and broader claims fail
validation.

Run intake with:

```text
oci-intake --topology topology.json --output guest-bundle
```

## Claim boundary

Intake is packaging. It does not provide namespace isolation, cgroups, registry
behavior, Docker semantics, Kubernetes semantics, image trust, vulnerability
scanning, cross-machine scheduling, or filesystem correctness. A valid receipt
proves identity agreement for the admitted inputs and produced roots. It does
not prove service correctness or a successful VM campaign.
