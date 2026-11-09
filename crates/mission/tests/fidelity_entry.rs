//! Disposable captured host events establish guard semantics, not human approval.
#[allow(dead_code)]
#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[allow(dead_code, unused_imports)]
#[path = "support/direct_transports.rs"]
mod direct_transports;
#[allow(dead_code)]
#[path = "support/host_hook.rs"]
mod host_hook;

use approval_fixture::Fx;
use direct_transports::{Mcp, cli, json};
use mission::{
    BaselineRequirementV4, IntentChangeV4, IntentEvidencePolicy, IntentOutcomeCategory,
    IntentReceiptReference, IntentSourceSpan, MissionLifecycle, MissionStore, WaypointWorkKind,
    approval::{
        EntryPath, GoalCoverageDispositionV4, GoalCoverageFindingV4, PlannedAcceptanceCheckV4,
        PlannedEvidenceClassV4, ReviewInput, ReviewKind, ReviewVerdict,
        TerminalOutcomeConclusionV4, TerminalOutcomeFindingV4,
    },
    intent::{IntentOperation, IntentOutput, SessionIntentProvider, execute_intent},
    intent_source_digest,
};
use session_api::HostPromptIngress;
use std::fs;
use uuid::Uuid;

#[path = "review_coverage/shared.rs"]
mod shared;
use shared::*;

fn reviewed_route() -> Fx {
    let fx = route(b"Write search design", WaypointWorkKind::Documentation);
    let intent = confirm_goal(&fx, IntentOutcomeCategory::Artifact, false);
    let cold = record(
        &fx,
        input(
            &fx,
            &intent,
            ReviewKind::ColdWalkthrough,
            ReviewVerdict::Pass,
            "cold",
            None,
        ),
        "cold",
    );
    record(
        &fx,
        input(
            &fx,
            &intent,
            ReviewKind::Formal,
            ReviewVerdict::Pass,
            "formal",
            Some(cold.review_id),
        ),
        "formal",
    );
    fx
}

fn capture_approval(fx: &Fx, challenge: &str) -> session_api::RequesterDecisionRef {
    let prepared = fx
        .approval()
        .prepare_challenge(fx.id, approval_fixture::SESSION, challenge)
        .unwrap();
    assert!(
        host_hook::run(
            &fx.ws,
            approval_fixture::SESSION,
            json!({
                "hook_event_name":"UserPromptSubmit", "prompt": prepared.approve_line,
                "message_id":Uuid::new_v4().to_string(),
            })
        )
        .status
        .success()
    );
    fx.session
        .find_requester_decision_for_challenge(approval_fixture::SESSION, challenge)
        .unwrap()
        .unwrap()
        .reference
}

#[test]
fn legacy_two_pass_reviews_cannot_offer_approval_without_confirmed_intent() {
    let fx = approval_fixture::linked_with_input(b"plan me");
    fx.reviewed();
    let error = fx
        .approval()
        .prepare_challenge(
            fx.id,
            approval_fixture::SESSION,
            &Uuid::new_v4().to_string(),
        )
        .unwrap_err();
    assert_eq!(error.code(), "approval_policy_upgrade_required");
}

#[test]
fn confirmed_design_only_approves_and_all_entry_paths_pass() {
    let fx = reviewed_route();
    let reference = capture_approval(&fx, &Uuid::new_v4().to_string());
    fx.approval().accept_decision(fx.id, &reference).unwrap();
    for path in [
        EntryPath::Activation,
        EntryPath::Dispatch,
        EntryPath::Resume,
    ] {
        fx.approval().check_entry(fx.id, path).unwrap();
    }
}

#[test]
fn baseline_added_after_legacy_reviews_requires_fresh_coverage_before_challenge() {
    let fx = route(b"Write search design", WaypointWorkKind::Documentation);
    fx.reviewed();
    confirm_goal(&fx, IntentOutcomeCategory::Artifact, false);
    assert!(
        fx.approval()
            .prepare_challenge(
                fx.id,
                approval_fixture::SESSION,
                &Uuid::new_v4().to_string()
            )
            .is_err()
    );
}

