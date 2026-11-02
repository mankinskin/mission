use super::*;

#[test]
fn finished_handoff_destination_and_stale_resume_do_not_mutate_binding_or_controls() {
    let f = linked();
    f.approve("controls", "controls-event");
    const DEST: &str = "33333333-3333-4333-8333-333333333333";
    f.session
        .init_runtime_context(session_api::SessionRuntimeInitRequest {
            session_id: Some(DEST.into()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();
    f.session
        .finish_workflow(DEST, vec![], vec![], None)
        .unwrap();
    let adapter = WorkspaceLifecycle::open(&f.ws);
    assert!(
        adapter
            .handoff_session(f.id, SESSION, DEST, &request(&f, "inactive target"))
            .is_err()
    );
    assert!(state(&f).pending_handoff.is_none());
    let identity = WaypointIdentity::Mission { mission_id: f.id };
    adapter
        .pause(&identity, &request(&f, "paused for revision"))
        .unwrap();
    let before = adapter.read_lifecycle(&identity).unwrap().control_sequence;
    f.edit_b("edit-paused", "revised route");
    assert_eq!(
        adapter
            .resume(&identity, &request(&f, "unsafe resume"))
            .unwrap_err()
            .code(),
        "approval_stale"
    );
    let after = adapter.read_lifecycle(&identity).unwrap();
    assert_eq!(after.control_sequence, before);
    assert_eq!(after.state, LifecycleState::Paused);
}

#[test]
fn concurrent_completion_competitors_cannot_append_two_events() {
    let f = completed();
    let pointer = evidence(&f, "b");
    let request = request(&f, &pointer);
    let results = std::thread::scope(|scope| {
        let run = || WorkspaceLifecycle::open(&f.ws).complete_waypoint(f.id, "b", &request);
        let a = scope.spawn(run);
        let b = scope.spawn(run);
        vec![a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .filter(|e| e.code() == "stale_control_sequence")
            .count(),
        1
    );
    assert_eq!(state(&f).completions.len(), 2);
}

#[test]
fn concurrent_reopen_selects_one_current_completion_and_stale_competitor_fails() {
    let f = completed();
    let a = request(&f, "first correction");
    let b = ControlRequest {
        op_id: Uuid::new_v4(),
        ..a.clone()
    };
    let revision = f.plan().revision;
    let results = std::thread::scope(|scope| {
        let run = |req: &ControlRequest| {
            WorkspaceLifecycle::open(&f.ws)
                .reopen_most_recent_completed(f.id, revision, 1, req, None)
        };
        let first = scope.spawn(move || run(&a));
        let second = scope.spawn(move || run(&b));
        vec![first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .filter(|e| e.code() == "stale_control_sequence")
            .count(),
        1
    );
    assert_eq!(state(&f).completions.len(), 1);
}

#[test]
fn result_review_adds_work_only_through_w3_semantic_change_and_w4_reapproval() {
    let f = completed();
    let adapter = WorkspaceLifecycle::open(&f.ws);
    let pointer = evidence(&f, "b");
    adapter
        .complete_waypoint(f.id, "b", &request(&f, &pointer))
        .unwrap();
    adapter
        .review_result(
            &WaypointIdentity::Mission { mission_id: f.id },
            &request(&f, "review proposes another waypoint"),
        )
        .unwrap();
    let lifecycle = MissionLifecycle::open(&f.ws);
    let mut extra = lifecycle.get_mission_source(f.id).unwrap().waypoints[1].clone();
    extra.waypoint_id = "c".into();
    extra.part.waypoint_id = "c".into();
    extra.title = "Additional reviewed work".into();
    extra.depends_on = vec!["b".into()];
    let before = f.plan();
    lifecycle
        .apply_plan_operation(
            f.id,
            PlanOperationRequest {
                request_id: "result-review-add".into(),
                expected_revision: before.revision,
                operation: PlanOperation::CreateWaypoint {
                    waypoint: extra,
                    after: Some("b".into()),
                },
            },
        )
        .unwrap();
    assert_ne!(f.plan().plan_digest, before.plan_digest);
    assert_eq!(f.code(EntryPath::Dispatch), "approval_stale");
    f.approve("added-work", "added-work-event");
    assert!(f.approval().check_entry(f.id, EntryPath::Dispatch).is_ok());
    assert_eq!(
        adapter
            .complete_waypoint(f.id, "c", &request(&f, "evidence/b.json"))
            .unwrap_err()
            .code(),
        "prerequisite_evidence_required"
    );
}
