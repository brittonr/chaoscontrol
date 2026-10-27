//! `ChaosControlConformanceAdapter`: Campaign's conformance port driven through the
//! pure ChaosControl adapter, plus broken-wrapper negative controls.
//!
//! Abstract scores map monotonically into rank dimensions 0 and 1 through the
//! suite's float view and the adapter's IEEE-754 bits encoding. Abstract moments map
//! to product moment facts whose digests are derived from the scenario handle; the
//! scenario has no selection for a moment, so scenario-scoped identities stand in
//! for the campaign, selection, and operation a product child moment binds.

use campaign_choregraph::{
    AdapterResult, CacheValidationRequest, ControlProjectionRequest, EventDraft, EventId,
    ValidatedControlProjection,
};
use campaign_conformance::{
    run_suite, AdapterIdentities, AuthoritySignals, CandidateContext, ClosureContext,
    ConformanceError, ConformanceReport, ConformanceResult, ConsumerShell, EntropyContext,
    ExecutionAuthority, ExecutionGateRequest, FixtureVerdict, GuidanceContext, MomentIdentity,
    MomentMapping, RecordMapping, ResultContext, RevisionContext, ScenarioDraw, ScenarioMoment,
    ScenarioOperation, ScenarioOutcome, ScenarioPreference, ScenarioReason, SuiteProfile,
};
use campaign_core::{
    CacheProjection, CampaignError, CampaignReceipt, CandidateRecord, DuplicateCandidateRule,
    EntropyTicket, ExplorationBounds, FrameWriter, FrontierState, GuidanceRecord,
    GuidanceRevisionRequest, Id, IdKind, ReceiptClaims, ResultObservation, RoleCollapseFixture,
    SelectionDecision, UnobservedClosure,
};

use crate::campaign_adapter::candidates::{build_candidate, operation_profile_id};
use crate::campaign_adapter::ranks::ranked_guidance;
use crate::campaign_adapter::{
    advance_projection, child_moment_draft, child_moment_id, child_payload_id, entropy_ticket,
    execution_authority, finding_ref, generation_profile_id, load_projection, receipt_claims,
    result_observation, root_moment_draft, root_moment_id, score_rank, unobserved_closure,
    validate_cache, AdapterError, AdapterIdentity, AdapterReason, BranchSlot, ChildMomentFacts,
    EntropyDraws, ResultFacts, RetainedChild, RootMomentFacts, MAX_RANK_MAGNITUDE,
};
use crate::explorer::{ExplorationMode, ExplorerConfig};

/// Seed of the conformance adapter's entropy source.
const CONFORMANCE_SEED: u64 = 0x00c0_ffee;
/// Campaign default profile values (`campaign-profile` `default.json` at Campaign `796fe38`).
const DEFAULT_THRESHOLD_PARTS_PER_MILLION: u64 = 100_000;
const DEFAULT_EXPLORATION_ORDER: [u32; 1] = [0];
const DEFAULT_RETENTION_RANK_ORDER: [u32; 1] = [0];
const DEFAULT_MAX_ROOTS: u64 = 256;
const DEFAULT_MAX_CANDIDATES: u64 = 32_768;
const DEFAULT_MAX_CANDIDATE_BATCH: u64 = 256;
const DEFAULT_MAX_RETAINED_CANDIDATES: u64 = 4096;
const DEFAULT_MAX_SELECTIONS: u64 = 32_768;
const DEFAULT_MAX_GUIDANCE_RECORDS: u64 = 65_536;
const DEFAULT_MAX_OUTSTANDING: u64 = 256;
const DEFAULT_MAX_DEPTH: u64 = 4096;
const DEFAULT_MAX_CHILD_MOMENTS: u64 = 256;
const DEFAULT_MAX_DERIVED_RECORDS: u64 = 256;
const DEFAULT_MAX_RANK_DIMENSIONS: u64 = 16;
const DEFAULT_MAX_FINDINGS: u64 = 4096;
const DEFAULT_MAX_CONTROL_EVENTS: u64 = 1_048_576;
const DEFAULT_MAX_RESOURCE_DIMENSIONS: u64 = 32;
const DEFAULT_MAX_OPERATION_SCHEMAS: u64 = 64;
const DEFAULT_MAX_PRUNE_BATCH: u64 = 256;
/// Only resource dimension the product declares.
const SELECTION_UNIT_DIMENSION: u32 = 0;
const SCENARIO_FACTS_LABEL: &[u8] = b"chaoscontrol.conformance.scenario-facts.v1";
const SCENARIO_SCORE_LABEL: &[u8] = b"chaoscontrol.conformance.scenario-score.v1";

