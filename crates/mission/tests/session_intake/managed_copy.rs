use super::*;

#[test]
fn session_owns_exact_bytes_and_mission_keeps_an_identical_provenance_copy() {
    let f = fixture();
    let bytes: &[u8] = b"\xff\x00 raw \r\n bytes\r\n";
    let receipt = f.submit("r-42", bytes).unwrap();
    assert_eq!(std::fs::read(&receipt.session_input_path).unwrap(), bytes);
    assert_eq!(std::fs::read(&receipt.input_path).unwrap(), bytes);
    assert!(receipt.session_input_path.starts_with(&f.session_root));
    assert!(receipt.input_path.starts_with(&receipt.mission_dir));
    assert_ne!(receipt.session_input_path, receipt.input_path);
    let abc = f.submit("r-abc", b"abc").unwrap();
    assert_eq!(abc.input_digest, ABC_DIGEST);
    assert_ne!(abc.mission_id, receipt.mission_id);
    let intake = f.session().read_intake(SESSION, "r-42").unwrap();
    let link = intake.linkage.unwrap();
    assert_eq!(link.mission_id, receipt.mission_id);
    assert_eq!(link.state, session_api::SessionIntakeLinkState::Linked);
    assert_eq!(link.mission_workspace_root, f.workspace);
}

#[test]
fn intake_never_overwrites_the_capture_transcript() {
    let f = fixture();
    let session = f.session();
    session
        .capture_copilot_hook(session_api::CopilotHookPayload {
            session_id: SESSION.to_string(),
            workspace_path: "ws".to_string(),
            captured_at: chrono::Utc::now(),
            conversation_id: None,
            agent_id: None,
            model: None,
            trigger: None,
            provisioning: None,
            messages: vec![session_api::CopilotHookMessage {
                role: session_api::SessionRole::User,
                content: "captured".to_string(),
                tool_name: None,
                captured_at: None,
                event_meta: None,
            }],
            events: vec![],
            runtime: None,
        })
        .unwrap();
    let transcript = f
        .session_root
        .join("sessions")
        .join(SESSION)
        .join("transcript.json");
    let before = std::fs::read(&transcript).unwrap();
    f.submit("r", b"abc").unwrap();
    assert_eq!(std::fs::read(&transcript).unwrap(), before);
}

#[test]
fn finished_session_rejects_new_intake_without_creating_anything() {
    let f = fixture();
    let done = f.submit("done", b"abc").unwrap();
    f.session()
        .finish_workflow(SESSION, vec![], vec![], None)
        .unwrap();
    let count = f.mission_count();
    let error = f.submit("new", b"abc").unwrap_err();
    assert_eq!(error.code(), "session_finished", "{error:?}");
    assert_eq!(f.mission_count(), count);
    assert!(
        !f.session_root
            .join("sessions")
            .join(SESSION)
            .join("intake/new")
            .exists()
    );
    assert_eq!(
        f.submit("done", b"abc").unwrap().mission_id,
        done.mission_id
    );
    assert_eq!(f.find("done").unwrap().mission_id, done.mission_id);
}

#[test]
fn replay_after_restart_returns_same_mission_and_paths() {
    let f = fixture();
    let first = f.submit("r-42", b"abc").unwrap();
    assert_eq!(f.submit("r-42", b"abc").unwrap(), first);
    assert_eq!(f.find("r-42").unwrap(), first);
    assert_eq!(f.mission_count(), 1);
}

#[test]
fn same_request_with_different_bytes_is_an_idempotency_conflict_without_new_mission() {
    let f = fixture();
    let first = f.submit("r-42", b"abc").unwrap();
    let error = f.submit("r-42", b"abd").unwrap_err();
    assert_eq!(error.code(), "idempotency_conflict", "{error:?}");
    assert_eq!(f.mission_count(), 1);
    assert_eq!(std::fs::read(&first.session_input_path).unwrap(), b"abc");
    assert_eq!(std::fs::read(&first.input_path).unwrap(), b"abc");
}

#[test]
fn different_workspace_roots_are_independent_operations() {
    let f = fixture();
    let first = f.submit("r", b"abc").unwrap();
    let other = f.workspace.parent().unwrap().join("ws2");
    std::fs::create_dir_all(&other).unwrap();
    let session = f.session();
    let error = submit_and_create(&ManagedIntakeRequest {
        session_store: &session,
        workspace_root: &other,
        session_id: SESSION,
        request_id: "r",
        input: b"abc",
    })
    .unwrap_err();
    assert_eq!(error.code(), "workspace_unavailable", "{error:?}");
    assert!(!other.join(".workflow-tools").exists());
    assert_eq!(f.find("r").unwrap().mission_id, first.mission_id);
}

