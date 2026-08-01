# Closed review contract for Rust-owned replay verdict artifacts.
let MaxNamespaceBytes = 128 in
let MaxKeyBytes = 256 in
let MaxMessageBytes = 1024 in
let MaxSourceBytes = 512 in
let MaxGuestBytes = 128 in
let MaxCategoryBytes = 64 in
let MaxCanonicalBytes = 2304 in
let U32Maximum = 4294967295 in
let ExactVersion = std.contract.from_predicate (fun value => value == 2) in
let NonEmptyString = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value > 0
) in
let BoundedString = fun maximum => std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value > 0
  && std.string.length value <= maximum
  && !(std.string.is_match "[[:cntrl:]]" value)
) in
let U32 = std.contract.from_predicate (fun value =>
  std.is_number value
  && std.number.is_integer value
  && value >= 0
  && value <= U32Maximum
) in
let PositiveU32 = std.contract.from_predicate (fun value =>
  std.is_number value
  && std.number.is_integer value
  && value > 0
  && value <= U32Maximum
) in
let Byte = std.contract.from_predicate (fun value =>
  std.is_number value
  && std.number.is_integer value
  && value >= 0
  && value <= 255
) in
let CanonicalBytes = std.contract.from_predicate (fun value =>
  std.is_array value
  && std.array.length value > 0
  && std.array.length value <= MaxCanonicalBytes
) in
let Fingerprint = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value
) in
let Category = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value > 0
  && std.string.length value <= MaxCategoryBytes
  && std.string.is_match "^[a-z0-9]([a-z0-9-]*[a-z0-9])?$" value
) in
let AssertionKind = std.contract.from_predicate (fun value =>
  value == "always"
  || value == "sometimes"
  || value == "reachable"
  || value == "unreachable"
) in
let LogicalKeyTag = std.contract.from_predicate (fun value =>
  value == "automatic" || value == "stable" || value == "legacy_u32"
) in
let LogicalKeyShape = {
  type | LogicalKeyTag,
  source_site | BoundedString MaxKeyBytes | optional,
  key | BoundedString MaxKeyBytes | optional,
  id | U32 | optional,
} in
let LogicalKey = std.contract.custom (fun label value =>
  std.contract.check LogicalKeyShape label value
  |> match {
    'Ok checked =>
      let has_source = std.record.has_field "source_site" checked in
      let has_key = std.record.has_field "key" checked in
      let has_id = std.record.has_field "id" checked in
      let valid =
        if checked.type == "automatic" then has_source && !has_key && !has_id
        else if checked.type == "stable" then !has_source && has_key && !has_id
        else !has_source && !has_key && has_id
      in
      if valid then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
) in
let Descriptor = {
  identity_version | std.contract.from_predicate (fun value => value == 1),
  namespace | BoundedString MaxNamespaceBytes,
  logical_key | LogicalKey,
  compatibility_id | U32 | optional,
  kind | AssertionKind,
  message | BoundedString MaxMessageBytes,
  source_file | BoundedString MaxSourceBytes,
  source_line | PositiveU32,
  source_column | PositiveU32,
  guest | BoundedString MaxGuestBytes,
  category | Category,
} in
let AssertionIdentity = {
  descriptor | Descriptor,
  fingerprint | Fingerprint,
  canonical_descriptor | Array Byte | CanonicalBytes,
  catalog_token | Fingerprint,
} in
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
let ArtifactHash = {
  path | NonEmptyString,
  sha256 | NonEmptyString,
} in
let ReplayVerdictShape = {
  schema_version | ExactVersion,
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
  assertion_id | U32 | optional,
  assertion_identity | AssertionIdentity | optional,
  replay_parent_depth | Number | optional,
  snapshot = {
    status | SnapshotStatus,
    present | Bool,
    digest_verified | Bool,
    reference | (import "./snapshot-ref.ncl") | optional,
    diagnostic | NonEmptyString | optional,
  },
  artifact_hashes | Array ArtifactHash,
} in
let ReplayBinding = fun value =>
  let has_path = std.record.has_field "bug_path" value in
  let has_bug = std.record.has_field "bug_id" value in
  let has_alias = std.record.has_field "assertion_id" value in
  let has_identity = std.record.has_field "assertion_identity" value in
  let has_depth = std.record.has_field "replay_parent_depth" value in
  let all_bug_fields = has_path && has_bug && has_alias && has_identity && has_depth in
  let no_bug_fields = !has_path && !has_bug && !has_alias && !has_identity && !has_depth in
  let alias_matches =
    if !all_bug_fields then true
    else if std.record.has_field "compatibility_id" value.assertion_identity.descriptor then
      value.assertion_identity.descriptor.compatibility_id == value.assertion_id
    else
      value.assertion_id == 0
  in
  (all_bug_fields || no_bug_fields) && alias_matches
in
std.contract.custom (fun label value =>
  std.contract.check ReplayVerdictShape label value
  |> match {
    'Ok checked => if ReplayBinding checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
