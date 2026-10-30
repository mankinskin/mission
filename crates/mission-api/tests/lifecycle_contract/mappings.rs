use super::mission_subject::project;
use super::*;

#[test]
fn ticket_states_map_exactly_and_unknown_is_explicit() {
    for (native, shared) in [
        ("planning", Planned),
        ("ready", Planned),
        ("in-implementation", Active),
        ("in-review", Active),
        ("on-hold", Paused),
        ("blocked", Blocked),
        ("done", Completed),
        ("cancelled", Cancelled),
    ] {
        assert_eq!(
            LifecycleState::from_ticket_state(native),
            Ok(shared),
            "{native}"
        );
    }
    assert_ne!(
        LifecycleState::from_ticket_state("cancelled").unwrap(),
        LifecycleState::from_ticket_state("blocked").unwrap()
    );
    let error = LifecycleState::from_ticket_state("invented").unwrap_err();
    assert_eq!(error.code(), "ticket_state_unmapped");
    assert_eq!(
        error,
        LifecycleError::TicketStateUnmapped("invented".into())
    );
    assert!(LifecycleState::from_ticket_state("").is_err());
    assert!(LifecycleState::from_ticket_state("Done").is_err());
}

#[test]
fn single_session_states_map_without_new_legacy_values() {
    assert_eq!(
        LifecycleState::from_waypoint_state(&WaypointState::Pending),
        Planned
    );
    assert_eq!(
        LifecycleState::from_waypoint_state(&WaypointState::InProgress),
        Active
    );
    assert_eq!(
        LifecycleState::from_waypoint_state(&WaypointState::Completed),
        Completed
    );
    assert_eq!(
        LifecycleState::from_waypoint_state(&WaypointState::Blocked),
        Blocked
    );
    // Legacy enum must not accept paused/cancelled.
    assert!(serde_json::from_str::<WaypointState>("\"paused\"").is_err());
    assert!(serde_json::from_str::<WaypointState>("\"cancelled\"").is_err());
    assert_eq!(serde_json::to_string(&Paused).unwrap(), "\"paused\"");
}

#[test]
fn waypoint_control_projection_and_precedence() {
    let none = TargetControl::default();
    assert_eq!(
        project_waypoint(&WaypointState::InProgress, &none),
        Ok(Active)
    );

    let paused = journal(vec![event(1, ControlKind::Pause, wp("a"), Active)])
        .projection()
        .unwrap();
    let control = paused.target(&wp("a"));
    assert_eq!(
        project_waypoint(&WaypointState::InProgress, &control),
        Ok(Paused)
    );
    // Other targets are unaffected.
    assert_eq!(
        project_waypoint(&WaypointState::InProgress, &paused.target(&wp("b"))),
        Ok(Active)
    );
    // Native progress during a pause is a conflict, not silently accepted.
    assert!(project_waypoint(&WaypointState::Completed, &control).is_err());
    assert!(project_waypoint(&WaypointState::Pending, &control).is_err());

    let resumed = journal(vec![
        event(1, ControlKind::Pause, wp("a"), Planned),
        event(2, ControlKind::Resume, wp("a"), Planned),
    ])
    .projection()
    .unwrap();
    assert_eq!(
        project_waypoint(&WaypointState::Pending, &resumed.target(&wp("a"))),
        Ok(Planned)
    );

    let cancelled = journal(vec![
        event(1, ControlKind::Pause, wp("a"), Active),
        event(2, ControlKind::Cancel, wp("a"), Active),
    ])
    .projection()
    .unwrap();
    assert_eq!(
        project_waypoint(&WaypointState::InProgress, &cancelled.target(&wp("a"))),
        Ok(Cancelled)
    );
    assert!(project_waypoint(&WaypointState::Completed, &cancelled.target(&wp("a"))).is_err());
}

#[test]
fn mission_projection_rows() {
    let facts = |waypoints: Vec<LifecycleState>, blocked| MissionFacts {
        waypoints,
        required_entry_unavailable: blocked,
    };
    let none = TargetControl::default();
    // Empty draft and nothing started: planned.
    assert_eq!(project(&facts(vec![], false), &none).state, Planned);
    assert_eq!(
        project(&facts(vec![Planned, Planned], false), &none).state,
        Planned
    );
    // Running work: active.
    let running = project(&facts(vec![Completed, Active, Planned], false), &none);
    assert_eq!(
        (running.state, running.result_review_needed),
        (Active, false)
    );
    // Ticket-backed paused waypoint still means started work.
    assert_eq!(
        project(&facts(vec![Paused, Planned], false), &none).state,
        Active
    );
    // Blocked entry.
    assert_eq!(
        project(&facts(vec![Blocked, Planned], true), &none).state,
        Blocked
    );
    // Legacy all-completed without explicit completion: active + review needed.
    let legacy = project(&facts(vec![Completed, Completed], false), &none);
    assert_eq!((legacy.state, legacy.result_review_needed), (Active, true));

    let target = ControlTarget::Mission;
    let complete = journal(vec![event(
        1,
        ControlKind::Complete,
        target.clone(),
        Active,
    )])
    .projection()
    .unwrap()
    .target(&target);
    let done = project(&facts(vec![Completed], false), &complete);
    assert_eq!((done.state, done.result_review_needed), (Completed, false));

    let pause = journal(vec![event(1, ControlKind::Pause, target.clone(), Active)])
        .projection()
        .unwrap()
        .target(&target);
    // Pause precedes blocked entry and active/planned.
    assert_eq!(project(&facts(vec![Blocked], true), &pause).state, Paused);
    let cancel = journal(vec![
        event(1, ControlKind::Pause, target.clone(), Active),
        event(2, ControlKind::Cancel, target.clone(), Active),
    ])
    .projection()
    .unwrap()
    .target(&target);
    // Terminal cancel precedes pause.
    assert_eq!(
        project(&facts(vec![Active], false), &cancel).state,
        Cancelled
    );
}

#[test]
fn mission_control_gates_eligibility_without_changing_shared_state() {
    let target = ControlTarget::Mission;
    let paused = journal(vec![event(1, ControlKind::Pause, target.clone(), Active)])
        .projection()
        .unwrap()
        .target(&target);
    // A Ticket-backed waypoint remains active; only eligibility is denied.
    let eligibility = ExecutionEligibility::effective(Active, &paused);
    assert_eq!(
        eligibility,
        ExecutionEligibility::Denied {
            denial: EligibilityDenial::MissionPaused
        }
    );
    assert!(ExecutionEligibility::effective(Active, &TargetControl::default()).is_eligible());
    assert!(ExecutionEligibility::effective(Planned, &TargetControl::default()).is_eligible());
    for state in [Paused, Blocked, Completed, Cancelled] {
        assert_eq!(
            ExecutionEligibility::effective(state, &TargetControl::default()),
            ExecutionEligibility::Denied {
                denial: EligibilityDenial::State(state)
            }
        );
    }
}

// MN-W1-04: journal schema
