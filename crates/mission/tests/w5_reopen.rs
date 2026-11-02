#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[path = "support/host_hook.rs"]
mod host_hook;

use approval_fixture::{Fx, SESSION, linked};
use mission::approval::EntryPath;
use mission::lifecycle::ReopenStep;
use mission::*;
use ticket_api::storage::store::TicketStore;
use uuid::Uuid;

fn request(f: &Fx, reason: &str) -> ControlRequest {
    ControlRequest {
        op_id: Uuid::new_v4(),
        expected_control_sequence: WorkspaceLifecycle::open(&f.ws)
            .read_lifecycle(&WaypointIdentity::Mission { mission_id: f.id })
            .unwrap()
            .control_sequence,
        evidence: Some(reason.into()),
    }
}
fn state(f: &Fx) -> MissionControlState {
    MissionLifecycle::open(&f.ws)
        .begin_approval_transaction(f.id)
        .unwrap()
        .control_state()
        .unwrap()
}
fn evidence(f: &Fx, waypoint: &str) -> String {
    let pointer = format!("evidence/{waypoint}.json");
    std::fs::create_dir_all(f.ws.join("evidence")).unwrap();
    std::fs::write(f.ws.join(&pointer), "{\"result\":\"passed\"}").unwrap();
    MissionLifecycle::open(&f.ws)
        .apply_plan_operation(
            f.id,
            PlanOperationRequest {
                request_id: format!("evidence-{waypoint}"),
                expected_revision: f.plan().revision,
                operation: PlanOperation::RecordEvidence {
                    plan_digest: f.plan().plan_digest,
                    result: OperatorResult {
                        waypoint_id: waypoint.into(),
                        outcome: "passed".into(),
                        operator_command: "cargo test".into(),
                        evidence_pointer: pointer.clone(),
                    },
                },
            },
        )
        .unwrap();
    pointer
}
fn completed() -> Fx {
    let f = linked();
    f.approve("initial", "initial-event");
    let adapter = WorkspaceLifecycle::open(&f.ws);
    adapter
        .activate(f.id, &request(&f, "approved start"))
        .unwrap();
    let store = TicketStore::open(&f.ws).unwrap();
    store.close(&f.ticket, "done", Some("w5-test")).unwrap();
    drop(store);
    let pointer = evidence(&f, "a");
    adapter
        .complete_waypoint(f.id, "a", &request(&f, &pointer))
        .unwrap();
    f
}

#[test]
fn mandatory_linked_done_to_ready_reopen_preserves_history_and_invalidates_approval() {
    let f = completed();
    let before = TicketStore::open(&f.ws)
        .unwrap()
        .get_history(&f.ticket)
        .unwrap();
    let immutable = state(&f).completions[0].clone();
    let adapter = WorkspaceLifecycle::open(&f.ws);
    let request = request(&f, "requester found missing behavior");
    let snapshot = adapter
        .reopen_most_recent_completed(f.id, f.plan().revision, 1, &request, None)
        .unwrap();
    assert_eq!(snapshot.state, LifecycleState::Planned);
    let store = TicketStore::open(&f.ws).unwrap();
    assert_eq!(store.get(&f.ticket).unwrap().extra["state"], "ready");
    let after = store.get_history(&f.ticket).unwrap();
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(after[0].fields, before[0].fields);
    assert!(after.last().unwrap().fields.contains_key("__reopen__"));
    let current = state(&f);
    assert_eq!(current.completions[0].result_ref, immutable.result_ref);
    assert!(!current.completions[0].current);
    assert!(current.stale.contains("b"));
    assert_eq!(f.code(EntryPath::Activation), "approval_stale");
    assert_eq!(
        adapter
            .reopen_most_recent_completed(f.id, f.plan().revision, 1, &request, None)
            .unwrap_err()
            .code(),
        "stale_control_sequence"
    );
    drop(store);
    f.approve("after-reopen", "after-reopen-event");
    assert!(
        f.approval()
            .check_entry(f.id, EntryPath::Activation)
            .is_ok()
    );
}

