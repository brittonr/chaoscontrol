# SpaceWasm bundle blocker

The vendor repair lets the broad check reach the existing SpaceWasm admission guard.
The guard rejects the manifest before runtime comparison.

`contracts/evidence/examples/spacewasm-mvp-differential-profile.ncl` admits this manifest digest:

`13058ea2d9913348a203cceff7b58d98b6446610ac80518dc3359b8d7ee57472`

The broad run measured:

`5dd34420136f7d3f98e62df57ea9bc61adf269e74b07ad8521b00335e77eb4d7`

The failed derivation left the Nix store before later inspection.
A fresh build of the same published Mantle input now has a GC root at `/home/brittonr/.cache/cc-spacewasm-bundle-20260905`.
`spacewasm-observed/manifest.json` preserves its exact manifest bytes.
The fresh manifest digest is:

`ded66a4959c9efeda62f2eb3d13a06de6df0ad01a1d53f222c199ab6e66d9eb7`

Its bundle identity is `6dbd889f9b098e1e6f134ced28ab9f3abbb2418bb27588707524d105da94f251`.
The profile instead admits `260f66f8df52b89f5673cbf3f2702d49d3413d45d47ab2baca994185d29e5cb3`.
The host runner still matches the admitted `be8aeb698afdecf6fb608910980292517ed952f122b6447705d4bdae485b0221` digest.
That runner match does not admit the rest of the bundle.

The current `source-checked-flake.log` reaches the same admission guard after the three source batches.
It exits one with the rooted `ded66a4959c9efeda62f2eb3d13a06de6df0ad01a1d53f222c199ab6e66d9eb7` manifest digest, not the admitted `13058ea2d9913348a203cceff7b58d98b6446610ac80518dc3359b8d7ee57472` digest.
The failed check is `/nix/store/5gv23d5kkfckadnihf4l7wjna1myly1h-spacewasm-mvp-differential-check.drv`.
The explicit focused checks and bounded replay pass separately. They do not replace this bundle guard.

The observations establish identity drift, not its cause.
The first manifest is unavailable, so this pass does not claim an exact changed-member diagnosis or a controlled reproducibility result.
A compatible producer bundle or an explicit new-cohort review is necessary before this rail can pass.
No Mantle worktree, provider pin, expected digest, or admission guard changed.