/// Campaign default profile with the product's rank magnitude.
fn suite_profile() -> SuiteProfile {
    let bounds = ExplorationBounds {
        bounds_id: Id::absent(),
        max_roots: DEFAULT_MAX_ROOTS,
        max_candidates: DEFAULT_MAX_CANDIDATES,
        max_candidate_batch: DEFAULT_MAX_CANDIDATE_BATCH,
        max_retained_candidates: DEFAULT_MAX_RETAINED_CANDIDATES,
        max_selections: DEFAULT_MAX_SELECTIONS,
        max_guidance_records: DEFAULT_MAX_GUIDANCE_RECORDS,
        max_outstanding: DEFAULT_MAX_OUTSTANDING,
        max_depth: DEFAULT_MAX_DEPTH,
        max_child_moments: DEFAULT_MAX_CHILD_MOMENTS,
        max_derived_records: DEFAULT_MAX_DERIVED_RECORDS,
        max_rank_dimensions: DEFAULT_MAX_RANK_DIMENSIONS,
        max_rank_magnitude: MAX_RANK_MAGNITUDE,
        max_findings: DEFAULT_MAX_FINDINGS,
        max_control_events: DEFAULT_MAX_CONTROL_EVENTS,
        max_resource_dimensions: DEFAULT_MAX_RESOURCE_DIMENSIONS,
        max_operation_schemas: DEFAULT_MAX_OPERATION_SCHEMAS,
        max_prune_batch: DEFAULT_MAX_PRUNE_BATCH,
    }
    .derive()
    .unwrap();
    SuiteProfile {
        bounds,
        duplicate_candidate: DuplicateCandidateRule::Reject,
        exploratory_threshold_parts_per_million: DEFAULT_THRESHOLD_PARTS_PER_MILLION,
        exploration_order: DEFAULT_EXPLORATION_ORDER.to_vec(),
        retention_rank_order: DEFAULT_RETENTION_RANK_ORDER.to_vec(),
    }
}

fn conformance_error(error: AdapterError) -> ConformanceError {
    match error {
        AdapterError::Campaign(error) => ConformanceError::Campaign(error),
        AdapterError::Choregraph(error) => ConformanceError::Adapter(error),
        AdapterError::History(error) => ConformanceError::History(error),
        AdapterError::Rank(_) => ConformanceError::Campaign(CampaignError::InvalidGuidanceShape),
        _ => ConformanceError::InvalidIdentities,
    }
}

fn scenario_digest(role: &[u8], handle: u32) -> [u8; 32] {
    FrameWriter::new(IdKind::SourceFacts, SCENARIO_FACTS_LABEL)
        .bytes(role)
        .u32(handle)
        .finish(IdKind::SourceFacts)
        .digest
}

fn scenario_id(kind: IdKind, handle: u32) -> Id {
    Id {
        kind,
        digest: scenario_digest(b"identity", handle),
    }
}

fn root_facts(handle: u32) -> RootMomentFacts {
    RootMomentFacts {
        epoch: 0,
        config_digest: scenario_digest(b"config", handle),
        seed: CONFORMANCE_SEED,
        bootstrap_tick: u64::from(handle),
        coverage_digest: scenario_digest(b"coverage", handle),
        oracle_digest: scenario_digest(b"oracle", handle),
        base_set_id: scenario_digest(b"base-set", handle),
    }
}

