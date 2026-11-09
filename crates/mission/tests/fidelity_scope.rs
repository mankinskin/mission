#[allow(dead_code)]
#[path = "support/host_hook.rs"]
mod host_hook;
#[path = "fidelity_scope/support.rs"]
mod support;

use mission::{
    IntentOutcomeCategory, MissionLifecycle, ScopeCriterionVerdictV4, WaypointWorkKind,
    approval::{ReviewKind, ReviewVerdict},
};
use support::{approval_fixture, confirm_goal, contract_route, input, record, scoped_input};

#[path = "fidelity_scope/drift.rs"]
mod drift;

#[test]
fn current_execution_contract_cannot_be_approved_without_scope_audits() {
    let fx = contract_route(WaypointWorkKind::Implementation);
    let intent = confirm_goal(&fx, mission::IntentOutcomeCategory::WorkingBehavior, false);
    let mut review = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold reviewer",
        None,
    );
    review.scope_findings.clear();
    let error = fx
        .approval()
        .record_review_with_request_id(fx.id, review, "scope-cold")
        .unwrap_err();

    assert_eq!(error.code(), "approval_scope_audit_required");

    let cold = record(
        &fx,
        scoped_input(
            &fx,
            &intent,
            ReviewKind::ColdWalkthrough,
            ReviewVerdict::Pass,
            "cold reviewer",
            None,
        ),
        "scope-cold",
    );
    let mut formal = input(
        &fx,
        &intent,
        ReviewKind::Formal,
        ReviewVerdict::Pass,
        "formal reviewer",
        Some(cold.review_id),
    );
    formal.scope_findings.clear();
    let error = fx
        .approval()
        .record_review_with_request_id(fx.id, formal, "scope-formal")
        .unwrap_err();
    assert_eq!(error.code(), "approval_scope_audit_required");
}

#[test]
fn failed_or_incomplete_scope_findings_cannot_be_recorded_as_a_pass() {
    let fx = contract_route(WaypointWorkKind::Implementation);
    let intent = confirm_goal(&fx, IntentOutcomeCategory::WorkingBehavior, false);
    let mut input = scoped_input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold reviewer",
        None,
    );
    input.scope_findings[0].compactness.verdict = ScopeCriterionVerdictV4::Fail;
    let error = fx.approval().record_review(fx.id, input).unwrap_err();
    assert_eq!(error.code(), "approval_scope_not_manageable");
    assert!(error.message().contains("ticket-backed work packages"));

    let mut input = scoped_input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold reviewer",
        None,
    );
    input.scope_findings[0].outcome_count = 2;
    let error = fx.approval().record_review(fx.id, input).unwrap_err();
    assert_eq!(error.code(), "approval_scope_not_manageable");
    assert!(
        error
            .message()
            .contains("separately ticket-backed waypoints")
    );

    let mut input = scoped_input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold reviewer",
        None,
    );
    input.scope_findings[0].approachability.rationale.clear();
    let error = fx.approval().record_review(fx.id, input).unwrap_err();
    assert_eq!(error.code(), "approval_scope_audit_incomplete");

    let mut input = scoped_input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold reviewer",
        None,
    );
    input.scope_findings[0].context_ids.clear();
    let error = fx.approval().record_review(fx.id, input).unwrap_err();
    assert_eq!(error.code(), "approval_scope_evidence_invalid");

    let mut input = scoped_input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold reviewer",
        None,
    );
    input.scope_findings[0]
        .boundary_paths
        .push("outside".into());
    let error = fx.approval().record_review(fx.id, input).unwrap_err();
    assert_eq!(error.code(), "approval_scope_evidence_invalid");

    let mut input = scoped_input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold reviewer",
        None,
    );
    input.scope_findings[0].execution_contract_digest = "0".repeat(64);
    let error = fx.approval().record_review(fx.id, input).unwrap_err();
    assert_eq!(error.code(), "approval_stale");
}

