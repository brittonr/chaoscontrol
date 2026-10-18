# Availability evidence only. The original readiness receipt remains authority.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "observations.json" in
let M = import "evidence-manifest.json" in
let Prior = import "../production-readiness-attempts41-45-20261018/receipt.ncl" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let RawArtifact = {
  id | P.identifier,
  path | String,
  digest | P.blake3,
  bytes | P.u64,
} in
let Authority = O.authority | {
  maximum_corrective_attempts | std.contract.Equal 45,
  used_corrective_attempts | std.contract.Equal 45,
  new_product_corrections | std.contract.Equal 0,
  release_gates_rerun | std.contract.Equal false,
  host_configuration_changed | std.contract.Equal false,
  credentials_accessed | std.contract.Equal false,
  user_decision | std.contract.Equal "Keep 45-attempt limit",
} in
let RequiredArtifact = O.required_artifact | {
  store_path | std.contract.Equal
    "/nix/store/vxl9fvbh9mii71ngf1q8sk0gpmcfnh62-chaoscontrol-initrd-raft",
  expected_content_digest | P.blake3 | std.contract.Equal
    "blake3:82b2ef606827644e8d10ca98142344e9fe5bae06cc3c11f837f5734654bf646e",
  substitute_with_different_identity_accepted | std.contract.Equal false,
} in
let Endpoints = O.endpoints | Array {
  id | P.identifier,
  visibility | P.closed_enum ["public", "private"],
  url | P.nullable_non_empty_string,
  tool | std.contract.Equal "read",
  observed_http_status | std.contract.Equal 404,
  raw_observation | P.closed_enum ["public", "private"],
  raw_query_index | P.u32,
} in
let RawInputs = M.raw_local_inputs | Array RawArtifact in
let EndpointInventory = std.array.length Endpoints == 6
  && P.unique_by (fun endpoint => endpoint.id) Endpoints
  && std.array.all (fun id => std.array.any (fun endpoint =>
    endpoint.id == id) Endpoints)
    ["nixos", "nix-community", "devenv", "spectrum",
      "configured-private-1", "configured-private-2"]
  && std.array.length (std.array.filter (fun endpoint =>
    endpoint.visibility == "public") Endpoints) == 4
  && std.array.length (std.array.filter (fun endpoint =>
    endpoint.visibility == "private") Endpoints) == 2
  && std.array.all (fun endpoint =>
    endpoint.raw_observation == endpoint.visibility
    && (if endpoint.visibility == "private" then endpoint.url == null
      else endpoint.url != null && std.string.is_match "^https://" endpoint.url)
    && std.array.any (fun artifact =>
      artifact.id == endpoint.raw_observation) RawInputs
  ) Endpoints in
let IdentityBoundary = M.identity_algorithm == "blake3"
  && !M.raw_artifacts_copied && !M.historical_inputs_modified
  && std.array.length M.curated_inputs == 1
  && std.array.length M.prior_authority_inputs == 6
  && std.array.length RawInputs == 2
  && P.unique_by (fun artifact => artifact.id) RawInputs
  && P.unique_by (fun endpoint => {
    observation = endpoint.raw_observation,
    index = endpoint.raw_query_index,
  }) Endpoints in
let SourceBoundary = O.source.implementation_revision == Prior.source.revision
  && !Prior.source.dirty
  && O.source.delivery_parent_revision
    == "a16289337fb42c485319a4576495849d39b5dd9b" in
let ReadinessBoundary = !Prior.production_ready && Prior.outcome == "blocked"
  && Authority.used_corrective_attempts == Prior.implementation_attempts.used
  && Authority.maximum_corrective_attempts
    == Prior.implementation_attempts.authorized_maximum
  && !O.artifact_recovered && !O.production_ready in
{
  schema = "chaoscontrol.historical-initrd-availability-receipt.v1",
  maintenance_owner = O.maintenance_owner,
  immediate_outcome = O.immediate_outcome,
  durable_capability = O.durable_capability,
  current_consumer = O.current_consumer,
  source = O.source,
  publication_prepared_at_utc = O.publication_prepared_at_utc,
  authority = Authority,
  evidence_class = "bounded-configured-cache-metadata-observation",
  observations_validated | std.contract.Equal true =
    EndpointInventory && IdentityBoundary && SourceBoundary && ReadinessBoundary,
  outcome | std.contract.Equal "missing-artifact-blocked" = "missing-artifact-blocked",
  artifact_recovered | std.contract.Equal false = O.artifact_recovered,
  production_ready | std.contract.Equal false = Prior.production_ready,
  required_artifact = RequiredArtifact,
  discovery_command = O.discovery_command,
  requested_metadata_path = O.requested_metadata_path,
  endpoints = Endpoints,
  private_endpoint_boundary = O.private_endpoint_boundary,
  curated_inputs | Array Artifact = M.curated_inputs,
  raw_local_inputs = RawInputs,
  prior_authority_inputs | Array Artifact = M.prior_authority_inputs,
  original_required_criteria = Prior.required_criteria,
  next_action = O.next_action,
  limitations = O.limitations,
}