#[test]
fn all_cross_store_failure_windows_gate_execution_and_resume_once_after_restart() {
    for window in [
        ReopenStep::BeforeTicketTransition,
        ReopenStep::AfterTicketTransition,
        ReopenStep::BeforeMissionFinalization,
    ] {
        let f = completed();
        let request = request(&f, "controlled correction");
        let revision = f.plan().revision;
        let before = TicketStore::open(&f.ws)
            .unwrap()
            .get_history(&f.ticket)
            .unwrap()
            .len();
        let adapter = WorkspaceLifecycle::open(&f.ws);
        assert_eq!(
            adapter
                .reopen_most_recent_completed(f.id, revision, 1, &request, Some(window))
                .unwrap_err()
                .code(),
            "cross_store_pending"
        );
        assert_eq!(f.code(EntryPath::Dispatch), "cross_store_pending");
        let accepted_before = MissionStore::open(&f.ws).get(f.id).unwrap();
        let mut replacement = accepted_before.bundle.clone();
        replacement.manifest.revision += 1;
        let store = MissionStore::open(&f.ws);
        assert!(
            store.publish(f.id, replacement.clone(), revision).is_err(),
            "publication must not strand the pending reopen"
        );
        assert!(
            store
                .publish_with_status_snapshot(
                    f.id,
                    replacement,
                    revision,
                    &Default::default(),
                    "pending-reopen",
                )
                .is_err()
        );
        assert_eq!(store.get(f.id).unwrap(), accepted_before);
        assert!(
            !adapter
                .read_lifecycle(&WaypointIdentity::Waypoint {
                    mission_id: f.id,
                    waypoint_id: "b".into(),
                })
                .unwrap()
                .eligibility
                .is_eligible()
        );
        let native = TicketStore::open(&f.ws)
            .unwrap()
            .get(&f.ticket)
            .unwrap()
            .extra["state"]
            .clone();
        assert_eq!(
            native,
            if window == ReopenStep::BeforeTicketTransition {
                "done"
            } else {
                "ready"
            }
        );
        WorkspaceLifecycle::open(&f.ws)
            .reopen_most_recent_completed(f.id, revision, 1, &request, None)
            .unwrap();
        assert!(state(&f).pending_reopen.is_none());
        assert_eq!(
            TicketStore::open(&f.ws)
                .unwrap()
                .get_history(&f.ticket)
                .unwrap()
                .len(),
            before + 1
        );
    }
}

#[test]
fn completion_cas_is_serialized_and_reopen_uses_sequence_not_final_route_position() {
    let f = completed();
    let pointer = evidence(&f, "b");
    let request_b = request(&f, &pointer);
    let adapter = WorkspaceLifecycle::open(&f.ws);
    adapter.complete_waypoint(f.id, "b", &request_b).unwrap();
    assert_eq!(
        adapter
            .complete_waypoint(f.id, "b", &request_b)
            .unwrap_err()
            .code(),
        "stale_control_sequence"
    );
    let reopen_b = request(&f, "review b again");
    adapter
        .reopen_most_recent_completed(f.id, f.plan().revision, 2, &reopen_b, None)
        .unwrap();
    let reopen_a = request(&f, "review a too");
    adapter
        .reopen_most_recent_completed(f.id, f.plan().revision, 1, &reopen_a, None)
        .unwrap();
    assert_eq!(
        state(&f).latest_completion().unwrap_err().code(),
        "no_completed_waypoint"
    );
    f.approve("repeat", "repeat-event");
    TicketStore::open(&f.ws)
        .unwrap()
        .close(&f.ticket, "done", Some("w5-test"))
        .unwrap();
    adapter
        .complete_waypoint(f.id, "a", &request(&f, "evidence/a.json"))
        .unwrap();
    assert_eq!(state(&f).latest_completion().unwrap().sequence, 3);
    let snapshot = adapter
        .reopen_most_recent_completed(
            f.id,
            f.plan().revision,
            3,
            &request(&f, "latest is earlier in route"),
            None,
        )
        .unwrap();
    assert!(
        matches!(snapshot.identity, WaypointIdentity::Waypoint { waypoint_id, .. } if waypoint_id == "a")
    );
    assert_eq!(state(&f).completions.len(), 3);
}

#[test]
fn cancelled_or_changed_ticket_rejects_reopen_without_finalizing_mission() {
    let f = completed();
    let adapter = WorkspaceLifecycle::open(&f.ws);
    let request = request(&f, "reopen");
    adapter
        .reopen_most_recent_completed(
            f.id,
            f.plan().revision,
            1,
            &request,
            Some(ReopenStep::BeforeTicketTransition),
        )
        .unwrap_err();
    let store = TicketStore::open(&f.ws).unwrap();
    // A competing audited domain reopen invalidates the recorded revision.
    let revision = store.get_history(&f.ticket).unwrap().last().unwrap().rev;
    store
        .reopen_done(
            &f.ticket,
            revision,
            "competing correction",
            Uuid::new_v4(),
            None,
        )
        .unwrap();
    assert_eq!(
        adapter
            .reopen_most_recent_completed(f.id, f.plan().revision, 1, &request, None)
            .unwrap_err()
            .code(),
        "ticket_transition_rejected"
    );
    assert!(state(&f).pending_reopen.is_some());
    assert!(state(&f).completions[0].current);
}

