use mission::{
    BaselineRequirementV4, EffectiveIntentV4, IntentEvidencePolicy, IntentOutcomeCategory,
    IntentReceiptReference, IntentSourceSpan, MissionLifecycle, MissionStore,
    approval::{
        GoalCoverageDispositionV4, GoalCoverageFindingV4, PlannedAcceptanceCheckV4,
        PlannedEvidenceClassV4, ReviewInput, TerminalOutcomeConclusionV4, TerminalOutcomeFindingV4,
    },
    intent::{IntentOperation, IntentOutput, SessionIntentProvider, execute_intent},
    intent_source_digest,
};
use session_api::{HostPromptIngress, SessionStoreConfig};
use std::{fs, path::Path};
use uuid::Uuid;

const OUTCOME: &str = "Publish the requested planning artifact";

pub fn confirm(workspace: &Path, id: Uuid, session_id: &str, session: &SessionStoreConfig) {
    let source = fs::read(
        workspace
            .join(".workflow-tools/mission/missions")
            .join(id.to_string())
            .join("input/submitted-input.bin"),
    )
    .unwrap();
    let requirements = vec![BaselineRequirementV4 {
        requirement_id: "planning-artifact".into(),
        source: IntentSourceSpan {
            source_sha256: intent_source_digest(&source),
            start: 0,
            end: source.len(),
            excerpt: String::from_utf8(source).unwrap(),
        },
        outcome: OUTCOME.into(),
        category: IntentOutcomeCategory::Artifact,
        evidence_policy: IntentEvidencePolicy::DeterministicArtifact,
    }];
    let reference = IntentReceiptReference {
        session_id: session_id.into(),
        challenge_id: Uuid::new_v4(),
    };
    let revision = MissionLifecycle::open(workspace)
        .review_plan(id)
        .unwrap()
        .revision;
    let (_, output) = execute_intent(
        workspace,
        id,
        IntentOperation::PrepareBaseline {
            expected_revision: revision,
            requirements: requirements.clone(),
            reference: reference.clone(),
        },
    )
    .unwrap();
    let IntentOutput::Challenge { prompt, .. } = output else {
        panic!("intent preparation must return a challenge");
    };
    session
        .ingest_intent_confirmation_prompt(
            session_id,
            &HostPromptIngress::trusted_local_hook(
                "UserPromptSubmit",
                prompt.into_bytes(),
                Some(Uuid::new_v4().to_string()),
            ),
        )
        .unwrap()
        .unwrap();
    execute_intent(
        workspace,
        id,
        IntentOperation::ConfirmBaseline {
            expected_revision: revision,
            requirements,
            reference,
        },
    )
    .unwrap();
}

pub fn enrich(workspace: &Path, id: Uuid, input: &mut ReviewInput) {
    let intent = MissionStore::open(workspace)
        .effective_intent(id, &SessionIntentProvider::new(workspace).unwrap())
        .unwrap();
    let Some(intent) = intent else { return };
    apply(workspace, id, &intent, input);
}

fn apply(workspace: &Path, id: Uuid, intent: &EffectiveIntentV4, input: &mut ReviewInput) {
    let plan = MissionLifecycle::open(workspace).review_plan(id).unwrap();
    let owner = plan.waypoints.first().unwrap();
    input.goal_coverage_findings = intent
        .requirements
        .iter()
        .map(|requirement| GoalCoverageFindingV4 {
            requirement_id: requirement.requirement_id.clone(),
            disposition: GoalCoverageDispositionV4::Fulfilled {
                source: requirement.source.clone(),
                observable_terminal_outcome: OUTCOME.into(),
                owner_waypoint_ids: vec![owner.waypoint_id.clone()],
                acceptance_checks: owner
                    .validation
                    .iter()
                    .map(|gate| PlannedAcceptanceCheckV4 {
                        waypoint_id: owner.waypoint_id.clone(),
                        command: gate.command.clone(),
                        cwd: gate.cwd.clone(),
                        evidence_class: PlannedEvidenceClassV4::DeterministicArtifact,
                    })
                    .collect(),
            },
        })
        .collect();
    input.terminal_outcome_finding = Some(TerminalOutcomeFindingV4 {
        source_sha256: intent.baseline.source_sha256.clone(),
        baseline_sha256: intent.baseline.baseline_sha256.clone(),
        amendment_chain_sha256: intent.chain_sha256.clone(),
        plan_revision: input.revision,
        plan_digest: input.plan_digest.clone(),
        cold_review_id: input.cold_review_id.clone(),
        conclusion: TerminalOutcomeConclusionV4::ReachesRequesterTarget,
        observable_terminal_outcome: OUTCOME.into(),
    });
}
