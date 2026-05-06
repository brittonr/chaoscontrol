# Nickel-authored dogfood receipt contract.
let ArtifactHash = import "artifact-hash.ncl" in
let ReplayAttempt = import "replay-attempt.ncl" in
let CheckpointReference = import "checkpoint-reference.ncl" in
let AcceptanceStatus = std.contract.from_predicate (fun value =>
  value == "accepted" || value == "partial" || value == "known-gap" || value == "invalid" || value == "raw-log-only"
) in
let NonEmptyString = std.contract.from_predicate (fun value => std.is_string value && std.string.length value > 0) in
let Coverage = {
  registered | Number,
  exercised | Number,
  passed | Number,
  failed | Number,
  unexercised | Number,
  summary_path | NonEmptyString,
} in
let BugReceipt = {
  path | NonEmptyString,
  assertion_id | Number,
  tick | Number,
  replay_parent_depth | Number,
  replay_context | NonEmptyString,
  replay_status | AcceptanceStatus,
  replay_attempt | ReplayAttempt,
} in
let ConfigDigest = { path | NonEmptyString, digest | NonEmptyString } in
let RawLog = { path | NonEmptyString, policy | NonEmptyString } in
{
  schema_version | NonEmptyString,
  status | AcceptanceStatus,
  acceptance_status | AcceptanceStatus,
  git_revision | NonEmptyString,
  run_id | NonEmptyString,
  command | NonEmptyString,
  config | ConfigDigest,
  kernel_path | NonEmptyString,
  initrd_path | NonEmptyString,
  artifact_hashes | Array ArtifactHash,
  assertion_coverage | Coverage,
  bug_reports | Array BugReceipt,
  checkpoint_reference | CheckpointReference,
  raw_logs | Array RawLog,
}