fn child_facts(handle: u32) -> ChildMomentFacts {
    ChildMomentFacts {
        campaign_id: scenario_id(IdKind::Campaign, handle),
        selection_id: scenario_id(IdKind::Selection, handle),
        operation_id: scenario_id(IdKind::Operation, handle),
        branch_slot: BranchSlot::Variant(handle),
        schedule_digest: scenario_digest(b"schedule", handle),
        variant_digest: scenario_digest(b"variant", handle),
        coverage_digest: scenario_digest(b"coverage", handle),
        oracle_digest: scenario_digest(b"oracle", handle),
        total_ticks: u64::from(handle),
        snapshot_tick: u64::from(handle),
        base_set_id: scenario_digest(b"base-set", handle),
    }
}

const fn scenario_reason(reason: ScenarioReason) -> AdapterReason {
    match reason {
        ScenarioReason::Retirement => AdapterReason::SnapshotNotRetained,
        ScenarioReason::Pruning => AdapterReason::RetentionPrune,
        ScenarioReason::Revision => AdapterReason::SelectionCountDecay,
        ScenarioReason::Closure => AdapterReason::SnapshotStateLost,
        ScenarioReason::ConsumerStop => AdapterReason::OperatorSignal,
    }
}

/// Conformance port implemented through the pure ChaosControl adapter.
struct ChaosControlConformanceAdapter {
    identity: AdapterIdentity,
    generation_profile_id: Id,
}

impl ChaosControlConformanceAdapter {
    fn new() -> Self {
        Self {
            identity: AdapterIdentity::for_seed(CONFORMANCE_SEED),
            generation_profile_id: generation_profile_id(&ExplorerConfig::default()).unwrap(),
        }
    }

    fn scored(
        &self,
        preference: &ScenarioPreference,
        campaign_id: Id,
        candidate_id: Id,
    ) -> ConformanceResult<GuidanceRecord> {
        let scores = preference.score.float_view();
        let mut ranks = Vec::with_capacity(scores.len());
        let mut facts = FrameWriter::new(IdKind::SourceFacts, SCENARIO_SCORE_LABEL)
            .id(candidate_id)
            .u64(preference.generation);
        for score in scores {
            ranks.push(score_rank(score).map_err(|error| conformance_error(error.into()))?);
            facts = facts.u64(score.to_bits());
        }
        ranked_guidance(
            &self.identity,
            campaign_id,
            candidate_id,
            preference.generation,
            ranks,
            facts.finish(IdKind::SourceFacts),
        )
        .map_err(conformance_error)
    }
}

impl MomentMapping for ChaosControlConformanceAdapter {
    fn identities(&self) -> ConformanceResult<AdapterIdentities> {
        Ok(AdapterIdentities {
            adapter_id: self.identity.adapter_id,
            projection_schema_id: self.identity.projection_schema_id,
            control_domain_id: self.identity.control_domain_id,
            guidance_profile_id: self.identity.guidance_profile_id,
            entropy_source_id: self.identity.entropy_source_id,
            operation_schema_ids: vec![self.identity.operation_schema_id],
            resource_dimension_ids: vec![self.identity.resource_dimension_id],
        })
    }

    fn exploration_draft(
        &mut self,
        moment: &ScenarioMoment,
        parent_event: Option<EventId>,
    ) -> ConformanceResult<EventDraft> {
        match (moment.parent, parent_event) {
            (None, None) => Ok(root_moment_draft(
                &self.identity,
                &root_facts(moment.handle.0),
            )),
            (Some(_), Some(parent)) => {
                child_moment_draft(&self.identity, &child_facts(moment.handle.0), parent)
                    .map_err(conformance_error)
            }
            _ => Err(ConformanceError::InvalidIdentities),
        }
    }

    fn moment_identity(
        &mut self,
        moment: &ScenarioMoment,
        event: EventId,
    ) -> ConformanceResult<MomentIdentity> {
        if moment.parent.is_none() {
            return Ok(MomentIdentity::Root {
                moment_id: root_moment_id(event),
            });
        }
        Ok(MomentIdentity::Child {
            moment_id: child_moment_id(event),
            event_payload_id: child_payload_id(&child_facts(moment.handle.0))
                .map_err(conformance_error)?,
        })
    }

