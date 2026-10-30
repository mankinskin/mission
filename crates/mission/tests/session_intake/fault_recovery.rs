use super::*;

#[test]
fn every_fault_window_reports_the_persisted_subset_and_retry_resumes_once() {
    let windows = [
        (
            IntakeStep::InputSubmitted,
            vec![IntakeStep::InputSubmitted],
            false,
        ),
        (
            IntakeStep::MissionReserved,
            vec![IntakeStep::InputSubmitted, IntakeStep::MissionReserved],
            true,
        ),
        (
            IntakeStep::MissionCreated,
            vec![
                IntakeStep::InputSubmitted,
                IntakeStep::MissionReserved,
                IntakeStep::MissionCreated,
            ],
            true,
        ),
        (
            IntakeStep::ProvenanceCopied,
            vec![
                IntakeStep::InputSubmitted,
                IntakeStep::MissionReserved,
                IntakeStep::MissionCreated,
                IntakeStep::ProvenanceCopied,
            ],
            true,
        ),
    ];
    for (fault, expected, has_mission_id) in windows {
        let f = fixture();
        let error = f.submit_with_fault("r", b"abc", fault).unwrap_err();
        assert_eq!(error.code(), "pending", "{fault:?}: {error:?}");
        let IntakeError::Pending(receipt) = error else {
            unreachable!()
        };
        assert_eq!(receipt.persisted, expected, "{fault:?}");
        assert_eq!(receipt.mission_id.is_some(), has_mission_id, "{fault:?}");
        let find_error = f.find("r").unwrap_err();
        assert_eq!(find_error.code(), "pending", "{fault:?}: {find_error:?}");
        let persisted_link = f.session().read_intake(SESSION, "r").unwrap().linkage;
        assert!(
            persisted_link
                .map(|l| l.state != session_api::SessionIntakeLinkState::Linked)
                .unwrap_or(true)
        );
        let done = f.submit("r", b"abc").unwrap();
        if let Some(id) = receipt.mission_id {
            assert_eq!(done.mission_id, id, "{fault:?}");
        }
        assert_eq!(f.mission_count(), 1, "{fault:?}");
        assert_eq!(std::fs::read(&done.input_path).unwrap(), b"abc");
        assert_eq!(f.find("r").unwrap(), done);
    }
}

#[test]
fn retry_with_conflicting_bytes_after_a_fault_does_not_resume_or_create() {
    let f = fixture();
    f.submit_with_fault("r", b"abc", IntakeStep::MissionReserved)
        .unwrap_err();
    let error = f.submit("r", b"different").unwrap_err();
    assert_eq!(error.code(), "idempotency_conflict", "{error:?}");
    assert_eq!(f.mission_count(), 0);
}

#[test]
fn unrelated_existing_missions_are_never_adopted() {
    let f = fixture();
    let unrelated = mission_api::MissionLifecycle::open(&f.workspace)
        .create_draft("Unrelated".into(), "Not ours".into())
        .unwrap();
    let receipt = f.submit("r", b"abc").unwrap();
    assert_ne!(receipt.mission_id, unrelated.mission_id);
    assert_eq!(f.mission_count(), 2);
}

#[test]
fn failures_are_distinguishable_from_not_found() {
    let f = fixture();
    let unknown = "22222222-2222-4222-8222-222222222222";
    let session = f.session();
    let missing = f.find("never-submitted").unwrap_err();
    assert_eq!(missing.code(), "intake_missing", "{missing:?}");
    let not_found = find(&session, &f.workspace, unknown, "r").unwrap_err();
    assert_eq!(not_found.code(), "session_not_found", "{not_found:?}");
    let submit_unknown = submit_and_create(&ManagedIntakeRequest {
        session_store: &session,
        workspace_root: &f.workspace,
        session_id: unknown,
        request_id: "r",
        input: b"abc",
    })
    .unwrap_err();
    assert_eq!(
        submit_unknown.code(),
        "session_not_found",
        "{submit_unknown:?}"
    );
    let unavailable = find(&session, Path::new("relative"), SESSION, "r").unwrap_err();
    assert_eq!(
        unavailable.code(),
        "workspace_unavailable",
        "{unavailable:?}"
    );
    let _ = SessionError::WorkspaceFinished {
        session_id: String::new(),
    };
}

#[test]
fn planning_mission_creation_grants_no_executable_state() {
    let f = fixture();
    let receipt = f.submit("r", b"abc").unwrap();
    assert!(!receipt.executable);
    assert!(
        !mission_api::lifecycle::LifecycleJournal::path(&f.workspace, receipt.mission_id).exists(),
        "no lifecycle/approval journal is created by intake"
    );
    assert!(
        !receipt.mission_dir.join("accepted.json").exists()
            || mission_api::MissionLifecycle::open(&f.workspace)
                .get_mission_source(receipt.mission_id)
                .unwrap()
                .waypoints
                .is_empty()
    );
}
