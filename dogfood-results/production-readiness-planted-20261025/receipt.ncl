# Replacement evidence under the user's explicit artifact-disposition decision.
# Historical receipts and verdicts remain unchanged.
let P = import "../../contracts/evidence/profile-primitives.ncl" in
let Prior = import "../production-readiness-mcp53-20261020/receipt.ncl" in
let O = import "observations.json" in
let Artifact = { path | String, digest | P.blake3, bytes | P.u64 } in
let Source = O.source_revision | std.contract.Equal "ef778be7ecc0467f176857dac779a262b9b741d1" in
let Campaign = fun id => std.array.first (std.array.filter (fun row => row.id == id) O.campaigns) in
let Control = Campaign "control" in
let Planted = Campaign "planted" in
let Probe = Campaign "deep-probe" in
let ReplayPassed = fun row => row.passed && row.parent_depth >= 3 in
let CampaignsReady =
  std.array.length O.campaigns == 3
  && std.array.all (fun row =>
    !row.execution.operational_timeout
    && row.execution.timeout_seconds == 600
    && row.rounds_completed == 10 && row.total_branches_run == 20
    && row.execution.exit_code == (if row.retained_findings > 0 then 1 else 0)
    && row.assertion_totals.catalog_status == "accepted"
    && std.array.length row.deep_safety_ids == std.array.length row.replays
    && std.array.all ReplayPassed row.replays
  ) O.campaigns
  && Control.maximum_admitted_depth >= 4
  && Planted.maximum_admitted_depth >= 4
  && std.array.all (fun depth =>
    std.array.any (fun row => row.depth == depth && row.captures > 0) Control.overlay_sizes_by_depth
    && std.array.any (fun row => row.depth == depth && row.captures > 0) Planted.overlay_sizes_by_depth
  ) [1, 2, 3, 4] in
let ReplacementReady =
  CampaignsReady && ReplayPassed O.selected
  && std.array.length Probe.replays > 0
  && std.array.any (fun row => row.bug_id == O.selected.bug_id && row.verdict == O.selected.verdict) Probe.replays in
let ApprovedDisposition =
  O.authorization.reselect_current_source_finding
  && O.authorization.record_historical_payloads_unattainable
  && !O.authorization.additional_product_changes in
let InheritedGatesReady =
  Prior.mcp_correction_ready
  && Prior.source.revision == Source && !Prior.source.dirty
  && Prior.historical_runtime_source.reused_campaign_depth_four_ready
  && Prior.historical_runtime_source.retained_deep_replays_ready
  && Prior.historical_runtime_source.strict_watchdog_ready
  && Prior.historical_runtime_source.current_evidence_verified
  && O.inherited_matrix_artifact_verification.all_verified
  && O.inherited_matrix_artifact_verification.artifacts_checked == 138
  && std.array.length O.inherited_artifact_verification.failures == 1
  && (std.array.first O.inherited_artifact_verification.failures).path == O.historical_recovery_record.current.path
  && O.historical_recovery_record.superseded_for_approved_review in
{
  schema = "chaoscontrol.replacement-planted-receipt.v1",
  source_revision = Source,
  owner = "chaoscontrol-vmm",
  maintenance_owner = "ChaosControl VMM, guest-kernel and release-evidence maintainers",
  immediate_outcome = "Snapshot-backed reproduction of an explicitly reselected current-source planted finding",
  durable_capability = "Repeatable unchanged-bound campaign, admitted checkpoint export and retained-parent replay using the existing explorer",
  current_consumer = "ChaosControl production-readiness review under the approved artifact disposition",
  inherited_release_evidence = {
    receipt = "../production-readiness-mcp53-20261020/receipt.ncl",
    tested_source = Prior.source,
    ready = InheritedGatesReady,
    native = Prior.native_evidence,
    artifact_verification = O.inherited_artifact_verification,
    matrix_artifact_verification = O.inherited_matrix_artifact_verification,
    historical_recovery_record = O.historical_recovery_record,
    kvm = Prior.kvm_evidence,
    boundary = "Historical exact-source release verdicts, not fresh release tests on this evidence-only delivery",
  },
  implementation_attempts = Prior.implementation_attempts,
  additional_implementation_attempts | std.contract.Equal 0 = 0,
  approval = O.authorization,
  approved_disposition | std.contract.Equal true = ApprovedDisposition,
  commands = O.commands,
  measured_logging_environment = O.measured_logging_environment,
  retained_boot_artifacts = O.retained_boot_artifacts,
  initial_unmeasured_attempt = {
    observation = O.initial_unmeasured_cohort,
    commands = O.initial_unmeasured_commands,
    input_retention_gap = O.initial_unmeasured_input_gap,
  },
  campaign_timeout_seconds | std.contract.Equal 600 = O.campaign_timeout_seconds,
  replay_timeout_seconds | std.contract.Equal 300 = O.replay_timeout_seconds,
  campaigns = O.campaigns,
  campaigns_ready | std.contract.Equal true = CampaignsReady,
  selected = O.selected,
  replacement_snapshot_replay_ready | std.contract.Equal true = ReplacementReady,
  historical_original_reproduced | std.contract.Equal false = false,
  historical_failed_payloads_recovered | std.contract.Equal false = false,
  original_unamended_acceptance_ready | std.contract.Equal false = false,
  production_ready | std.contract.Equal false = false,
  approved_review_ready | std.contract.Equal true = InheritedGatesReady && ReplacementReady && ApprovedDisposition,
  outcome = "approved-replacement-evidence-complete",
  artifacts | Array Artifact = O.artifacts,
  non_claims = O.non_claims,
}