    fn reason(&mut self, reason: ScenarioReason) -> ConformanceResult<Id> {
        Ok(self.identity.reason_id(scenario_reason(reason)))
    }
}

impl RecordMapping for ChaosControlConformanceAdapter {
    fn candidate(
        &mut self,
        operation: &ScenarioOperation,
        context: &CandidateContext,
    ) -> ConformanceResult<CandidateRecord> {
        let [cost] = operation.costs.as_slice() else {
            return Err(ConformanceError::InvalidIdentities);
        };
        if cost.dimension != SELECTION_UNIT_DIMENSION {
            return Err(ConformanceError::InvalidIdentities);
        }
        build_candidate(
            &self.identity,
            context.campaign_id,
            context.parent_moment_id,
            context.depth,
            operation_profile_id(
                ExplorationMode::FaultSchedule,
                u64::from(operation.handle.0),
                None,
            ),
            self.generation_profile_id,
            cost.units,
        )
        .map_err(conformance_error)
    }

    fn guidance(
        &mut self,
        preference: &ScenarioPreference,
        context: &GuidanceContext,
    ) -> ConformanceResult<GuidanceRecord> {
        self.scored(preference, context.campaign_id, context.candidate_id)
    }

    fn revision(
        &mut self,
        preference: &ScenarioPreference,
        context: &RevisionContext,
    ) -> ConformanceResult<GuidanceRevisionRequest> {
        Ok(GuidanceRevisionRequest {
            candidate_id: context.candidate_id,
            successor: self.scored(preference, context.campaign_id, context.candidate_id)?,
            invalidated_guidance_id: context.invalidated_guidance_id,
            reason_id: self.reason(ScenarioReason::Revision)?,
        })
    }

    fn entropy_ticket(
        &mut self,
        draw: &ScenarioDraw,
        context: &EntropyContext,
    ) -> ConformanceResult<EntropyTicket> {
        entropy_ticket(
            &self.identity,
            context.campaign_id,
            context.state_id,
            context.selection_profile_id,
            context.counter,
            EntropyDraws {
                explore_draw: draw.explore_draw,
                index_draw: draw.index_draw,
            },
        )
        .map_err(conformance_error)
    }

    fn result(
        &mut self,
        outcome: &ScenarioOutcome,
        context: &ResultContext<'_>,
    ) -> ConformanceResult<ResultObservation> {
        let selection = context.selection;
        let findings = (0..outcome.findings)
            .map(|index| {
                finding_ref(
                    u64::from(index),
                    selection.selection_id.digest,
                    selection.parent_moment_id,
                )
            })
            .collect();
        let facts = ResultFacts {
            children: context
                .children
                .iter()
                .map(|child| RetainedChild {
                    moment_id: child.child_moment_id,
                    event_payload_id: child.event_payload_id,
                })
                .collect(),
            findings,
            round_manifest_digest: selection.selection_id.digest,
        };
        result_observation(&self.identity, context.state, selection, &facts)
            .map_err(conformance_error)
    }

    fn closure(&mut self, context: &ClosureContext<'_>) -> ConformanceResult<UnobservedClosure> {
        unobserved_closure(
            &self.identity,
            context.selection,
            scenario_reason(ScenarioReason::Closure),
        )
        .map_err(conformance_error)
    }
}

impl ConsumerShell for ChaosControlConformanceAdapter {
    fn load_history(
        &mut self,
        request: &ControlProjectionRequest<'_>,
        previous: Option<&ValidatedControlProjection>,
    ) -> AdapterResult<ValidatedControlProjection> {
        match previous {
            Some(held) => advance_projection(held, request),
            None => load_projection(request),
        }
    }

    fn validate_cache(
        &mut self,
        request: &CacheValidationRequest<'_>,
        cache: &CacheProjection,
    ) -> AdapterResult<FrontierState> {
        validate_cache(request, cache)
    }

