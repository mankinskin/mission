//! W1 (MN-W1-01..04) contract tests for the shared lifecycle types, mappings,
//! version-1 control journal and capability traits.

use mission_api::lifecycle::MissionProjection;
use mission_api::{
    Capability, CommitCheckpoint, ControlEvent, ControlKind, ControlRequest, ControlTarget,
    DirectWaypoint, EligibilityDenial, ExecutionEligibility, LIFECYCLE_JOURNAL_VERSION,
    LifecycleError, LifecycleJournal, LifecycleState, MissionFacts, MissionLifecycle,
    PauseResumeCapability, ReopenCapability, TargetControl, ValidationGate, WaypointIdentity,
    WaypointState, project_mission, project_waypoint,
};
use tempfile::TempDir;
use uuid::Uuid;

use LifecycleState::*;

fn event(
    seq: u64,
    kind: ControlKind,
    target: ControlTarget,
    prior: LifecycleState,
) -> ControlEvent {
    ControlEvent {
        seq,
        op_id: Uuid::new_v4(),
        target,
        prior_seq: seq - 1,
        event: kind,
        prior_state: prior,
        evidence: (kind == ControlKind::Complete).then(|| "evidence".to_string()),
    }
}

fn wp(id: &str) -> ControlTarget {
    ControlTarget::Waypoint {
        waypoint_id: id.into(),
    }
}

fn journal(events: Vec<ControlEvent>) -> LifecycleJournal {
    LifecycleJournal {
        version: LIFECYCLE_JOURNAL_VERSION,
        events,
    }
}

// MN-W1-01
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
#[test]
fn journal_v1_round_trips_and_has_required_fields() {
    let original = journal(vec![
        event(1, ControlKind::Pause, wp("a"), Active),
        event(2, ControlKind::Resume, wp("a"), Active),
        event(3, ControlKind::Complete, ControlTarget::Mission, Active),
    ]);
    let text = original.to_json().unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["version"], 1);
    let first = &value["events"][0];
    for field in [
        "seq",
        "op_id",
        "target",
        "prior_seq",
        "event",
        "prior_state",
    ] {
        assert!(first.get(field).is_some(), "missing {field}");
    }
    assert_eq!(LifecycleJournal::from_json(&text).unwrap(), original);
}

#[test]
fn journal_errors_are_explicit() {
    let code = |text: &str| LifecycleJournal::from_json(text).unwrap_err().code();
    assert_eq!(code("{not json"), "lifecycle_journal_corrupt");
    assert_eq!(code("{\"events\":[]}"), "lifecycle_journal_corrupt");
    assert_eq!(
        code("{\"version\":2,\"events\":[]}"),
        "lifecycle_journal_unsupported_version"
    );
    assert_eq!(
        code("{\"version\":\"1\",\"events\":[]}"),
        "lifecycle_journal_unsupported_version"
    );
    assert_eq!(
        code("{\"version\":1,\"events\":[],\"extra\":1}"),
        "lifecycle_journal_corrupt"
    );
    assert_eq!(
        code("{\"version\":1,\"events\":[{\"seq\":1}]}"),
        "lifecycle_journal_corrupt"
    );

    let invalid = |events: Vec<ControlEvent>| journal(events).projection().unwrap_err().code();
    let t = ControlTarget::Mission;
    // Non-monotonic / gapped prior sequence / duplicate op id.
    let mut skipped = event(1, ControlKind::Pause, t.clone(), Active);
    skipped.prior_seq = 5;
    assert_eq!(invalid(vec![skipped]), "control_history_invalid");
    let first = event(1, ControlKind::Pause, t.clone(), Active);
    let mut repeat = event(2, ControlKind::Resume, t.clone(), Active);
    repeat.op_id = first.op_id;
    assert_eq!(
        invalid(vec![first.clone(), repeat]),
        "control_history_invalid"
    );
    let mut backwards = event(2, ControlKind::Resume, t.clone(), Active);
    backwards.seq = 1;
    backwards.prior_seq = 1;
    assert_eq!(
        invalid(vec![first.clone(), backwards]),
        "control_history_invalid"
    );
    // Illegal edges.
    assert_eq!(
        invalid(vec![event(1, ControlKind::Resume, t.clone(), Active)]),
        "control_history_invalid"
    );
    assert_eq!(
        invalid(vec![
            first.clone(),
            event(2, ControlKind::Pause, t.clone(), Active)
        ]),
        "control_history_invalid"
    );
    assert_eq!(
        invalid(vec![
            first.clone(),
            event(2, ControlKind::Resume, t.clone(), Planned)
        ]),
        "control_history_invalid"
    );
    assert_eq!(
        invalid(vec![
            first.clone(),
            event(2, ControlKind::Complete, t.clone(), Active)
        ]),
        "control_history_invalid",
    );
    assert_eq!(
        invalid(vec![event(1, ControlKind::Pause, t.clone(), Completed)]),
        "control_history_invalid"
    );
    let mut no_evidence = event(1, ControlKind::Complete, t.clone(), Active);
    no_evidence.evidence = None;
    assert_eq!(invalid(vec![no_evidence]), "control_history_invalid");
    // Conflicting terminal events.
    assert_eq!(
        invalid(vec![
            event(1, ControlKind::Complete, t.clone(), Active),
            event(2, ControlKind::Cancel, t.clone(), Active),
        ]),
        "control_history_invalid"
    );
    assert_eq!(
        invalid(vec![
            event(1, ControlKind::Cancel, t.clone(), Active),
            event(2, ControlKind::Pause, t, Active),
        ]),
        "control_history_invalid"
    );
}