#[test]
fn passing_scope_audits_bind_reviews_and_contracts_across_challenge_and_entry() {
    let fx = contract_route(WaypointWorkKind::Implementation);
    let intent = confirm_goal(&fx, IntentOutcomeCategory::WorkingBehavior, false);
    let cold = record(
        &fx,
        scoped_input(
            &fx,
            &intent,
            ReviewKind::ColdWalkthrough,
            ReviewVerdict::Pass,
            "cold reviewer",
            None,
        ),
        "scope-cold",
    );
    let formal = scoped_input(
        &fx,
        &intent,
        ReviewKind::Formal,
        ReviewVerdict::Pass,
        "formal reviewer",
        Some(cold.review_id.clone()),
    );
    let recorded_formal = fx
        .approval()
        .record_review_with_request_id(fx.id, formal.clone(), "scope-formal")
        .unwrap();
    let mut divergent = formal;
    divergent.scope_findings[0].rationale = "Changed rationale".into();
    assert_eq!(
        fx.approval()
            .record_review_with_request_id(fx.id, divergent, "scope-formal")
            .unwrap_err()
            .code(),
        "approval_conflict"
    );

    let prepared = fx
        .approval()
        .prepare_challenge(fx.id, approval_fixture::SESSION, "scope-approved")
        .unwrap();
    assert!(
        host_hook::run(
            &fx.ws,
            approval_fixture::SESSION,
            serde_json::json!({
                "hook_event_name":"UserPromptSubmit",
                "prompt":prepared.approve_line,
                "message_id":"scope-approve-event",
            }),
        )
        .status
        .success()
    );
    let reference = fx
        .session
        .find_requester_decision_for_challenge(approval_fixture::SESSION, "scope-approved")
        .unwrap()
        .unwrap()
        .reference;
    fx.approval().accept_decision(fx.id, &reference).unwrap();
    let grants = fx.approval().grants(fx.id).unwrap();
    assert_eq!(recorded_formal.scope_findings.len(), 1);
    assert_eq!(
        recorded_formal
            .scope_binding_sha256
            .as_deref()
            .map(str::len),
        Some(64)
    );
    for path in [
        mission::approval::EntryPath::Activation,
        mission::approval::EntryPath::Dispatch,
        mission::approval::EntryPath::Resume,
    ] {
        fx.approval().check_entry(fx.id, path).unwrap();
    }

    let lifecycle = MissionLifecycle::open(&fx.ws);
    let mut source = lifecycle.get_mission_source(fx.id).unwrap();
    source.waypoints[0]
        .execution_contract
        .as_mut()
        .unwrap()
        .rationale = "Semantic contract change after approval.".into();
    lifecycle
        .publish_draft(fx.id, source, fx.plan().revision)
        .unwrap();
    assert_eq!(fx.approval().grants(fx.id).unwrap(), grants);
    for path in [
        mission::approval::EntryPath::Activation,
        mission::approval::EntryPath::Dispatch,
        mission::approval::EntryPath::Resume,
    ] {
        assert!(fx.approval().check_entry(fx.id, path).is_err());
    }
}

#[test]
fn changed_contract_digest_after_challenge_blocks_approval_consumption() {
    let fx = contract_route(WaypointWorkKind::Implementation);
    let intent = confirm_goal(&fx, IntentOutcomeCategory::WorkingBehavior, false);
    let cold = record(
        &fx,
        scoped_input(
            &fx,
            &intent,
            ReviewKind::ColdWalkthrough,
            ReviewVerdict::Pass,
            "cold reviewer",
            None,
        ),
        "scope-cold",
    );
    record(
        &fx,
        scoped_input(
            &fx,
            &intent,
            ReviewKind::Formal,
            ReviewVerdict::Pass,
            "formal reviewer",
            Some(cold.review_id),
        ),
        "scope-formal",
    );
    let prepared = fx
        .approval()
        .prepare_challenge(fx.id, approval_fixture::SESSION, "scope-stale")
        .unwrap();
    assert!(
        host_hook::run(
            &fx.ws,
            approval_fixture::SESSION,
            serde_json::json!({
                "hook_event_name":"UserPromptSubmit",
                "prompt":prepared.approve_line,
                "message_id":"scope-stale-event",
            }),
        )
        .status
        .success()
    );
    let reference = fx
        .session
        .find_requester_decision_for_challenge(approval_fixture::SESSION, "scope-stale")
        .unwrap()
        .unwrap()
        .reference;

    let lifecycle = MissionLifecycle::open(&fx.ws);
    let mut source = lifecycle.get_mission_source(fx.id).unwrap();
    source.waypoints[0]
        .execution_contract
        .as_mut()
        .unwrap()
        .environment
        .profile = "changed-profile".into();
    lifecycle
        .publish_draft(fx.id, source, fx.plan().revision)
        .unwrap();

    assert!(fx.approval().accept_decision(fx.id, &reference).is_err());
    assert!(fx.approval().grants(fx.id).unwrap().is_empty());
}

#[test]
fn bounded_documentation_contract_can_pass_with_deterministic_artifact_evidence() {
    let fx = contract_route(WaypointWorkKind::Documentation);
    let intent = confirm_goal(&fx, IntentOutcomeCategory::Artifact, false);
    let cold = record(
        &fx,
        scoped_input(
            &fx,
            &intent,
            ReviewKind::ColdWalkthrough,
            ReviewVerdict::Pass,
            "cold reviewer",
            None,
        ),
        "documentation-cold",
    );
    record(
        &fx,
        scoped_input(
            &fx,
            &intent,
            ReviewKind::Formal,
            ReviewVerdict::Pass,
            "formal reviewer",
            Some(cold.review_id),
        ),
        "documentation-formal",
    );

    assert!(
        fx.approval()
            .prepare_challenge(fx.id, approval_fixture::SESSION, "documentation-scope",)
            .is_ok()
    );
}