    fn execution_gate(&mut self, request: &ExecutionGateRequest<'_>) -> ExecutionAuthority {
        if execution_authority(request.selection, request.publication, request.evidence) {
            ExecutionAuthority::Granted
        } else {
            ExecutionAuthority::Refused
        }
    }

    fn promotion_claims(
        &mut self,
        _receipt: &CampaignReceipt,
        _signals: &AuthoritySignals,
    ) -> ReceiptClaims {
        receipt_claims()
    }

    fn role_identities(
        &mut self,
        candidate: &CandidateRecord,
        work_item: &SelectionDecision,
        result: &ResultObservation,
    ) -> RoleCollapseFixture {
        RoleCollapseFixture {
            candidate_record_id: candidate.candidate_id,
            queued_work_id: work_item.selection_id,
            computed_result_id: result.result_id,
        }
    }
}

fn failures(report: &ConformanceReport) -> Vec<(&'static str, FixtureVerdict)> {
    report
        .failures()
        .into_iter()
        .map(|outcome| (outcome.name, outcome.verdict.clone()))
        .collect()
}

// r[verify chaoscontrol.campaign_policy.adapter.valid]
// r[verify chaoscontrol.campaign_policy.ranks.valid]
// r[verify chaoscontrol.campaign_policy.entropy.valid]
// r[verify chaoscontrol.campaign_policy.history.valid]
#[test]
fn chaoscontrol_adapter_passes_every_fixture_with_the_reference_transcript() {
    let profile = suite_profile();
    let mut adapter = ChaosControlConformanceAdapter::new();
    let report = run_suite(&mut adapter, &profile);
    assert_eq!(failures(&report), Vec::new());
    assert_eq!(report.outcomes.len(), PINNED_FIXTURE_COUNT);
    assert_eq!(
        report.outcomes.len(),
        campaign_conformance::fixture_names().len()
    );
    assert!(report
        .outcomes
        .iter()
        .all(|outcome| outcome.verdict == FixtureVerdict::Passed));
    let digest = report.transcript.digest().unwrap();
    assert_eq!(digest, reference_pilot_digest(&profile));
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(hex, PINNED_REFERENCE_DIGEST);
}

/// Fixture count of the pinned Campaign conformance suite (Campaign `796fe38`).
const PINNED_FIXTURE_COUNT: usize = 80;
/// Campaign's announced reference transcript digest for the pinned suite on the
/// default profile; both Campaign pilots produce it.
const PINNED_REFERENCE_DIGEST: &str =
    "28b873e9873c97b5b57194bbb499503e05fae0dc16633b9fe0ce844e0328b51b";
/// Campaign's announced reference transcript digest for the pinned suite under
/// `exploration_order = [1]`, `retention_rank_order = [1]`.
const PINNED_REORDERED_DIGEST: &str =
    "37b9ff95d8c8d5b1b175be8e3e8445261d68daadddb3d903530b0186976c5bae";

/// Transcript digest of Campaign's own conforming reference pilot on `profile`.
///
/// The pinned suite's Animus-shaped fixture adapter is the Campaign-side pilot
/// (`campaign.consumer_pilots.valid`); comparing against its run keeps the
/// assertion exact while the suite grows between pins.
fn reference_pilot_digest(profile: &SuiteProfile) -> [u8; 32] {
    let mut pilot = campaign_conformance::fixtures::animus_shaped::OpaqueCandidateAdapter::new();
    let report = run_suite(&mut pilot, profile);
    assert_eq!(failures(&report), Vec::new());
    report.transcript.digest().unwrap()
}

#[test]
fn a_changed_profile_changes_the_transcript_digest() {
    let mut profile = suite_profile();
    profile.exploration_order = vec![1];
    profile.retention_rank_order = vec![1];
    let report = run_suite(&mut ChaosControlConformanceAdapter::new(), &profile);
    assert_eq!(failures(&report), Vec::new());
    let digest = report.transcript.digest().unwrap();
    assert_eq!(digest, reference_pilot_digest(&profile));
    assert_ne!(digest, reference_pilot_digest(&suite_profile()));
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(hex, PINNED_REORDERED_DIGEST);
}

