use mission_api::*;
use uuid::Uuid;

fn mission(root: &std::path::Path) -> Uuid {
    MissionLifecycle::open(root)
        .create(
            "Journal".into(),
            "Goal".into(),
            vec![DirectWaypoint {
                waypoint_id: "a".into(),
                title: "A".into(),
                depends_on: vec![],
                prompt: "Do A".into(),
                validation: vec![ValidationGate {
                    command: "true".into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: "done".into(),
                },
                non_goals: vec![],
            }],
        )
        .unwrap()
        .mission_id
}

#[test]
fn v1_journal_writes_are_durable_and_require_the_previous_sequence() {
    let dir = tempfile::tempdir_in(".").unwrap();
    let id = mission(dir.path());
    let lifecycle = MissionLifecycle::open(dir.path());
    let event = LifecycleJournal::default()
        .next_event(
            ControlTarget::Mission,
            ControlKind::Pause,
            LifecycleState::Planned,
            Uuid::new_v4(),
            Some("inspection".into()),
        )
        .unwrap();
    let tx = lifecycle.begin_approval_transaction(id).unwrap();
    tx.append_control(dir.path(), event.clone()).unwrap();
    assert_eq!(
        tx.append_control(dir.path(), event).unwrap_err().code(),
        "stale_control_sequence"
    );
    drop(tx);
    let journal = LifecycleJournal::read(dir.path(), id).unwrap();
    assert_eq!(journal.events.len(), 1);
    assert_eq!(
        journal
            .projection()
            .unwrap()
            .target(&ControlTarget::Mission)
            .paused_from,
        Some(LifecycleState::Planned)
    );
}

#[test]
fn corrupt_and_unknown_journal_versions_never_become_empty_history() {
    let dir = tempfile::tempdir_in(".").unwrap();
    let id = mission(dir.path());
    let path = LifecycleJournal::path(dir.path(), id);
    std::fs::write(&path, "{\"version\":99,\"events\":[]}").unwrap();
    assert_eq!(
        LifecycleJournal::read(dir.path(), id).unwrap_err().code(),
        "lifecycle_journal_unsupported_version"
    );
    std::fs::write(&path, "broken").unwrap();
    assert_eq!(
        LifecycleJournal::read(dir.path(), id).unwrap_err().code(),
        "lifecycle_journal_corrupt"
    );
}

#[test]
fn immutable_completion_history_cannot_be_rewritten_or_reactivated() {
    let dir = tempfile::tempdir_in(".").unwrap();
    let id = mission(dir.path());
    let lifecycle = MissionLifecycle::open(dir.path());
    let tx = lifecycle.begin_approval_transaction(id).unwrap();
    let mut state = tx.control_state().unwrap();
    state.completions.push(CompletionRecord {
        sequence: 1,
        waypoint_id: "a".into(),
        plan_digest: "digest".into(),
        plan_revision: 1,
        result_ref: "evidence".into(),
        current: true,
    });
    tx.write_control_state(&mut state, 0).unwrap();
    state.completions[0].result_ref = "rewritten".into();
    assert_eq!(
        tx.write_control_state(&mut state, 1).unwrap_err().code(),
        "lifecycle_journal_corrupt"
    );
}

#[test]
fn runtime_schema_version_and_sequence_gaps_are_explicit_errors() {
    let dir = tempfile::tempdir_in(".").unwrap();
    let id = mission(dir.path());
    let lifecycle = MissionLifecycle::open(dir.path());
    let tx = lifecycle.begin_approval_transaction(id).unwrap();
    let mut state = MissionControlState::default();
    state.version = 2;
    state.sequence = 1;
    tx.write_record_new(
        "controls/00000000000000000001.json",
        &serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    assert_eq!(
        tx.control_state().unwrap_err().code(),
        "lifecycle_journal_unsupported_version"
    );
}

#[test]
fn terminal_phase_cannot_regress_without_finalizing_a_recorded_reopen() {
    let dir = tempfile::tempdir_in(".").unwrap();
    let id = mission(dir.path());
    let lifecycle = MissionLifecycle::open(dir.path());
    let tx = lifecycle.begin_approval_transaction(id).unwrap();
    let mut state = tx.control_state().unwrap();
    state.phase = LifecycleState::Completed;
    state.phase_reason = Some("validated result".into());
    state.result_review = Some("accepted".into());
    state.completions.push(CompletionRecord {
        sequence: 1,
        waypoint_id: "a".into(),
        plan_digest: "digest".into(),
        plan_revision: 1,
        result_ref: "evidence".into(),
        current: true,
    });
    tx.write_control_state(&mut state, 0).unwrap();
    for phase in [LifecycleState::Active, LifecycleState::Planned] {
        state.phase = phase;
        assert_eq!(
            tx.write_control_state(&mut state, 1).unwrap_err().code(),
            "lifecycle_journal_corrupt"
        );
        assert_eq!(tx.control_state().unwrap().phase, LifecycleState::Completed);
    }
}

#[test]
fn raw_progress_and_locked_progress_cannot_bypass_validated_completion() {
    let dir = tempfile::tempdir_in(".").unwrap();
    let id = mission(dir.path());
    let lifecycle = MissionLifecycle::open(dir.path());
    let before = lifecycle.get(id).unwrap();
    for locked in [false, true] {
        let error = if locked {
            lifecycle
                .begin_approval_transaction(id)
                .unwrap()
                .record_progress("a", WaypointState::Completed, 1)
                .unwrap_err()
        } else {
            lifecycle
                .record_progress(id, "a", WaypointState::Completed, 1)
                .unwrap_err()
        };
        let MissionError::Validation(_, diagnostics) = error else {
            panic!("expected completion gate");
        };
        assert_eq!(diagnostics[0].code, "completion_evidence_required");
        assert_eq!(lifecycle.get(id).unwrap(), before);
    }
}
