## Tasks

- [x] [serial] Confirm the documented container-intake non-goal and the multiprocess supervisor dependency surface. r[chaoscontrol.oci_intake.bundle_lowering]
- [ ] [depends:oci-intake-baseline] Define the Nickel topology contract and deterministic bundle manifest projection. r[chaoscontrol.oci_intake.nickel_topology]
- [ ] [depends:oci-intake-topology] Implement intake lowering from the topology into a guest bundle with manifest entries and shared bindings. r[chaoscontrol.oci_intake.bundle_lowering]
- [ ] [depends:oci-intake-lowering] Add admitted image extraction with declared order and typed unsupported-format rejection. r[chaoscontrol.oci_intake.image_boundary]
- [ ] [depends:oci-intake-image] Bind image, layer, root, and manifest identities into receipts with fail-closed validation. r[chaoscontrol.oci_intake.provenance]
- [ ] [parallel] Add positive multi-image fixtures and negative topology, format, root, and provenance fixtures. r[chaoscontrol.oci_intake.validation]
- [ ] [depends:oci-intake-validation] Update product-scope facts and roadmap, then run intake, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.oci_intake.claim_boundary] r[chaoscontrol.oci_intake.validation]