#[test]
fn empty_store_bootstrap_returns_managed_layout_receipt() {
    let f = fixture();
    assert!(!f.workspace.join(".workflow-tools").exists());
    let receipt = f.submit("r", b"abc").unwrap();
    assert_eq!(receipt.revision, 1);
    assert_eq!(receipt.workspace_root, f.workspace);
    assert_eq!(
        receipt.mission_dir,
        f.missions_root().join(receipt.mission_id.to_string())
    );
    assert!(
        receipt
            .input_path
            .starts_with(receipt.mission_dir.join("input"))
    );
    assert_eq!(receipt.source_path, receipt.mission_dir.join("source.json"));
    assert!(receipt.source_path.is_file());
    assert_eq!(
        receipt.generated_plan_entry.path,
        receipt.mission_dir.join("generated/README.md")
    );
    assert_eq!(
        receipt.generated_plan_entry.state,
        GeneratedPlanState::NotYetGenerated
    );
    assert!(!receipt.generated_plan_entry.path.exists());
    assert!(!receipt.mission_dir.join("generated").exists());
    let draft = mission_api::MissionLifecycle::open(&f.workspace)
        .get_mission_source(receipt.mission_id)
        .unwrap();
    assert_eq!(draft.revision, receipt.revision);
    assert!(!draft.title.is_empty() && !draft.objective.is_empty());
    assert!(
        draft.objective.contains("pending later planning"),
        "{}",
        draft.objective
    );
    assert!(draft.waypoints.is_empty());
}

#[test]
fn unsafe_request_ids_are_rejected_before_any_write() {
    let f = fixture();
    for bad in ["", "../escape", "a/b", ".hidden", "a b"] {
        let error = f.submit(bad, b"abc").unwrap_err();
        assert_eq!(error.code(), "invalid_request", "{bad:?}: {error:?}");
    }
    assert!(!f.workspace.join(".workflow-tools").exists());
    assert!(
        !f.session_root
            .join("sessions")
            .join(SESSION)
            .join("intake")
            .exists()
    );
}

#[cfg(unix)]
#[test]
fn symlinked_workspace_stores_are_rejected_without_writing_outside() {
    let f = fixture();
    let outside = f.workspace.parent().unwrap().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, f.workspace.join(".workflow-tools")).unwrap();
    let error = f.submit("r", b"abc").unwrap_err();
    assert_eq!(error.code(), "workspace_unavailable", "{error:?}");
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
}

#[test]
fn unusable_or_incompatible_roots_are_structured_failures() {
    let f = fixture();
    let session = f.session();
    let error = submit_and_create(&ManagedIntakeRequest {
        session_store: &session,
        workspace_root: Path::new("relative/ws"),
        session_id: SESSION,
        request_id: "r",
        input: b"abc",
    })
    .unwrap_err();
    assert_eq!(error.code(), "workspace_unavailable", "{error:?}");
    let file_root = f.workspace.parent().unwrap().join("file-root");
    std::fs::write(&file_root, b"x").unwrap();
    let error = submit_and_create(&ManagedIntakeRequest {
        session_store: &session,
        workspace_root: &file_root,
        session_id: SESSION,
        request_id: "r",
        input: b"abc",
    })
    .unwrap_err();
    assert_eq!(error.code(), "workspace_unavailable", "{error:?}");
    let bad = f.workspace.parent().unwrap().join("bad-ws");
    std::fs::create_dir_all(bad.join(".workflow-tools/mission")).unwrap();
    std::fs::write(
        bad.join(".workflow-tools/mission/missions"),
        b"not a directory",
    )
    .unwrap();
    let error = submit_and_create(&ManagedIntakeRequest {
        session_store: &session,
        workspace_root: &bad,
        session_id: SESSION,
        request_id: "r2",
        input: b"abc",
    })
    .unwrap_err();
    assert_eq!(error.code(), "incompatible_store", "{error:?}");
    assert_eq!(
        std::fs::read(bad.join(".workflow-tools/mission/missions")).unwrap(),
        b"not a directory"
    );
}
