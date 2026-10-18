# Diagnostic evidence only. The original complete readiness receipt remains authority.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let O = import "observations.json" in
let I = import "reproduction-inputs.json" in
let M = import "evidence-manifest.json" in
let Prior = import "../production-readiness-attempts41-45-20261018/receipt.ncl" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let Authority = O.authority | {
  maximum_corrective_attempts | std.contract.Equal 45,
  used_corrective_attempts | std.contract.Equal 45,
  new_product_corrections | std.contract.Equal 0,
  release_gates_rerun | std.contract.Equal false,
  campaign_bounds_changed | std.contract.Equal false,
  user_decision | std.contract.Equal "Keep current limit",
} in
let Core = O.core_suffix_measurements in
let CoreValidated = std.array.length Core == 2
  && P.unique_by (fun observation => observation.id) Core
  && std.array.all (fun observation =>
    let result = observation.measurement in
    result.all_recorded_state_boundaries_match
    && result.final_complete_state_equal
    && result.settlement_controls > 0
    && result.one_pass_applications == result.settlement_controls
    && result.suffix_applications
      == result.settlement_controls * (result.settlement_controls + 1) / 2
    && result.one_pass_seconds > 0
    && result.held_projection_suffix_seconds > 0
  ) Core in
let Campaigns = O.campaigns in
let CampaignsValidated = std.array.length Campaigns == 3
  && P.unique_by (fun campaign => campaign.id) Campaigns
  && std.array.all (fun id => std.array.any (fun campaign =>
    campaign.id == id && campaign.rounds_completed == 8
    && campaign.operational_timeout && campaign.exit_code == 124
  ) Campaigns) ["control", "planted"]
  && std.array.any (fun campaign => campaign.id == "deep-probe"
    && campaign.rounds_completed == 10 && !campaign.operational_timeout
    && campaign.exit_code == 1) Campaigns
  && std.array.all (fun campaign =>
    campaign.timeout_seconds == 600 && campaign.host_watchdog_timeouts == 0
    && campaign.rounds_completed == std.array.length campaign.rows
    && std.array.all (fun round => round.retained == 50
      && round.settlement_control_events == round.proposals + 4
      && round.source_derived_replayed_suffix_applications
        == round.settlement_control_events * (round.settlement_control_events + 1) / 2
    ) campaign.rows
  ) Campaigns in
let CommandsValidated = std.array.length I.commands == 8
  && P.unique_by (fun command => command.id) I.commands
  && std.array.all (fun command => command.observation.exit_code == 0
    && !command.observation.operational_timeout) I.commands in
let JournalValidated = std.array.length O.journal_measurements == 3
  && std.array.all (fun journal => journal.digest_chain_verified
    && journal.decode_seconds > 0) O.journal_measurements in
let SnapshotValidated = O.snapshot_store_measurement.exact_bytes_equal
  && std.array.length O.snapshot_store_measurement.puts == 2
  && std.array.all (fun put => put.reference_unchanged
    && put.compressed_bytes == O.snapshot_store_measurement.original_artifact.bytes
  ) O.snapshot_store_measurement.puts in
let HistoricalBoundary = !Prior.production_ready && Prior.outcome == "blocked"
  && O.prior_readiness.verdict == Prior.outcome
  && O.prior_readiness.production_ready == Prior.production_ready
  && O.prior_readiness.required_criteria == Prior.required_criteria
  && Prior.implementation_attempts.used == Authority.used_corrective_attempts in
let IdentityBoundary = M.identity_algorithm == "blake3"
  && M.independent_identity_comparisons == 126 && M.identity_mismatches == 0
  && M.published_historical_inputs_verified == 70
  && M.all_historical_input_bytes_unchanged && M.all_cited_source_bytes_unchanged in
let Validated = CoreValidated && CampaignsValidated && CommandsValidated
  && JournalValidated && SnapshotValidated && HistoricalBoundary && IdentityBoundary in
{
  schema = "chaoscontrol.campaign-timing-diagnosis-receipt.v1",
  maintenance_owner = O.maintenance_owner,
  immediate_outcome = O.immediate_outcome,
  durable_capability = O.durable_capability,
  current_consumer = O.current_consumer,
  source = O.source,
  authority = Authority,
  evidence_class = "offline-retained-data-diagnosis",
  diagnosis_validated | std.contract.Equal true = Validated,
  outcome | std.contract.Equal "diagnosed-not-corrected" = "diagnosed-not-corrected",
  production_ready | std.contract.Equal false = Prior.production_ready,
  prior_readiness = O.prior_readiness,
  original_required_criteria = Prior.required_criteria,
  core_suffix_measurements = Core,
  campaign_timing = Campaigns,
  journal_measurements = O.journal_measurements,
  snapshot_store_measurement = O.snapshot_store_measurement,
  curated_inputs | Array Artifact = M.curated_inputs,
  raw_local_manifest | Artifact = M.raw_local_manifest,
  local_diagnosis | Artifact = O.local_diagnosis,
  local_validation | Artifact = O.local_validation,
  source_citations = O.source_citations,
  conclusions = O.conclusions,
  limitations = O.limitations,
  next_action = O.next_action,
  non_claim = O.non_claim,
}
