# Nickel-authored dogfood receipt contract.
let ArtifactHash = import "artifact-hash.ncl" in
let ReplayAttempt = import "replay-attempt.ncl" in
let CheckpointReference = import "checkpoint-reference.ncl" in
let AcceptanceStatus = std.contract.from_predicate (fun value =>
  value == "accepted" || value == "partial" || value == "known-gap" || value == "invalid" || value == "raw-log-only"
) in
let NonEmptyString = std.contract.from_predicate (fun value => std.is_string value && std.string.length value > 0) in
# r[impl chaoscontrol.campaign_policy.evidence]
# Campaign/Choregraph structural artifacts never satisfy product evidence.
# Mirrors crates/chaoscontrol-evidence/src/campaign_evidence_boundary.rs.
let CampaignJournalDirectory = "campaign-journal" in
let FrontierTraceFile = "frontier-trace.jsonl" in
let JournalSegmentPattern = "^seg-[0-9]+\\.cbor$" in
let StructuralSchemas = [
  "chaoscontrol.campaign-journal-segment.v1",
  "chaoscontrol.frontier-trace.v1",
  "chaoscontrol.campaign-projection.v1",
  "chaoscontrol.campaign-adapter.v1",
  "chaoscontrol.exploration-moment.v1",
] in
let StructuralSchemaNamespaces = "^(campaign|choregraph)\\." in
let path_components = fun path =>
  std.string.split "/" (std.string.replace "\\" "/" (std.string.lowercase path))
in
let is_structural_path = fun path =>
  let components = path_components path in
  std.array.elem CampaignJournalDirectory components
  || std.array.elem FrontierTraceFile components
  || std.array.any (fun component => std.string.is_match JournalSegmentPattern component) components
in
# r[impl chaoscontrol.campaign_policy.rollback]
# Legacy-diagnostic rollback outputs never satisfy dogfood evidence.
# Mirrors crates/chaoscontrol-evidence/src/frontier_policy_gate.rs.
let LegacyDiagnosticPolicyReceipt = "frontier-policy.json" in
let is_legacy_diagnostic_path = fun path =>
  let components = path_components path in
  std.array.length components > 0
  && std.array.last components == LegacyDiagnosticPolicyReceipt
in
let is_structural_schema = fun schema =>
  let normalized = std.string.lowercase (std.string.trim schema) in
  std.array.elem normalized StructuralSchemas
  || std.string.is_match StructuralSchemaNamespaces normalized
in
let ProductArtifactPath = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value > 0
  && !(is_structural_path value)
  && !(is_legacy_diagnostic_path value)
) in
let ProductSchema = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value > 0 && !(is_structural_schema value)
) in
let ProductArtifactRef = { path | ProductArtifactPath, .. } in
let Coverage = {
  registered | Number,
  exercised | Number,
  passed | Number,
  failed | Number,
  unexercised | Number,
  summary_path | ProductArtifactPath,
} in
let BugReceipt = {
  path | ProductArtifactPath,
  assertion_id | Number,
  tick | Number,
  replay_parent_depth | Number,
  replay_parent_snapshot_ref | Dyn | default = null,
  replay_context | NonEmptyString,
  replay_status | AcceptanceStatus,
  replay_attempt | ReplayAttempt,
} in
let ConfigDigest = { path | ProductArtifactPath, digest | NonEmptyString } in
let RawLog = { path | NonEmptyString, policy | NonEmptyString } in
{
  schema_version | ProductSchema,
  status | AcceptanceStatus,
  acceptance_status | AcceptanceStatus,
  git_revision | NonEmptyString,
  run_id | NonEmptyString,
  command | NonEmptyString,
  config | ConfigDigest,
  kernel_path | NonEmptyString,
  initrd_path | NonEmptyString,
  artifact_hashes | Array ArtifactHash | Array ProductArtifactRef,
  assertion_coverage | Coverage,
  bug_reports | Array BugReceipt,
  checkpoint_reference | CheckpointReference | ProductArtifactRef,
  raw_logs | Array RawLog,
}