fn amend(fx: &Fx) {
    let intent = MissionStore::open(&fx.ws)
        .effective_intent(fx.id, &SessionIntentProvider::new(&fx.ws).unwrap())
        .unwrap()
        .unwrap();
    let reference = IntentReceiptReference {
        session_id: approval_fixture::SESSION.into(),
        challenge_id: Uuid::new_v4(),
    };
    let operation = |prepare| {
        let revision = fx.plan().revision;
        let changes = vec![IntentChangeV4::Exclude {
            prior_requirement_id: "search".into(),
        }];
        if prepare {
            IntentOperation::PrepareAmendment {
                expected_revision: revision,
                amendment_id: "exclude-search".into(),
                prior_chain_sha256: intent.chain_sha256.clone(),
                source: "Exclude search".into(),
                changes,
                reference: reference.clone(),
            }
        } else {
            IntentOperation::AppendAmendment {
                expected_revision: revision,
                amendment_id: "exclude-search".into(),
                prior_chain_sha256: intent.chain_sha256.clone(),
                source: "Exclude search".into(),
                changes,
                reference: reference.clone(),
            }
        }
    };
    let (_, prepared) = execute_intent(&fx.ws, fx.id, operation(true)).unwrap();
    let IntentOutput::Challenge { prompt, .. } = prepared else {
        panic!("challenge required")
    };
    capture(fx, prompt);
    execute_intent(&fx.ws, fx.id, operation(false)).unwrap();
}

#[test]
fn amendments_after_reviews_block_challenge_and_after_capture_block_consumption() {
    let fx = reviewed_route();
    amend(&fx);
    assert!(
        fx.approval()
            .prepare_challenge(
                fx.id,
                approval_fixture::SESSION,
                &Uuid::new_v4().to_string()
            )
            .is_err()
    );
    let fx = reviewed_route();
    let reference = capture_approval(&fx, &Uuid::new_v4().to_string());
    amend(&fx);
    assert!(fx.approval().accept_decision(fx.id, &reference).is_err());
    assert!(fx.approval().grants(fx.id).unwrap().is_empty());
}

#[test]
fn amended_intent_revokes_all_entry_paths_without_deleting_grant_history() {
    let fx = reviewed_route();
    let reference = capture_approval(&fx, &Uuid::new_v4().to_string());
    fx.approval().accept_decision(fx.id, &reference).unwrap();
    let grants = fx.approval().grants(fx.id).unwrap();
    amend(&fx);
    for path in [
        EntryPath::Activation,
        EntryPath::Dispatch,
        EntryPath::Resume,
    ] {
        assert!(fx.approval().check_entry(fx.id, path).is_err());
    }
    assert_eq!(fx.approval().grants(fx.id).unwrap(), grants);
}

#[test]
fn legal_evidence_progress_preserves_fidelity_but_objective_edit_does_not() {
    let fx = reviewed_route();
    let reference = capture_approval(&fx, &Uuid::new_v4().to_string());
    fx.approval().accept_decision(fx.id, &reference).unwrap();
    let plan = fx.plan();
    MissionLifecycle::open(&fx.ws)
        .apply_plan_operation(
            fx.id,
            mission::PlanOperationRequest {
                request_id: "evidence".into(),
                expected_revision: plan.revision,
                operation: mission::PlanOperation::RecordEvidence {
                    plan_digest: plan.plan_digest,
                    result: mission::OperatorResult {
                        waypoint_id: "a".into(),
                        outcome: "passed".into(),
                        operator_command: "cargo test".into(),
                        evidence_pointer: "ce://default/test/fidelity".into(),
                    },
                },
            },
        )
        .unwrap();
    fx.approval()
        .check_entry(fx.id, EntryPath::Activation)
        .unwrap();
    MissionLifecycle::open(&fx.ws)
        .apply_plan_operation(
            fx.id,
            mission::PlanOperationRequest {
                request_id: "narrow-goal".into(),
                expected_revision: fx.plan().revision,
                operation: mission::PlanOperation::SetGoal {
                    title: "Other".into(),
                    objective: "Unrequested goal".into(),
                },
            },
        )
        .unwrap();
    assert!(
        fx.approval()
            .check_entry(fx.id, EntryPath::Activation)
            .is_err()
    );
}

