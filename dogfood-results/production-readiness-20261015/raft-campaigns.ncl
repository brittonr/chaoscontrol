let RunProfile = import "../../contracts/evidence/run-config.ncl" in
let Base = import "../../contracts/evidence/examples/raft-run-config.ncl" in
let derived = fun id cmdline => Base & {
  profile_id | force = id,
  artifacts | force = {
    kernel = {
      kind = "absolute-path",
      path = "/nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux",
      identity = "blake3:b21d78e8169b34eb776d37d0f3ba15c2d0e58063423823a35a3424fca6e17fc2",
    },
    initrd = {
      kind = "absolute-path",
      path = "/nix/store/vxl9fvbh9mii71ngf1q8sk0gpmcfnh62-chaoscontrol-initrd-raft",
      identity = "blake3:82b2ef606827644e8d10ca98142344e9fe5bae06cc3c11f837f5734654bf646e",
    },
    disk = null,
  },
  guest_cmdline | force = cmdline,
  exploration | force = Base.exploration & {
    # round_stops.rs SHORT_RUN_ROUND_LIMIT: shorter runs stop on the first finding.
    max_rounds | force = 10,
    # Existing CLI frontier bound; smaller than the example profile's 10000.
    max_frontier | force = 50,
  },
} | RunProfile in
{
  schema = "chaoscontrol.production-lineage-campaign-inputs.v1",
  owner = "chaoscontrol-vmm",
  derived_from = "contracts/evidence/examples/raft-run-config.ncl",
  timeout_seconds = 600,
  timeout_source = "contracts/kvm-release/matrix.ncl FreshReplayTimeoutSeconds",
  workers = 1,
  clock_boundary = "Existing explorer CLI clock profile; separate from the explicit hide-TSC continuation proof. No paired fresh-campaign repeatability claim.",
  control = derived "production-lineage-control-ten-round" "raft_bug=none",
  planted = derived "production-lineage-planted-ten-round" "raft_bug=fig8_commit",
  non_claims = [
    "An operational watchdog, failed branch, or absent finding is not a successful deep replay.",
    "Planted-bug evidence is distinct from the no-planted-bug continuation proof.",
    "A passing run does not establish release eligibility or universal determinism.",
  ],
}
