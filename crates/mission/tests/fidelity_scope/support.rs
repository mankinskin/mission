#[allow(dead_code)]
#[path = "../support/approval_fixture.rs"]
pub(super) mod approval_fixture;

use mission::{
    BaselineRequirementV4, IntentEvidencePolicy, IntentOutcomeCategory, IntentReceiptReference,
    IntentSourceSpan, MissionLifecycle, MissionStore, ScopeCriterionFindingV4,
    ScopeCriterionVerdictV4, TypedDraft, WaypointOutcomeCategoryV4, WaypointScopeFindingV4,
    WaypointWorkKind,
    approval::{
        GoalCoverageDispositionV4, GoalCoverageFindingV4, PlannedAcceptanceCheckV4,
        PlannedEvidenceClassV4, ReviewInput, ReviewKind, ReviewVerdict,
        TerminalOutcomeConclusionV4, TerminalOutcomeFindingV4,
    },
    intent::{IntentOperation, IntentOutput, SessionIntentProvider, execute_intent},
    intent_source_digest,
};
use session_api::HostPromptIngress;
use std::fs;
use uuid::Uuid;

#[path = "../review_coverage/shared.rs"]
pub(super) mod shared;

pub(super) fn confirm_goal(
    fx: &approval_fixture::Fx,
    category: IntentOutcomeCategory,
    allow_mock: bool,
) -> mission::EffectiveIntentV4 {
    shared::confirm_goal(fx, category, allow_mock)
}

pub(super) fn input(
    fx: &approval_fixture::Fx,
    intent: &mission::EffectiveIntentV4,
    kind: ReviewKind,
    verdict: ReviewVerdict,
    reviewer: &str,
    cold_review_id: Option<String>,
) -> ReviewInput {
    shared::input(fx, intent, kind, verdict, reviewer, cold_review_id)
}

pub(super) fn record(
    fx: &approval_fixture::Fx,
    input: ReviewInput,
    request_id: &str,
) -> mission::approval::ReviewRecord {
    shared::record(fx, input, request_id)
}

pub(super) fn contract_route(kind: WaypointWorkKind) -> approval_fixture::Fx {
    let fx = shared::route(shared::SOURCE, kind);
    let lifecycle = MissionLifecycle::open(&fx.ws);
    let mut source = lifecycle.get_mission_source(fx.id).unwrap();
    let contract_source: TypedDraft = serde_json::from_str(include_str!(
        "../../../mission-api/tests/fixtures/execution-contract-v4.json"
    ))
    .unwrap();
    source.schema_version = 4;
    source.waypoints.truncate(1);
    let mut contract = contract_source.waypoints[0]
        .execution_contract
        .clone()
        .unwrap();
    contract.work_kind = kind;
    source.waypoints[0].work_kind = Some(kind);
    if kind == WaypointWorkKind::Documentation {
        contract.outcome_category = WaypointOutcomeCategoryV4::Artifact;
        contract.expected_outcome = "A deterministic documentation artifact".into();
        for check in &mut contract.checks {
            if check.kind == mission::ExecutionCheckKindV4::Acceptance {
                check.evidence_class = mission::ExecutionEvidenceClassV4::DeterministicArtifact;
            }
        }
        for evidence in &mut contract.evidence_requirements {
            evidence.class = mission::ExecutionEvidenceClassV4::DeterministicArtifact;
        }
    }
    source.waypoints[0].execution_contract = Some(contract);
    lifecycle
        .publish_draft(fx.id, source, fx.plan().revision)
        .unwrap();
    fx
}

pub(super) fn contract_route_with_handover() -> approval_fixture::Fx {
    let fx = shared::route(shared::SOURCE, WaypointWorkKind::Implementation);
    let lifecycle = MissionLifecycle::open(&fx.ws);
    let mut source = lifecycle.get_mission_source(fx.id).unwrap();
    let producer = source.waypoints[0].execution_contract.as_mut().unwrap();
    producer.no_handover_reason = None;
    producer.handover = Some(mission::ExecutionHandoverV4 {
        id: "handover".into(),
        producer: mission::ExecutionHandoverEndpointV4 {
            waypoint_id: "a".into(),
            artifact_id: "artifact".into(),
            content_reference: "ce://default/artifact/handover".into(),
            schema: "v1".into(),
            root: "workflow-tools/mission".into(),
            check_ids: vec!["check-acceptance".into()],
        },
        consumer: mission::ExecutionHandoverEndpointV4 {
            waypoint_id: "b".into(),
            artifact_id: "artifact".into(),
            content_reference: "ce://default/artifact/handover".into(),
            schema: "v1".into(),
            root: "workflow-tools/mission".into(),
            check_ids: vec!["check-preflight".into()],
        },
    });
    source.waypoints[1].depends_on = vec!["a".into()];
    let consumer = source.waypoints[1].execution_contract.as_mut().unwrap();
    consumer.declared_dependency_ids = vec!["a".into()];
    consumer.no_dependencies_reason = None;
    lifecycle
        .publish_draft(fx.id, source, fx.plan().revision)
        .unwrap();
    fx
}

fn scope_finding(waypoint: &mission::ReviewWaypoint) -> WaypointScopeFindingV4 {
    let contract = waypoint.execution_contract.as_ref().unwrap();
    let criterion = || ScopeCriterionFindingV4 {
        verdict: ScopeCriterionVerdictV4::Pass,
        rationale: "The declared task and checks fit one agent's existing one-sitting threshold."
            .into(),
    };
    WaypointScopeFindingV4 {
        waypoint_id: waypoint.waypoint_id.clone(),
        execution_contract_digest: waypoint.execution_contract_digest.clone().unwrap(),
        outcome_count: 1,
        completeness: criterion(),
        compactness: criterion(),
        approachability: criterion(),
        single_coherent_outcome: criterion(),
        single_agent_one_sitting: criterion(),
        boundary_task: contract.boundary.task.clone(),
        boundary_paths: contract.boundary.paths.clone(),
        context_ids: contract
            .authoritative_context
            .iter()
            .map(|context| context.id.clone())
            .collect(),
        acceptance_check_ids: contract
            .checks
            .iter()
            .filter(|check| check.kind == mission::ExecutionCheckKindV4::Acceptance)
            .map(|check| check.id.clone())
            .collect(),
        rationale: "The single outcome is concrete and its boundary, context, and acceptance route are explicit."
            .into(),
    }
}

pub(super) fn scoped_input(
    fx: &approval_fixture::Fx,
    intent: &mission::EffectiveIntentV4,
    kind: ReviewKind,
    verdict: ReviewVerdict,
    reviewer: &str,
    cold_review_id: Option<String>,
) -> ReviewInput {
    let mut input = shared::input(fx, intent, kind, verdict, reviewer, cold_review_id);
    input.scope_findings = fx.plan().waypoints.iter().map(scope_finding).collect();
    input
}
