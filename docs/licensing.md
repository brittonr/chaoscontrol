# ChaosControl license map

## Repository-owned source

Future revisions of repository-owned ChaosControl source use `AGPL-3.0-or-later`.

This policy includes these paths:

- all packages under `crates/`;
- Rust tools under `tools/` and repository-owned scripts;
- configuration, contracts, audits, and checked fixtures;
- `.cairn/` lifecycle material and project documentation;
- repository-owned workload templates under `docs/templates/`;
- generated source that contains only repository-owned material.

Each Cargo package and copyable template includes the complete AGPL text. The workspace package metadata uses the same license expression.

## Exclusions and retained terms

This policy does not replace terms that ChaosControl does not own.

These items retain their governing terms:

- third-party Cargo and Nix dependencies;
- imported or upstream-derived source and assets;
- kernels, firmware, guest images, and external workloads;
- generated material that contains upstream source;
- immutable earlier releases and grants.

`bug-hunt-results/`, `dogfood-results/`, reports, traces, receipts, VM output, and unrelated workload output are not automatically relicensed. Processing a workload does not by itself relicense unrelated workload source or output.

The dependency-license policy keeps third-party AGPL review fail-closed. It permits AGPL only for enumerated repository-owned packages and reviewed exceptions.

## Prior grants

Revision `c169afc3d37698f816b54238c03fbc36d3ea1aa3` is the last published revision before the unified repository-owned policy.

The first unified implementation revision is `FIRST_UNIFIED_REVISION`. A later closeout commit records this immutable identity because a commit cannot contain its own identity.

Earlier Apache-2.0 releases and grants remain valid. This policy does not revoke or narrow rights already granted.

## Template distribution

Repository-owned template source is `AGPL-3.0-or-later`. The template manifest, source headers, README, and included `LICENSE` state that rule.

Copying a template does not relicense unrelated application source. Users must review the template license before distribution.

## Complete texts

- `LICENSE` and `LICENSES/AGPL-3.0-or-later.txt` contain the complete AGPL text.
- `LICENSES/Apache-2.0.txt` remains for prior grants and third-party references.

Package metadata, license files, templates, and policy checks are distribution facts. They do not change deterministic VM, replay, snapshot, or evidence identities unless a versioned schema includes them.
