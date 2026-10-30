use super::*;

#[test]
fn reserved_replay_after_session_finish_mutates_no_product_state() {
    let f = fixture();
    f.submit_with_fault("r", b"abc", IntakeStep::MissionReserved)
        .unwrap_err();
    f.session()
        .finish_workflow(SESSION, vec![], vec![], None)
        .unwrap();
    let error = f.submit("r", b"abc").unwrap_err();
    assert_eq!(error.code(), "session_finished", "{error:?}");
    assert_eq!(
        f.mission_count(),
        0,
        "no Mission may be created after finish"
    );
    let link = f
        .session()
        .read_intake(SESSION, "r")
        .unwrap()
        .linkage
        .unwrap();
    assert_eq!(link.state, session_api::SessionIntakeLinkState::Reserved);
}

#[cfg(unix)]
#[test]
fn dangling_finish_marker_is_an_incompatible_store_and_creates_nothing() {
    let f = fixture();
    let session_dir = f.session_root.join("sessions").join(SESSION);
    std::os::unix::fs::symlink(
        f.workspace.parent().unwrap().join("missing-finish-target"),
        session_dir.join("finish.json"),
    )
    .unwrap();
    let error = f.submit("r", b"abc").unwrap_err();
    assert_eq!(error.code(), "incompatible_store", "{error:?}");
    assert_eq!(f.mission_count(), 0);
    assert!(!session_dir.join("intake").exists());
}

#[cfg(unix)]
#[test]
fn symlinked_session_roots_are_incompatible_stores() {
    let f = fixture();
    let link = f.workspace.parent().unwrap().join("session-link");
    std::os::unix::fs::symlink(&f.session_root, &link).unwrap();
    let session = SessionStoreConfig::new(&link);
    let error = submit_and_create(&ManagedIntakeRequest {
        session_store: &session,
        workspace_root: &f.workspace,
        session_id: SESSION,
        request_id: "r",
        input: b"abc",
    })
    .unwrap_err();
    assert_eq!(error.code(), "incompatible_store", "{error:?}");
    assert_eq!(f.mission_count(), 0);
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
fn generated_plan_state_rejects_symlinks_directories_and_unreadable_paths() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    let receipt = f.submit("r", b"abc").unwrap();
    let generated = receipt.mission_dir.join("generated");
    let readme = generated.join("README.md");
    let outside = f.workspace.parent().unwrap().join("generated-outside");
    std::fs::create_dir_all(&outside).unwrap();
    assert_eq!(
        f.find("r").unwrap().generated_plan_entry.state,
        GeneratedPlanState::NotYetGenerated
    );
    std::os::unix::fs::symlink(&outside, &generated).unwrap();
    assert_eq!(f.find("r").unwrap_err().code(), "incompatible_store");
    std::fs::remove_file(&generated).unwrap();
    std::fs::create_dir_all(&readme).unwrap();
    assert_eq!(f.find("r").unwrap_err().code(), "incompatible_store");
    std::fs::remove_dir(&readme).unwrap();
    std::fs::write(outside.join("real.md"), b"x").unwrap();
    std::os::unix::fs::symlink(outside.join("real.md"), &readme).unwrap();
    assert_eq!(f.find("r").unwrap_err().code(), "incompatible_store");
    std::fs::remove_file(&readme).unwrap();
    std::fs::write(&readme, b"# plan").unwrap();
    assert_eq!(
        f.find("r").unwrap().generated_plan_entry.state,
        GeneratedPlanState::Generated
    );
    std::fs::remove_file(&readme).unwrap();
    std::fs::set_permissions(&generated, std::fs::Permissions::from_mode(0o000)).unwrap();
    let privileged = std::fs::read_dir(&generated).is_ok();
    let result = f.find("r");
    std::fs::set_permissions(&generated, std::fs::Permissions::from_mode(0o755)).unwrap();
    if !privileged {
        assert_eq!(result.unwrap_err().code(), "storage_error");
    }
}

