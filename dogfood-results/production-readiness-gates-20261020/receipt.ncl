# Append-only gate evidence. Historical receipts retain their original verdicts.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "observations.json" in
let M = import "validated-matrix.json" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let Revision = "29329c854338702457eb9967c4830b79487a8901" in
let ExpectedRows = [
  "build-release-binaries", "build-raft-guest", "deterministic-smp",
  "serialized-snapshot-replay", "virtio-malformed-input", "admitted-drift",
  "fresh-workload-replay",
] in
let Source = O.source | {
  revision | std.contract.Equal Revision,
  dirty | std.contract.Equal false,
} in
let MatrixSource = M.source | std.contract.Equal Source in
let Rows = O.matrix.rows in
let RowIds = std.array.map (fun row => row.id) Rows in
let CuratedRowIds = std.array.map (fun row => row.id) M.rows in
let KvmReady = O.commands.matrix.exit_code == 0
  && !O.commands.matrix.operational_timeout
  && O.commands.matrix_validation.exit_code == 0
  && !O.commands.matrix_validation.operational_timeout
  && O.matrix.terminal_class == "release-eligible"
  && M.terminal_class == "release-eligible"
  && MatrixSource == Source
  && RowIds == ExpectedRows && CuratedRowIds == ExpectedRows
  && std.array.all (fun row => row.status == "passed" && row.exit_code == 0) Rows
  && std.array.all (fun row => row.status == "passed") M.rows in
let NixReady = O.commands.nix.exit_code == 0
  && !O.commands.nix.operational_timeout in
let Replay = O.fresh_replay.summary in
let ReplayReady = Replay.accepted && Replay.export_exit_status == 0
  && Replay.reproduce_exit_status == 0 && Replay.verdict.reproduced
  && Replay.verdict.replay_class == "snapshot_backed_reproduced"
  && Replay.verdict.replay_parent_depth > 0
  && Replay.verdict.snapshot_status == "valid" in
{
  schema = "chaoscontrol.unchanged-source-gate-receipt.v1",
  owner = O.owner,
  source = Source,
  outcome = if NixReady && KvmReady then "gate-pair-passed" else "blocked",
  production_ready | std.contract.Equal false = false,
  authority = O.authority | {
    new_corrective_attempts | std.contract.Equal 0,
    last_numbered_ledger | String,
    last_numbered_ledger_used | std.contract.Equal 47,
    boundary | String,
  },
  nix_ready | std.contract.Equal NixReady = NixReady,
  nix_failure = O.nix_failure,
  kvm_release_matrix_ready | std.contract.Equal KvmReady = KvmReady,
  independent_validation = M,
  verified_row_artifacts | std.contract.Equal 138 = O.verified_row_artifacts,
  fresh_replay_ready | std.contract.Equal ReplayReady = ReplayReady,
  fresh_replay = O.fresh_replay,
  exact_commands = O.commands,
  evidence_artifacts | Array Artifact = O.artifacts,
  historical_receipts | Array Artifact = O.historical_receipts,
  causal_corrections = O.causal_corrections,
  remaining_prerequisites = O.remaining_prerequisites,
  boundary = O.boundary,
}
