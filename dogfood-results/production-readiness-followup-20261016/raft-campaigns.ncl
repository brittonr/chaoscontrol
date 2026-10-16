let Prior = import "../production-readiness-closure-20261015/raft-campaigns.ncl" in
let RunProfile = import "../../contracts/evidence/run-config.ncl" in
let Artifacts = {
  kernel = {
    kind = "absolute-path",
    path = "/nix/store/bv30bckqdvkp7w7sc0frfy30iz3rzyfa-chaoscontrol-vmlinux/vmlinux",
    identity = "blake3:51b571a44f01a46ba4e4e63b4e9dd26c93b301a2b6feac51ef7455111ec2fa2b",
  },
  initrd = {
    kind = "absolute-path",
    path = "/nix/store/mk5j30m6xn8irij2pgbn4hwfl8b6sdq1-chaoscontrol-initrd-raft",
    identity = "blake3:af7e9608d2a7a77ce535f04b54b1410913ca9bea5e8c43b938134c6fc9259cf3",
  },
  disk = null,
} in
let derived = fun prior id cmdline => (
  std.record.update "profile_id" id (
    std.record.update "artifacts" Artifacts (
      std.record.update "guest_cmdline" cmdline prior
    )
  )
) | RunProfile in
Prior & {
  schema | force = "chaoscontrol.production-lineage-campaign-followup-inputs.v1",
  derived_from | force = "dogfood-results/production-readiness-closure-20261015/raft-campaigns.ncl",
  clock_boundary | force = "Explicit Linux clocksource=jiffies notsc through the existing guest command-line interface. CPU feature visibility and public VM defaults are unchanged. No universal determinism claim.",
  artifact_boundary | force = "Current source-built patched guest kernel and retained initrd, independently BLAKE3-bound. Preserve the earlier exact CLI bitmap/frontier binding; no historical input or receipt is rewritten.",
  control | force = derived Prior.control "production-lineage-control-jiffies-ten-round" "raft_bug=none clocksource=jiffies notsc",
  planted | force = derived Prior.planted "production-lineage-planted-jiffies-ten-round" "raft_bug=fig8_commit clocksource=jiffies notsc",
  selected_probe | force = derived Prior.selected_probe "production-lineage-selected-snapshot-probe-jiffies" "raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=1 clocksource=jiffies notsc",
  deep_probe | force = derived Prior.planted "production-lineage-deep-snapshot-probe-jiffies-ten-round" "raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=6000 clocksource=jiffies notsc",
  deep_probe_boundary = "Separate existing-guest mechanism validation. Threshold 6000 is a guest iteration, not a larger VMM or wall-clock bound. Require actual retained parent depth >=3 and snapshot_backed_reproduced for every retained deep safety finding; do not infer an algorithm bug or alter the release cohort.",
}