/// The one contract a wrapper breaks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Breakage {
    StaleHistory,
    ExecutionBeforePublication,
    CrossedIdentities,
    BudgetMiscount,
    HistoryRewrite,
    ReceiptOverclaim,
    RoleCollapse,
}

/// ChaosControl adapter wrapped with one breakage.
struct BrokenAdapter {
    inner: ChaosControlConformanceAdapter,
    breakage: Breakage,
    journal: Option<ValidatedControlProjection>,
}

impl BrokenAdapter {
    fn new(breakage: Breakage) -> Self {
        Self {
            inner: ChaosControlConformanceAdapter::new(),
            breakage,
            journal: None,
        }
    }
}

impl MomentMapping for BrokenAdapter {
    fn identities(&self) -> ConformanceResult<AdapterIdentities> {
        self.inner.identities()
    }

    fn exploration_draft(
        &mut self,
        moment: &ScenarioMoment,
        parent_event: Option<EventId>,
    ) -> ConformanceResult<EventDraft> {
        self.inner.exploration_draft(moment, parent_event)
    }

    fn moment_identity(
        &mut self,
        moment: &ScenarioMoment,
        event: EventId,
    ) -> ConformanceResult<MomentIdentity> {
        self.inner.moment_identity(moment, event)
    }

    fn reason(&mut self, reason: ScenarioReason) -> ConformanceResult<Id> {
        self.inner.reason(reason)
    }
}

impl RecordMapping for BrokenAdapter {
    fn candidate(
        &mut self,
        operation: &ScenarioOperation,
        context: &CandidateContext,
    ) -> ConformanceResult<CandidateRecord> {
        let mut record = self.inner.candidate(operation, context)?;
        if self.breakage == Breakage::BudgetMiscount {
            for cost in &mut record.declared_costs {
                cost.amount = 1;
            }
        }
        Ok(record)
    }

    fn guidance(
        &mut self,
        preference: &ScenarioPreference,
        context: &GuidanceContext,
    ) -> ConformanceResult<GuidanceRecord> {
        self.inner.guidance(preference, context)
    }

    fn revision(
        &mut self,
        preference: &ScenarioPreference,
        context: &RevisionContext,
    ) -> ConformanceResult<GuidanceRevisionRequest> {
        self.inner.revision(preference, context)
    }

    fn entropy_ticket(
        &mut self,
        draw: &ScenarioDraw,
        context: &EntropyContext,
    ) -> ConformanceResult<EntropyTicket> {
        self.inner.entropy_ticket(draw, context)
    }

    fn result(
        &mut self,
        outcome: &ScenarioOutcome,
        context: &ResultContext<'_>,
    ) -> ConformanceResult<ResultObservation> {
        let mut observation = self.inner.result(outcome, context)?;
        let other = context
            .state
            .outstanding()
            .iter()
            .find(|outstanding| outstanding.selection_id != context.selection.selection_id);
        if let (Breakage::CrossedIdentities, Some(other)) = (self.breakage, other) {
            observation.selection_id = other.selection_id;
            observation.parent_moment_id = other.parent_moment_id;
            observation.operation_id = other.operation_id;
        }
        Ok(observation)
    }

    fn closure(&mut self, context: &ClosureContext<'_>) -> ConformanceResult<UnobservedClosure> {
        self.inner.closure(context)
    }
}

impl ConsumerShell for BrokenAdapter {
    fn load_history(
        &mut self,
        request: &ControlProjectionRequest<'_>,
        previous: Option<&ValidatedControlProjection>,
    ) -> AdapterResult<ValidatedControlProjection> {
        if self.breakage != Breakage::HistoryRewrite {
            return self.inner.load_history(request, previous);
        }
        match self.inner.load_history(request, previous) {
            Ok(loaded) => {
                self.journal = Some(loaded.clone());
                Ok(loaded)
            }
            Err(error) => self.journal.clone().ok_or(error),
        }
    }

