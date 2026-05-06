# Rust-derived replay verdict artifact emitted by chaoscontrol-explore.
let NonEmptyString = std.contract.from_predicate (fun value => std.is_string value && std.string.length value > 0) in
let ReplayClass = std.contract.from_predicate (fun value =>
  value == "snapshot_backed_reproduced"
  || value == "snapshot_backed_not_reproduced"
  || value == "schedule_only_replay_gap"
  || value == "missing_snapshot_ref"
  || value == "missing_snapshot_artifact"
  || value == "invalid_snapshot_digest"
  || value == "no_bug_found"
  || value == "replay_error"
) in
let SnapshotStatus = std.contract.from_predicate (fun value =>
  value == "not_required"
  || value == "missing_ref"
  || value == "valid"
  || value == "missing_artifact"
  || value == "invalid_digest"
  || value == "invalid_ref"
) in
{
  schema_version | Number,
  run_id | NonEmptyString,
  replay_class | ReplayClass,
  reproduced | Bool,
  command = {
    command | NonEmptyString,
    exit_status | Number,
  },
  diagnostic | NonEmptyString,
  bug_path | NonEmptyString | optional,
  bug_id | Number | optional,
  assertion_id | Number | optional,
  replay_parent_depth | Number | optional,
  snapshot = {
    status | SnapshotStatus,
    present | Bool,
    digest_verified | Bool,
    reference | Dyn | optional,
    diagnostic | NonEmptyString | optional,
  },
  artifact_hashes | Array Dyn,
}
