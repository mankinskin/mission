use super::*;

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