    fn validate_cache(
        &mut self,
        request: &CacheValidationRequest<'_>,
        cache: &CacheProjection,
    ) -> AdapterResult<FrontierState> {
        if self.breakage != Breakage::StaleHistory {
            return self.inner.validate_cache(request, cache);
        }
        let cached = usize::try_from(cache.control_event_count)
            .map_err(|_| CampaignError::ArithmeticOverflow)?;
        let events: Vec<_> = request
            .payloads
            .iter()
            .take(cached)
            .map(|payload| payload.control_event.clone())
            .collect();
        Ok(campaign_core::reconstruct(&events)?)
    }

    fn execution_gate(&mut self, request: &ExecutionGateRequest<'_>) -> ExecutionAuthority {
        if self.breakage == Breakage::ExecutionBeforePublication {
            return ExecutionAuthority::Granted;
        }
        self.inner.execution_gate(request)
    }

    fn promotion_claims(
        &mut self,
        receipt: &CampaignReceipt,
        signals: &AuthoritySignals,
    ) -> ReceiptClaims {
        let mut claims = self.inner.promotion_claims(receipt, signals);
        if self.breakage == Breakage::ReceiptOverclaim {
            claims.operation_success = signals.has_zero_exit_status;
            claims.release_readiness = signals.is_top_ranked && signals.is_selected;
            claims.authorization = signals.has_check_reference;
        }
        claims
    }

    fn role_identities(
        &mut self,
        candidate: &CandidateRecord,
        work_item: &SelectionDecision,
        result: &ResultObservation,
    ) -> RoleCollapseFixture {
        let mut roles = self.inner.role_identities(candidate, work_item, result);
        if self.breakage == Breakage::RoleCollapse {
            roles.queued_work_id = candidate.candidate_id;
            roles.computed_result_id = candidate.candidate_id;
        }
        roles
    }
}

fn broken_failures(breakage: Breakage) -> Vec<(&'static str, FixtureVerdict)> {
    failures(&run_suite(
        &mut BrokenAdapter::new(breakage),
        &suite_profile(),
    ))
}

fn let_through(declared: &[(&'static str, &'static str)]) -> Vec<(&'static str, FixtureVerdict)> {
    declared
        .iter()
        .map(|(name, forbidden)| (*name, FixtureVerdict::NegativeFixturePassed { forbidden }))
        .collect()
}

// r[verify chaoscontrol.campaign_policy.publication_fence.invalid]
#[test]
fn execution_before_publication_fails_the_named_fence_fixtures() {
    assert_eq!(
        broken_failures(Breakage::ExecutionBeforePublication),
        let_through(&[
            (
                "fencing.unpublished_execution",
                "a planned but unpublished selection was executed",
            ),
            (
                "fencing.non_durable_execution",
                "a selection executed without durable publication",
            ),
            (
                "fencing.stale_branch",
                "stale branch evidence authorized execution",
            ),
            (
                "fencing.foreign_selection",
                "an unpublished selection executed under another selection's durable publication",
            ),
            (
                "fencing.stale_state_selection",
                "a stale-state selection executed under the winner's durable publication",
            ),
            (
                "fencing.gate_matrix_ranked",
                "a selection executed without a publication or evidence",
            ),
            (
                "fencing.gate_matrix_exploratory",
                "a selection executed without a publication or evidence",
            ),
        ])
    );
}

// r[verify chaoscontrol.campaign_policy.adapter.invalid]
#[test]
fn role_collapse_fails_the_collapsed_identity_fixture() {
    assert_eq!(
        broken_failures(Breakage::RoleCollapse),
        let_through(&[(
            "roles.collapsed_identity",
            "the adapter stored candidate, work item, and result as one record",
        )])
    );
}