#[test]
fn pause_cancel_do_not_cascade_ticket_and_resume_requires_current_approval() {
    let f = linked();
    let adapter = WorkspaceLifecycle::open(&f.ws);
    assert_eq!(
        adapter
            .activate(f.id, &request(&f, "start"))
            .unwrap_err()
            .code(),
        "approval_required"
    );
    f.approve("control", "control-event");
    adapter.activate(f.id, &request(&f, "start")).unwrap();
    let identity = WaypointIdentity::Mission { mission_id: f.id };
    adapter.pause(&identity, &request(&f, "pause")).unwrap();
    assert_eq!(
        TicketStore::open(&f.ws)
            .unwrap()
            .get(&f.ticket)
            .unwrap()
            .extra["state"],
        "ready"
    );
    adapter.resume(&identity, &request(&f, "resume")).unwrap();
    adapter
        .block(f.id, &request(&f, "blocked by external input"))
        .unwrap();
    assert_eq!(
        adapter
            .resume(&identity, &request(&f, "input arrived"))
            .unwrap()
            .state,
        LifecycleState::Active
    );
    adapter
        .cancel(&identity, &request(&f, "requester cancelled"))
        .unwrap();
    assert_eq!(
        adapter.read_lifecycle(&identity).unwrap().state,
        LifecycleState::Cancelled
    );
    assert_eq!(
        TicketStore::open(&f.ws)
            .unwrap()
            .get(&f.ticket)
            .unwrap()
            .extra["state"],
        "ready"
    );
}

#[test]
fn explicit_result_review_and_completion_require_all_current_waypoint_evidence() {
    let f = completed();
    let adapter = WorkspaceLifecycle::open(&f.ws);
    let identity = WaypointIdentity::Mission { mission_id: f.id };
    assert_eq!(
        adapter
            .complete(&identity, &request(&f, "done"))
            .unwrap_err()
            .code(),
        "result_review_required"
    );
    let pointer = evidence(&f, "b");
    adapter
        .complete_waypoint(f.id, "b", &request(&f, &pointer))
        .unwrap();
    adapter
        .review_result(&identity, &request(&f, "operator reviewed every outcome"))
        .unwrap();
    assert_eq!(
        adapter
            .complete(&identity, &request(&f, "results accepted"))
            .unwrap()
            .state,
        LifecycleState::Completed
    );
    adapter
        .reopen_most_recent_completed(
            f.id,
            f.plan().revision,
            2,
            &request(&f, "further work"),
            None,
        )
        .unwrap();
    assert_ne!(
        adapter.read_lifecycle(&identity).unwrap().state,
        LifecycleState::Completed
    );
    f.approve("recomplete", "recomplete-event");
    adapter
        .complete_waypoint(f.id, "b", &request(&f, "evidence/b.json"))
        .unwrap();
    assert_ne!(
        adapter.read_lifecycle(&identity).unwrap().state,
        LifecycleState::Completed
    );
    adapter
        .review_result(&identity, &request(&f, "new result review"))
        .unwrap();
    assert_eq!(
        adapter
            .complete(&identity, &request(&f, "new explicit completion"))
            .unwrap()
            .state,
        LifecycleState::Completed
    );
}

#[test]
fn pending_reopen_prevents_route_replacement_and_descendant_entry_needs_evidence() {
    let f = completed();
    let adapter = WorkspaceLifecycle::open(&f.ws);
    let req = request(&f, "reopen");
    let revision = f.plan().revision;
    adapter
        .reopen_most_recent_completed(
            f.id,
            revision,
            1,
            &req,
            Some(ReopenStep::BeforeTicketTransition),
        )
        .unwrap_err();
    let error = MissionLifecycle::open(&f.ws)
        .apply_plan_operation(
            f.id,
            PlanOperationRequest {
                request_id: "illegal-edit-during-pending".into(),
                expected_revision: revision,
                operation: PlanOperation::SetGoal {
                    title: "replacement".into(),
                    objective: "new goal".into(),
                },
            },
        )
        .unwrap_err();
    assert_eq!(error.diagnostics().unwrap()[0].code, "cross_store_pending");
    adapter
        .reopen_most_recent_completed(f.id, revision, 1, &req, None)
        .unwrap();
    f.approve("fresh", "fresh-event");
    let guard = f.approval();
    let guard = guard.guard_entry(f.id, EntryPath::Dispatch).unwrap();
    assert_eq!(
        guard
            .record_progress("b", WaypointState::InProgress, revision)
            .unwrap_err()
            .code(),
        "prerequisite_evidence_required"
    );
}

#[path = "w5_reopen/concurrency_and_add.rs"]
mod concurrency_and_add;
#[path = "w5_reopen/handoff.rs"]
mod handoff;
