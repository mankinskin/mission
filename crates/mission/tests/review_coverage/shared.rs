use super::approval_fixture::{self, Fx, linked_with_input};
use super::*;

pub(super) const SOURCE: &[u8] = b"Implement search behavior";
const OUTCOME: &str = "Search returns matching records";
const ACCEPTANCE_COMMAND: &str = "cargo test review::terminal";

pub(super) fn route(input: &[u8], kind: WaypointWorkKind) -> Fx {
    let fx = linked_with_input(input);
    let lifecycle = MissionLifecycle::open(&fx.ws);
    let mut source = lifecycle.get_mission_source(fx.id).unwrap();
    let owner = &mut source.waypoints[0];
    owner.work_kind = Some(kind);
    owner.prompt = match kind {
        WaypointWorkKind::Implementation => "Implement and test the requested behavior.".into(),
        WaypointWorkKind::Documentation => "Write documentation only.".into(),
        WaypointWorkKind::Research => "Research the requested behavior.".into(),
    };
    owner.validation = vec![mission::ValidationGate {
        command: ACCEPTANCE_COMMAND.into(),
        cwd: None,
    }];
    lifecycle
        .publish_draft(fx.id, source, fx.plan().revision)
        .unwrap();
    fx.attach_execution_contracts();
    fx
}

pub(super) fn capture(fx: &Fx, prompt: String) {
    fx.session
        .ingest_intent_confirmation_prompt(
            approval_fixture::SESSION,
            &HostPromptIngress::trusted_local_hook(
                "UserPromptSubmit",
                prompt.into_bytes(),
                Some(Uuid::new_v4().to_string()),
            ),
        )
        .unwrap()
        .unwrap();
}

pub(super) fn confirm_goal(
    fx: &Fx,
    category: IntentOutcomeCategory,
    allow_mock: bool,
) -> mission::EffectiveIntentV4 {
    let source_path = fx
        .ws
        .join(".workflow-tools/mission/missions")
        .join(fx.id.to_string())
        .join("input/submitted-input.bin");
    let source = fs::read(source_path).unwrap();
    let requirement = BaselineRequirementV4 {
        requirement_id: "search".into(),
        source: IntentSourceSpan {
            source_sha256: intent_source_digest(&source),
            start: 0,
            end: source.len(),
            excerpt: String::from_utf8(source.clone()).unwrap(),
        },
        outcome: OUTCOME.into(),
        category: category.clone(),
        evidence_policy: match category {
            IntentOutcomeCategory::WorkingBehavior => {
                IntentEvidencePolicy::ExecutableBehavior { allow_mock }
            }
            IntentOutcomeCategory::Artifact => IntentEvidencePolicy::DeterministicArtifact,
            IntentOutcomeCategory::Decision => IntentEvidencePolicy::CapturedDecision,
            IntentOutcomeCategory::ResearchReview => IntentEvidencePolicy::ResearchRecord,
        },
    };
    let reference = IntentReceiptReference {
        session_id: approval_fixture::SESSION.into(),
        challenge_id: Uuid::new_v4(),
    };
    let revision = fx.plan().revision;
    let (_, prepared) = execute_intent(
        &fx.ws,
        fx.id,
        IntentOperation::PrepareBaseline {
            expected_revision: revision,
            requirements: vec![requirement.clone()],
            reference: reference.clone(),
        },
    )
    .unwrap();
    let IntentOutput::Challenge { prompt, .. } = prepared else {
        panic!("baseline preparation must return a requester challenge");
    };
    capture(fx, prompt);
    execute_intent(
        &fx.ws,
        fx.id,
        IntentOperation::ConfirmBaseline {
            expected_revision: revision,
            requirements: vec![requirement],
            reference,
        },
    )
    .unwrap();
    MissionStore::open(&fx.ws)
        .effective_intent(fx.id, &SessionIntentProvider::new(&fx.ws).unwrap())
        .unwrap()
        .unwrap()
}

pub(super) fn input(
    fx: &Fx,
    intent: &mission::EffectiveIntentV4,
    kind: ReviewKind,
    verdict: ReviewVerdict,
    reviewer: &str,
    cold_review_id: Option<String>,
) -> ReviewInput {
    let plan = fx.plan();
    let requirement = intent
        .requirements
        .first()
        .or_else(|| intent.baseline.requirements.first())
        .unwrap();
    let class = match &requirement.category {
        IntentOutcomeCategory::WorkingBehavior => PlannedEvidenceClassV4::ExecutableBehavior,
        IntentOutcomeCategory::Artifact => PlannedEvidenceClassV4::DeterministicArtifact,
        IntentOutcomeCategory::Decision => PlannedEvidenceClassV4::CapturedDecision,
        IntentOutcomeCategory::ResearchReview => PlannedEvidenceClassV4::ResearchRecord,
    };
    let goal_coverage_findings = vec![GoalCoverageFindingV4 {
        requirement_id: requirement.requirement_id.clone(),
        disposition: GoalCoverageDispositionV4::Fulfilled {
            source: requirement.source.clone(),
            observable_terminal_outcome: OUTCOME.into(),
            owner_waypoint_ids: vec!["a".into()],
            acceptance_checks: vec![PlannedAcceptanceCheckV4 {
                waypoint_id: "a".into(),
                command: ACCEPTANCE_COMMAND.into(),
                cwd: None,
                evidence_class: class,
            }],
        },
    }];
    ReviewInput {
        kind,
        verdict,
        reviewer: reviewer.into(),
        revision: plan.revision,
        plan_digest: plan.plan_digest.clone(),
        evidence_identity: plan.evidence_identity,
        cold_review_id: cold_review_id.clone(),
        goal_coverage_findings,
        terminal_outcome_finding: Some(TerminalOutcomeFindingV4 {
            source_sha256: intent.baseline.source_sha256.clone(),
            baseline_sha256: intent.baseline.baseline_sha256.clone(),
            amendment_chain_sha256: intent.chain_sha256.clone(),
            plan_revision: plan.revision,
            plan_digest: plan.plan_digest,
            cold_review_id,
            conclusion: TerminalOutcomeConclusionV4::ReachesRequesterTarget,
            observable_terminal_outcome: OUTCOME.into(),
        }),
        scope_findings: fx.scope_findings(),
    }
}

pub(super) fn record(
    fx: &Fx,
    input: ReviewInput,
    request_id: &str,
) -> mission::approval::ReviewRecord {
    fx.approval()
        .record_review_with_request_id(fx.id, input, request_id)
        .unwrap()
}