// r[verify chaoscontrol.campaign_policy.history.invalid]
#[test]
fn stale_history_and_history_rewrite_fail_the_named_fixtures() {
    assert_eq!(
        broken_failures(Breakage::StaleHistory),
        let_through(&[
            (
                "caches.stale",
                "a cache of a stale head was used as current state",
            ),
            (
                "caches.moved_binding",
                "a cache bound to a superseded graph was used as current state",
            ),
            (
                "caches.validation_matrix",
                "a cache of a stale state was used as current state",
            ),
            // The state-coverage fixture first presents each held state's lineage
            // on a graph older than its projection's; an unvalidated cache uses it.
            (
                "history.state_coverage",
                "a cache of a held state's lineage on an older graph was used as current state",
            ),
        ])
    );
    assert_eq!(
        broken_failures(Breakage::HistoryRewrite),
        let_through(&[
            (
                "history.rewritten_payload",
                "a rewritten control payload was loaded as history",
            ),
            (
                "history.altered_graph",
                "history altered after binding was loaded as current",
            ),
            (
                "history.ambiguous_lineage",
                "an ambiguous control lineage was loaded as history",
            ),
            (
                "history.load_matrix",
                "a rewound control branch was loaded as current history",
            ),
            // Campaign refuses the state-coverage fixture's load of a held state's
            // lineage on an older graph as stale; a journal fallback loads it.
            (
                "history.state_coverage",
                "a held state's lineage on an older graph was loaded as current",
            ),
        ])
    );
}

// r[verify chaoscontrol.campaign_policy.observations.invalid]
#[test]
fn crossed_identities_and_budget_miscount_fail_the_named_fixtures() {
    // The out-of-order settlement's crossed result names the other selection but
    // keeps the executed selection's reserved children, so Campaign refuses it
    // with the typed reservation error: that negative fixture fails too, as does
    // the state-coverage fixture at its child-reporting fork heads.
    let mut crossed = let_through(&[(
        "results.crossed_selection",
        "a result settled a selection other than the executed one",
    )]);
    let refused = FixtureVerdict::NegativeFixtureFailed(ConformanceError::Campaign(
        CampaignError::InvalidChildReservation,
    ));
    crossed.push(("results.out_of_order_settlement", refused.clone()));
    crossed.push(("history.state_coverage", refused));
    assert_eq!(broken_failures(Breakage::CrossedIdentities), crossed);
    let mut expected = vec![
        (
            "budgets.reservation",
            FixtureVerdict::PositiveFixtureFailed(ConformanceError::Diverged {
                expected: "a selection reserves exactly its abstract cost",
            }),
        ),
        (
            "budgets.declared_dimensions",
            FixtureVerdict::PositiveFixtureFailed(ConformanceError::Diverged {
                expected:
                    "the adapter declares every abstract cost in its own dimension with its exact units",
            }),
        ),
    ];
    expected.extend(let_through(&[(
        "budgets.overdraw",
        "a selection overdrew the abstract budget",
    )]));
    assert_eq!(broken_failures(Breakage::BudgetMiscount), expected);
}

// r[verify chaoscontrol.campaign_policy.evidence.invalid]
#[test]
fn receipt_overclaim_fails_the_promotion_fixtures() {
    assert_eq!(
        broken_failures(Breakage::ReceiptOverclaim),
        let_through(&[
            // The state-coverage fixture presents every receipt of each covered
            // state's session, the admission's first.
            (
                "history.state_coverage",
                "a campaign admission event receipt carried a promotion claim",
            ),
            (
                "receipts.promotion_overclaim",
                "rank, selection, exit status, or a check reference became promotion authority",
            ),
            // Every claims fixture presents every receipt of its session, the
            // admission's first, so an overclaiming shell fails there first.
            (
                "receipts.non_selection_overclaim",
                "a campaign admission event receipt carried a promotion claim",
            ),
            (
                "receipts.planner_overclaim",
                "a campaign admission event receipt carried a promotion claim",
            ),
            (
                "receipts.recorded_stop_overclaim",
                "a campaign admission event receipt carried a promotion claim",
            ),
        ])
    );
}