#[test]
fn missing_journal_means_no_controls_and_bad_files_error() {
    let temp = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let empty = LifecycleJournal::read(temp.path(), id).unwrap();
    assert!(empty.events.is_empty());
    let projection = empty.projection().unwrap();
    assert_eq!(projection.last_seq, 0);
    assert_eq!(
        projection.target(&ControlTarget::Mission),
        TargetControl::default()
    );

    let path = LifecycleJournal::path(temp.path(), id);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "garbage").unwrap();
    assert_eq!(
        LifecycleJournal::read(temp.path(), id).unwrap_err().code(),
        "lifecycle_journal_corrupt"
    );
    std::fs::write(&path, "{\"version\":9,\"events\":[]}").unwrap();
    assert_eq!(
        LifecycleJournal::read(temp.path(), id).unwrap_err().code(),
        "lifecycle_journal_unsupported_version"
    );
}

// MN-W1-03: single-session control edges
#[test]
fn next_event_enforces_nonterminal_pause_cancel_resume_and_paused_completion() {
    let t = wp("a");
    let build = |journal: &LifecycleJournal, kind, state| {
        journal.next_event(t.clone(), kind, state, Uuid::new_v4(), Some("ev".into()))
    };
    let empty = LifecycleJournal::default();
    // Terminal work cannot pause or cancel.
    for terminal in [Completed, Cancelled] {
        for kind in [ControlKind::Pause, ControlKind::Cancel] {
            assert_eq!(
                build(&empty, kind, terminal).unwrap_err().code(),
                "illegal_transition"
            );
        }
    }
    assert_eq!(
        build(&empty, ControlKind::Resume, Active)
            .unwrap_err()
            .code(),
        "illegal_transition"
    );

    let pause = build(&empty, ControlKind::Pause, Blocked).unwrap();
    assert_eq!(
        (pause.seq, pause.prior_seq, pause.prior_state),
        (1, 0, Blocked)
    );
    let mut paused = empty.clone();
    paused.events.push(pause);
    // Resume restores the exact preceding nonterminal state.
    let resume = build(&paused, ControlKind::Resume, Paused).unwrap();
    assert_eq!(
        (resume.seq, resume.prior_seq, resume.prior_state),
        (2, 1, Blocked)
    );
    // Completing a paused waypoint is rejected.
    assert_eq!(
        build(&paused, ControlKind::Complete, Paused)
            .unwrap_err()
            .code(),
        "illegal_transition"
    );
    // Cancel of a paused waypoint records the preceding nonterminal state.
    assert_eq!(
        build(&paused, ControlKind::Cancel, Paused)
            .unwrap()
            .prior_state,
        Blocked
    );
}

