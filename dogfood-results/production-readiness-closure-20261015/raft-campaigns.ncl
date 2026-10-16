let RunProfile = import "../../contracts/evidence/run-config.ncl" in
let Base = import "../../contracts/evidence/examples/raft-run-config.ncl" in
let Artifacts = {
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
} in
let derived = fun id cmdline => Base & {
  profile_id | force = id,
  artifacts | force = Artifacts,
  guest_cmdline | force = cmdline,
  exploration | force = Base.exploration & {
    max_rounds | force = 10,
    max_frontier | force = 50,
  },
  # Exact CLI bitmap configuration, not the example's blind mode.
  coverage | force = { mode = "bitmap", bitmap_gpa = 917504 },
} | RunProfile in
{
  schema = "chaoscontrol.production-lineage-campaign-inputs.v2",
  owner = "chaoscontrol-vmm",
  derived_from = "contracts/evidence/examples/raft-run-config.ncl",
  timeout_seconds = 600,
  timeout_source = "Unchanged operator bound recorded by the historical and closure execution observations; not the release matrix's separate 3600-second fresh-replay bound",
  artifact_boundary = "These retained unpatched artifacts bind the last complete strict continuation and accepted planted probe. Failed kernel cohorts 18 and 20 retain their own concrete argv, projections and hashes; their patches were backed out rather than relabeled as passing evidence.",
  workers = 1,
  diagnostic_environment = { RUST_LOG = "chaoscontrol_explore::explorer=debug,chaoscontrol_vmm::vm=info" },
  overlay_measurement = "Existing incremental snapshot debug records report total retained overlay pages; multiply by snapshot PAGE_SIZE=4096 for payload bytes, not encoded-artifact bytes",
  clock_boundary = "Existing explorer single-vCPU visible-TSC profile; the strict watchdog continuation proof separately selects hide-TSC. No universal or paired seed-repeatability claim.",
  control = derived "readiness-closure-control-ten-round" "raft_bug=none",
  planted = derived "readiness-closure-fig8-ten-round" "raft_bug=fig8_commit",
  selected_probe | RunProfile = Base & {
    profile_id | force = "readiness-closure-admitted-snapshot-probe",
    artifacts | force = Artifacts,
    seed | force = 0,
    guest_cmdline | force = "raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=1",
    topology | force = Base.topology & { memory_mib | force = 256 },
    exploration | force = Base.exploration & {
      ticks_per_branch | force = 80,
      max_frontier | force = 50,
    },
    coverage | force = { mode = "bitmap", bitmap_gpa = 917504 },
  },
  selected_probe_authority = "contracts/fresh-workload-proofs/cohort.ncl raft row; seed zero from contracts/kvm-release/matrix.ncl fresh-workload-replay",
  non_claims = [
    "An operational timeout, failed branch, or absent finding is not a guest bug or successful replay.",
    "The admitted snapshot replay probe is intentional mechanism-validation evidence, not discovery or reproduction of a fig8 algorithm defect.",
    "Campaign completion does not establish native readiness, release eligibility, universal determinism, or exhaustive exploration.",
    "This current projection corrects the historical profile's blind-mode description; historical receipts remain unchanged and are not promoted.",
  ],
}
