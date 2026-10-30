//! Fixture hook stdin validates plumbing, not human authorization.
#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[path = "support/host_hook.rs"]
mod host_hook;
use approval_fixture::{Fx, SESSION, linked};
use mission::approval::{ApprovalStep, EntryPath, ReviewKind, ReviewVerdict};
use mission::{
    LifecycleRead, LifecycleTransition, TransitionRequest, WaypointIdentity, WorkspaceLifecycle,
};
use session_api::{DecisionKind, RequesterChallengeRequest, RequesterDecisionRef};
use uuid::Uuid;

fn submit(f: &Fx, line: &str, event: &str) {
    assert!(
        host_hook::run(
            &f.ws,
            SESSION,
            serde_json::json!({
                "hook_event_name": "UserPromptSubmit", "prompt": line, "message_id": event,
            })
        )
        .status
        .success()
    );
}
fn decide(f: &Fx, challenge: &str, kind: DecisionKind, event: &str) -> RequesterDecisionRef {
    let prepared = f
        .approval()
        .prepare_challenge(f.id, SESSION, challenge)
        .unwrap();
    let line = match kind {
        DecisionKind::Approve => prepared.approve_line,
        DecisionKind::Reject => prepared.reject_line,
        DecisionKind::Replan => prepared.replan_line,
    };
    submit(f, &line, event);
    f.session
        .find_requester_decision_for_challenge(SESSION, challenge)
        .unwrap()
        .unwrap()
        .reference
}
fn activate(f: &Fx) -> Result<String, String> {
    WorkspaceLifecycle::open(&f.ws)
        .transition(
            &WaypointIdentity::Ticket {
                ticket_id: f.ticket,
            },
            &TransitionRequest {
                op_id: Uuid::new_v4(),
                to_native_state: "in-implementation".into(),
                author: Some("w4-test".into()),
            },
        )
        .map(|s| s.native_state)
        .map_err(|e| e.code().to_string())
}
fn ticket_state(f: &Fx) -> String {
    WorkspaceLifecycle::open(&f.ws)
        .read_lifecycle(&WaypointIdentity::Ticket {
            ticket_id: f.ticket,
        })
        .unwrap()
        .native_state
}
#[test]
fn interrupted_prepare_recovers_one_challenge() {
    let f = linked();
    f.reviewed();
    let error = f
        .approval()
        .prepare_challenge_with_fault(
            f.id,
            SESSION,
            "c1",
            Some(ApprovalStep::AfterChallengeJournal),
        )
        .unwrap_err();
    assert_eq!(error.code(), "approval_pending");
    assert_eq!(
        f.approval().pending(f.id).unwrap()[0].stage,
        "challenge-unregistered"
    );
    assert!(f.session.read_requester_challenge(SESSION, "c1").is_err());
    f.approval().prepare_challenge(f.id, SESSION, "c1").unwrap();
    assert!(f.approval().pending(f.id).unwrap().is_empty());
}
#[test]
fn interrupted_accept_never_enters_until_consumed() {
    let f = linked();
    f.reviewed();
    let reference = decide(&f, "c1", DecisionKind::Approve, "evt-1");
    assert_eq!(
        f.approval()
            .accept_decision_with_fault(f.id, &reference, Some(ApprovalStep::BeforeGrant))
            .unwrap_err()
            .code(),
        "approval_pending"
    );
    assert_eq!(f.code(EntryPath::Activation), "approval_required");
    assert_eq!(
        f.approval()
            .accept_decision_with_fault(f.id, &reference, Some(ApprovalStep::AfterGrant))
            .unwrap_err()
            .code(),
        "approval_pending"
    );
    assert_eq!(
        f.approval().pending(f.id).unwrap()[0].stage,
        "disposition-unconsumed"
    );
    assert_eq!(f.code(EntryPath::Resume), "approval_pending");
    assert_eq!(activate(&f), Err("approval_pending".into()));
    assert_eq!(ticket_state(&f), "ready");
    f.approval().accept_decision(f.id, &reference).unwrap();
    assert_eq!(f.approval().grants(f.id).unwrap().len(), 1);
    assert_eq!(activate(&f), Ok("in-implementation".into()));
}
#[test]
fn semantic_edit_then_revert_never_revives_grant() {
    let f = linked();
    f.approve("c1", "evt-1");
    let granted = f.plan();
    f.edit_b("semantic", "b changed");
    assert_eq!(f.code(EntryPath::Dispatch), "approval_stale");
    f.edit_b("revert", "b prompt");
    assert_eq!(f.plan().plan_digest, granted.plan_digest);
    for path in [
        EntryPath::Activation,
        EntryPath::Dispatch,
        EntryPath::Resume,
    ] {
        assert_eq!(f.code(path), "approval_stale");
    }
    assert_eq!(activate(&f), Err("approval_stale".into()));
}
#[test]
fn later_reject_replan_and_failed_review_invalidate_prior_grant() {
    for kind in [DecisionKind::Reject, DecisionKind::Replan] {
        let f = linked();
        f.approve("c1", "evt-1");
        f.approval()
            .check_entry(f.id, EntryPath::Activation)
            .unwrap();
        let reference = decide(&f, "c2", kind, "evt-2");
        assert!(
            f.approval()
                .accept_decision(f.id, &reference)
                .unwrap()
                .grant
                .is_none()
        );
        assert_eq!(f.code(EntryPath::Activation), "approval_required");
        assert_eq!(activate(&f), Err("approval_required".into()));
    }
    let f = linked();
    f.approve("c1", "evt-1");
    f.review(ReviewKind::Formal, ReviewVerdict::Fail, "formal")
        .unwrap();
    assert_eq!(f.code(EntryPath::Resume), "approval_required");
    assert_eq!(activate(&f), Err("approval_required".into()));
}
#[test]
fn foreign_root_unjournaled_challenge_is_never_accepted() {
    let f = linked();
    f.reviewed();
    let plan = f.plan();
    let challenge = f
        .session
        .register_requester_challenge(&RequesterChallengeRequest {
            session_id: SESSION.into(),
            challenge_id: "x1".into(),
            mission_id: f.id,
            mission_root: std::path::PathBuf::from("/elsewhere/ws"),
            revision: plan.revision,
            plan_digest: plan.plan_digest,
            review_snapshot_digest: "c".repeat(64),
        })
        .unwrap();
    submit(
        &f,
        &f.session
            .render_decision_prompt(&challenge, DecisionKind::Approve),
        "evt-x",
    );
    let reference = f
        .session
        .find_requester_decision_for_challenge(SESSION, "x1")
        .unwrap()
        .unwrap()
        .reference;
    assert_eq!(
        f.approval()
            .accept_decision(f.id, &reference)
            .unwrap_err()
            .code(),
        "approval_authority_unavailable"
    );
    assert!(f.approval().grants(f.id).unwrap().is_empty());
    assert_eq!(f.code(EntryPath::Activation), "approval_required");
}