#[test]
fn legacy_grant_fields_remain_readable_but_never_supply_new_entry_authority() {
    let fx = reviewed_route();
    let reference = capture_approval(&fx, &Uuid::new_v4().to_string());
    fx.approval().accept_decision(fx.id, &reference).unwrap();
    let journal = fx
        .ws
        .join(".workflow-tools/mission/missions")
        .join(fx.id.to_string())
        .join("approvals/journal");
    for entry in fs::read_dir(journal).unwrap() {
        let path = entry.unwrap().path();
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if let Some(object) = record["entry"].as_object_mut() {
            object.remove("fidelity_binding");
        }
        fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
    }
    assert_eq!(fx.approval().grants(fx.id).unwrap().len(), 1);
    assert_eq!(
        fx.approval()
            .check_entry(fx.id, EntryPath::Activation)
            .unwrap_err()
            .code(),
        "approval_policy_upgrade_required"
    );
}
#[test]
fn falsely_narrowed_feature_never_offers_approval_after_both_pass_attempts() {
    let fx = route(SOURCE, WaypointWorkKind::Documentation);
    let intent = confirm_goal(&fx, IntentOutcomeCategory::WorkingBehavior, false);
    for kind in [ReviewKind::ColdWalkthrough, ReviewKind::Formal] {
        let review = input(
            &fx,
            &intent,
            kind,
            ReviewVerdict::Pass,
            "reviewer",
            (kind == ReviewKind::Formal).then(|| "review-00000001".into()),
        );
        assert!(fx.approval().record_review(fx.id, review).is_err());
    }
    assert!(
        fx.approval()
            .prepare_challenge(
                fx.id,
                approval_fixture::SESSION,
                &Uuid::new_v4().to_string()
            )
            .is_err()
    );
}

#[test]
fn amended_intent_blocks_cli_generic_and_named_activation() {
    let fx = reviewed_route();
    let reference = capture_approval(&fx, &Uuid::new_v4().to_string());
    fx.approval().accept_decision(fx.id, &reference).unwrap();
    amend(&fx);
    let revision = fx.plan().revision;
    let args = [
        "activate-waypoint".into(),
        fx.id.to_string(),
        "a".into(),
        "--request-id".into(),
        Uuid::new_v4().to_string(),
        "--expected-revision".into(),
        revision.to_string(),
    ];
    let output = cli(&fx.ws, &args);
    assert_eq!(output["code"], "approval_stale", "{output}");
    let mut mcp = Mcp::start(&fx.ws);
    mcp.initialize();
    let generic = mcp.request(2, "tools/call", json!({"name":"mission_operation","arguments":{
                "operation":"record_progress","mission_id":fx.id,"waypoint_id":"a","state":"in-progress","expected_current_revision":revision,
            }}));
    let text: serde_json::Value =
        serde_json::from_str(generic["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["diagnostics"][0]["code"], "approval_stale", "{text}");
    let named = mcp.request(3, "tools/call", json!({"name":"mission_waypoint_activate","arguments":{
                "workspace_root":fx.ws,"mission_id":fx.id,"waypoint_id":"a","request_id":Uuid::new_v4(),"expected_revision":revision,
            }}));
    let text: serde_json::Value =
        serde_json::from_str(named["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["code"], "approval_stale", "{text}");
}
