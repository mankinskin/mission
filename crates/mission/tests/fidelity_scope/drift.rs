use super::*;
use mission::{
    ExecutionCheckKindV4, ExecutionEvidenceClassV4, ExecutionFailureDispositionV4,
    ExecutionToolActionV4, TypedDraft, WaypointExecutionContractV4, approval::PreparedChallenge,
};
use serde_json::Value;
use std::{fs, path::PathBuf};

type ContractMutation = fn(&mut TypedDraft);

fn contract(source: &mut TypedDraft) -> &mut WaypointExecutionContractV4 {
    source.waypoints[0].execution_contract.as_mut().unwrap()
}

fn add_mutating_tool_action(source: &mut TypedDraft) {
    contract(source)
        .tool_policy
        .actions
        .push(ExecutionToolActionV4::CliMutating);
}

fn reorder_permissions(source: &mut TypedDraft) {
    contract(source).tool_policy.permissions.reverse();
}

fn expand_read_roots(source: &mut TypedDraft) {
    contract(source)
        .tool_policy
        .read_roots
        .push("workflow-tools/mission/crates/mission".into());
}

fn narrow_write_roots(source: &mut TypedDraft) {
    contract(source).tool_policy.write_roots[0] = "workflow-tools/mission/crates/mission".into();
}

fn change_environment_profile(source: &mut TypedDraft) {
    contract(source).environment.profile = "alternate-rust-workspace".into();
}

fn change_precondition(source: &mut TypedDraft) {
    contract(source).preconditions[0].description = "A revised prerequisite.".into();
}

fn change_postcondition(source: &mut TypedDraft) {
    contract(source).postconditions[0].description = "A revised terminal condition.".into();
}

fn change_acceptance_check(source: &mut TypedDraft) {
    let check = contract(source)
        .checks
        .iter_mut()
        .find(|check| check.kind == ExecutionCheckKindV4::Acceptance)
        .unwrap();
    check.command = "cargo test -p mission".into();
}

fn change_regression_check(source: &mut TypedDraft) {
    let check = contract(source)
        .checks
        .iter_mut()
        .find(|check| check.kind == ExecutionCheckKindV4::Regression)
        .unwrap();
    check.command = "cargo test --test fidelity_scope".into();
}

fn change_evidence_class(source: &mut TypedDraft) {
    let check = contract(source)
        .checks
        .iter_mut()
        .find(|check| check.kind == ExecutionCheckKindV4::Preflight)
        .unwrap();
    check.evidence_class = ExecutionEvidenceClassV4::ExecutableBehavior;
}

fn change_fallback(source: &mut TypedDraft) {
    let policy = &mut contract(source).failure_policy;
    policy.exhaustion = ExecutionFailureDispositionV4::Escalate;
    policy.escalation_target = "Mission owner".into();
}

fn change_handover(source: &mut TypedDraft) {
    contract(source)
        .handover
        .as_mut()
        .unwrap()
        .producer
        .content_reference = "ce://default/artifact/updated-handover".into();
}

const CONTRACT_MUTATIONS: [(&str, bool, ContractMutation); 12] = [
    ("tool-action", false, add_mutating_tool_action),
    ("permissions", false, reorder_permissions),
    ("read-roots", false, expand_read_roots),
    ("write-roots", false, narrow_write_roots),
    ("environment-profile", false, change_environment_profile),
    ("preconditions", false, change_precondition),
    ("postconditions", false, change_postcondition),
    ("acceptance-check", false, change_acceptance_check),
    ("regression-check", false, change_regression_check),
    ("evidence-class", false, change_evidence_class),
    ("fallback", false, change_fallback),
    ("handover", true, change_handover),
];

fn fixture(with_handover: bool) -> approval_fixture::Fx {
    if with_handover {
        support::contract_route_with_handover()
    } else {
        contract_route(WaypointWorkKind::Implementation)
    }
}

fn reviewed(fx: &approval_fixture::Fx) -> mission::EffectiveIntentV4 {
    let intent = confirm_goal(fx, IntentOutcomeCategory::WorkingBehavior, false);
    let cold = record(
        fx,
        scoped_input(
            fx,
            &intent,
            ReviewKind::ColdWalkthrough,
            ReviewVerdict::Pass,
            "cold reviewer",
            None,
        ),
        "drift-cold",
    );
    record(
        fx,
        scoped_input(
            fx,
            &intent,
            ReviewKind::Formal,
            ReviewVerdict::Pass,
            "formal reviewer",
            Some(cold.review_id),
        ),
        "drift-formal",
    );
    intent
}

fn mutate_contract(fx: &approval_fixture::Fx, mutation: ContractMutation) {
    let lifecycle = MissionLifecycle::open(&fx.ws);
    let mut source = lifecycle.get_mission_source(fx.id).unwrap();
    mutation(&mut source);
    lifecycle
        .publish_draft(fx.id, source, fx.plan().revision)
        .unwrap();
}