#[test]
fn control_journal_does_not_touch_plan_revision_or_approval_inputs() {
    let temp = TempDir::new().unwrap();
    let lifecycle = MissionLifecycle::open(temp.path());
    let accepted = lifecycle
        .create(
            "Title".into(),
            "objective".into(),
            vec![DirectWaypoint {
                waypoint_id: "first".into(),
                title: "t".into(),
                depends_on: vec![],
                prompt: "p".into(),
                validation: vec![ValidationGate {
                    command: "true".into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: "m".into(),
                },
                non_goals: vec![],
            }],
        )
        .unwrap();
    let id = accepted.mission_id;
    let before = lifecycle.get(id).unwrap();
    let journal = journal(vec![event(1, ControlKind::Pause, wp("first"), Planned)]);
    let path = LifecycleJournal::path(temp.path(), id);
    std::fs::write(&path, journal.to_json().unwrap()).unwrap();
    let after = lifecycle.get(id).unwrap();
    assert_eq!(before, after);
    assert_eq!(after.bundle.manifest.revision, 1);
    assert_eq!(LifecycleJournal::read(temp.path(), id).unwrap(), journal);
    // Legacy native state stays untouched by control events.
    assert_eq!(
        after.bundle.waypoints[0].state,
        before.bundle.waypoints[0].state
    );
}

struct Stub;
impl PauseResumeCapability for Stub {
    fn provider_name(&self) -> &str {
        "stub"
    }
}
impl ReopenCapability for Stub {
    fn provider_name(&self) -> &str {
        "stub"
    }
}

#[test]
fn unsupported_capabilities_return_explicit_errors() {
    let identity = WaypointIdentity::Ticket {
        ticket_id: Uuid::new_v4(),
    };
    let request = ControlRequest {
        op_id: Uuid::new_v4(),
        expected_control_sequence: 0,
        evidence: None,
    };
    let error = PauseResumeCapability::pause(&Stub, &identity, &request).unwrap_err();
    assert_eq!(error.code(), "unsupported_capability");
    assert_eq!(
        error,
        LifecycleError::UnsupportedCapability {
            capability: Capability::PauseResume,
            provider: "stub".into()
        }
    );
    assert!(PauseResumeCapability::resume(&Stub, &identity, &request).is_err());
    assert_eq!(
        ReopenCapability::reopen(&Stub, &identity, &request)
            .unwrap_err()
            .to_string(),
        "unsupported_capability: reopen is not supported by stub"
    );
}

fn project(facts: &MissionFacts, control: &TargetControl) -> MissionProjection {
    project_mission(facts, control).unwrap()
}

// Review fix 1: completion is validated against required outcomes.
#[test]
fn completion_event_is_validated_against_required_outcomes() {
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
    let facts = |waypoints| MissionFacts {
        waypoints,
        required_entry_unavailable: false,
    };
    for bad in [
        vec![Completed, Planned],
        vec![Active],
        vec![Completed, Cancelled],
        vec![Paused],
        vec![],
    ] {
        let error = project_mission(&facts(bad.clone()), &complete).unwrap_err();
        assert_eq!(error.code(), "control_history_invalid", "{bad:?}");
    }
    let ok = project_mission(&facts(vec![Completed, Completed]), &complete).unwrap();
    assert_eq!((ok.state, ok.result_review_needed), (Completed, false));
}

// Review fix 2: unsatisfied required entry (cancelled/blocked) blocks the Mission.
#[test]
fn cancelled_or_blocked_required_entry_blocks_mission_in_api_projection() {
    let none = TargetControl::default();
    for first in [Cancelled, Blocked] {
        let projected = project_mission(
            &MissionFacts {
                waypoints: vec![Completed, first, Planned],
                required_entry_unavailable: false,
            },
            &none,
        )
        .unwrap();
        assert_eq!(projected.state, Blocked, "{first:?}");
    }
    // A cancelled entry is never eligible work.
    let eligibility = ExecutionEligibility::effective(Blocked, &none);
    assert!(!eligibility.is_eligible());
}

// Review fix 3: journal targets must belong to the Mission.
#[test]
fn journal_waypoint_targets_are_validated_against_membership() {
    let known = ["a".to_string()].into_iter().collect();
    let ok = journal(vec![
        event(1, ControlKind::Pause, wp("a"), Active),
        event(2, ControlKind::Cancel, ControlTarget::Mission, Active),
    ]);
    ok.validate_waypoint_targets(&known).unwrap();
    let ghost = journal(vec![event(1, ControlKind::Pause, wp("ghost"), Active)]);
    let error = ghost.validate_waypoint_targets(&known).unwrap_err();
    assert_eq!(error.code(), "control_history_invalid");
    assert!(error.to_string().contains("ghost"));
}

// Review fix 4: the module path is public.
#[test]
fn lifecycle_module_path_is_public_and_reexports_are_retained() {
    let via_module: mission_api::lifecycle::LifecycleState =
        mission_api::lifecycle::LifecycleState::Planned;
    let via_root: mission_api::LifecycleState = via_module;
    assert_eq!(via_root.as_str(), "planned");
    assert_eq!(mission_api::lifecycle::LIFECYCLE_JOURNAL_VERSION, 1);
}
