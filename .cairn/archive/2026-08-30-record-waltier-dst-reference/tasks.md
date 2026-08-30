# Tasks: Record WalTier DST reference

## Phase 1: Reference record

- [x] [serial] Add WalTier DST to ChaosControl comparison-source documentation beside the Antithesis source, with explicit layer and claim boundaries. r[chaoscontrol.waltier_dst.source]
- [x] [serial] Name history monotonicity, exact-prefix state, and snapshot-object conservation. r[chaoscontrol.waltier_dst.oracle]
- [x] [serial] State that store-seam simulation is a distinct mechanism layer from KVM guest simulation and imposes no new ChaosControl gates. r[chaoscontrol.waltier_dst.boundary]

## Phase 2: Verification

- [x] [parallel] Verify the record preserves the existing Antithesis comparison-source posture and repo policy. r[chaoscontrol.waltier_dst.boundary]
- [x] [parallel] Run package, workspace, Clippy, Cairn, and Nix checks and document non-claims. r[chaoscontrol.waltier_dst.verification]