#[cfg(unix)]
#[test]
fn real_io_failure_creating_the_mission_reports_persisted_subset_and_retry_resumes() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    let root = f.missions_root();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = root.join(".probe");
    if std::fs::File::create(&probe).is_ok() {
        std::fs::remove_file(&probe).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let error = f.submit("r", b"abc").unwrap_err();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (mission, persisted) = pending_subset(error);
    assert_eq!(
        persisted,
        vec![IntakeStep::InputSubmitted, IntakeStep::MissionReserved]
    );
    let reserved = reserved_mission(&f, "r");
    assert_eq!(mission, Some(reserved));
    let receipt = f.submit("r", b"abc").unwrap();
    assert_eq!(receipt.mission_id, reserved);
    assert_eq!(f.mission_count(), 1);
}

#[test]
fn partial_provenance_copy_is_reported_honestly_and_never_overwritten() {
    let f = fixture();
    f.submit_with_fault("r", b"abc", IntakeStep::MissionCreated)
        .unwrap_err();
    let mission = reserved_mission(&f, "r");
    let input_dir = f.missions_root().join(mission.to_string()).join("input");
    std::fs::create_dir_all(&input_dir).unwrap();
    std::fs::write(input_dir.join("submitted-input.bin"), b"abc").unwrap();
    let (id, persisted) = pending_subset(f.find("r").unwrap_err());
    assert_eq!(id, Some(mission));
    assert_eq!(
        persisted,
        vec![
            IntakeStep::InputSubmitted,
            IntakeStep::MissionReserved,
            IntakeStep::MissionCreated
        ]
    );
    let receipt = f.submit("r", b"abc").unwrap();
    assert_eq!(receipt.mission_id, mission);
    assert_eq!(std::fs::read(&receipt.input_path).unwrap(), b"abc");
    let g = fixture();
    g.submit_with_fault("r", b"abc", IntakeStep::MissionCreated)
        .unwrap_err();
    let mission = reserved_mission(&g, "r");
    let input_dir = g.missions_root().join(mission.to_string()).join("input");
    std::fs::create_dir_all(&input_dir).unwrap();
    std::fs::write(input_dir.join("submitted-input.bin"), b"zzz").unwrap();
    assert_eq!(
        g.submit("r", b"abc").unwrap_err().code(),
        "incompatible_store"
    );
    assert_eq!(
        std::fs::read(input_dir.join("submitted-input.bin")).unwrap(),
        b"zzz"
    );
}

#[test]
fn linked_find_and_replay_survive_a_later_title_and_objective_edit() {
    let f = fixture();
    let receipt = f.submit("r", b"abc").unwrap();
    let api = mission_api::MissionLifecycle::open(&f.workspace);
    let mut draft = api.get_mission_source(receipt.mission_id).unwrap();
    draft.title = "Edited by a later planner".into();
    draft.objective = "Edited objective".into();
    let saved = api
        .save_draft(receipt.mission_id, draft, receipt.revision)
        .unwrap();
    let found = f.find("r").unwrap();
    assert_eq!(found.revision, saved.revision);
    assert!(found.revision > receipt.revision);
    let again = f.submit("r", b"abc").unwrap();
    assert_eq!(again.mission_id, receipt.mission_id);
    assert_eq!(again.input_digest, receipt.input_digest);
    assert_eq!(f.mission_count(), 1);
}

#[test]
fn edited_draft_with_immutable_provenance_is_resumed_not_refused() {
    let f = fixture();
    f.submit_with_fault("r", b"abc", IntakeStep::ProvenanceCopied)
        .unwrap_err();
    let mission = reserved_mission(&f, "r");
    let api = mission_api::MissionLifecycle::open(&f.workspace);
    let mut draft = api.get_mission_source(mission).unwrap();
    draft.title = "Edited before linkage completed".into();
    api.save_draft(mission, draft, 1).unwrap();
    assert_eq!(f.submit("r", b"abc").unwrap().mission_id, mission);
    assert_eq!(f.mission_count(), 1);
}