fn challenge_with_decision(
    fx: &approval_fixture::Fx,
    challenge: &str,
) -> (PreparedChallenge, session_api::RequesterDecisionRef) {
    let prepared = fx
        .approval()
        .prepare_challenge(fx.id, approval_fixture::SESSION, challenge)
        .unwrap();
    assert!(
        host_hook::run(
            &fx.ws,
            approval_fixture::SESSION,
            serde_json::json!({
                "hook_event_name":"UserPromptSubmit",
                "prompt":prepared.approve_line,
                "message_id":format!("{challenge}-event"),
            }),
        )
        .status
        .success()
    );
    let reference = fx
        .session
        .find_requester_decision_for_challenge(approval_fixture::SESSION, challenge)
        .unwrap()
        .unwrap()
        .reference;
    (prepared, reference)
}

#[test]
fn semantic_contract_drift_blocks_challenge_consumption_and_entry() {
    for (name, needs_handover, mutation) in CONTRACT_MUTATIONS {
        let fx = fixture(needs_handover);
        reviewed(&fx);
        let (_, reference) = challenge_with_decision(&fx, &format!("consume-{name}"));
        mutate_contract(&fx, mutation);
        let error = fx
            .approval()
            .accept_decision(fx.id, &reference)
            .unwrap_err();
        assert_eq!(error.code(), "approval_stale", "{name}: {error}");
        assert!(fx.approval().grants(fx.id).unwrap().is_empty(), "{name}");
    }

    for (name, needs_handover, mutation) in CONTRACT_MUTATIONS {
        let fx = fixture(needs_handover);
        reviewed(&fx);
        let (_, reference) = challenge_with_decision(&fx, &format!("entry-{name}"));
        fx.approval().accept_decision(fx.id, &reference).unwrap();
        mutate_contract(&fx, mutation);
        let error = fx
            .approval()
            .check_entry(fx.id, mission::approval::EntryPath::Dispatch)
            .unwrap_err();
        assert_eq!(error.code(), "approval_stale", "{name}: {error}");
    }
}

fn journal_record(fx: &approval_fixture::Fx, sequence: u32) -> PathBuf {
    fx.ws
        .join(".workflow-tools/mission/missions")
        .join(fx.id.to_string())
        .join(format!("approvals/journal/{sequence:08}.json"))
}

fn mutate_journal(fx: &approval_fixture::Fx, sequence: u32, mutate: impl FnOnce(&mut Value)) {
    let path = journal_record(fx, sequence);
    let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    mutate(&mut record);
    fs::write(&path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
}

fn reviewed_one_waypoint() -> approval_fixture::Fx {
    let fx = contract_route(WaypointWorkKind::Implementation);
    reviewed(&fx);
    fx
}

#[test]
fn forged_scope_review_binding_is_rejected_before_challenge() {
    let fx = reviewed_one_waypoint();
    mutate_journal(&fx, 1, |record| {
        record["entry"]["scope_binding_sha256"] = serde_json::json!("f".repeat(64));
    });
    let error = fx
        .approval()
        .prepare_challenge(fx.id, approval_fixture::SESSION, "forged-review-scope")
        .unwrap_err();
    assert_eq!(error.code(), "approval_journal_corrupt");
}

#[test]
fn forged_fidelity_challenge_hashes_are_rejected_before_consumption() {
    for field in [
        "execution_contracts_sha256",
        "cold_scope_findings_sha256",
        "formal_scope_findings_sha256",
    ] {
        let fx = reviewed_one_waypoint();
        let (_, reference) = challenge_with_decision(&fx, &format!("forged-{field}"));
        mutate_journal(&fx, 3, |record| {
            record["entry"]["fidelity_binding"][field] = serde_json::json!("0".repeat(64));
        });
        let error = fx
            .approval()
            .accept_decision(fx.id, &reference)
            .unwrap_err();
        assert_eq!(error.code(), "approval_stale", "{field}: {error}");
        assert!(fx.approval().grants(fx.id).unwrap().is_empty(), "{field}");
    }
}

#[test]
fn forged_grant_fidelity_hashes_are_rejected_before_entry() {
    for field in [
        "execution_contracts_sha256",
        "cold_scope_findings_sha256",
        "formal_scope_findings_sha256",
    ] {
        let fx = reviewed_one_waypoint();
        let (_, reference) = challenge_with_decision(&fx, &format!("grant-{field}"));
        fx.approval().accept_decision(fx.id, &reference).unwrap();
        mutate_journal(&fx, 4, |record| {
            record["entry"]["fidelity_binding"][field] = serde_json::json!("0".repeat(64));
        });
        let error = fx
            .approval()
            .check_entry(fx.id, mission::approval::EntryPath::Dispatch)
            .unwrap_err();
        assert!(
            matches!(error.code(), "approval_journal_corrupt" | "approval_stale"),
            "{field}: {error}"
        );
    }
}
