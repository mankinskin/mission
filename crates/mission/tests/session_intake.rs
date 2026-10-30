//! W2 (MN-W2-01..05) behavior tests for Session input intake and managed Missions.

use std::path::{Path, PathBuf};

use mission::intake::{
    GeneratedPlanState, IntakeError, IntakeStep, ManagedIntake, ManagedIntakeRequest, find,
    submit_and_create, submit_and_create_with_fault,
};
use session_api::{SessionError, SessionStoreConfig};
use uuid::Uuid;

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const ABC_DIGEST: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn reserved_mission(f: &Fixture, request_id: &str) -> Uuid {
    f.session()
        .read_intake(SESSION, request_id)
        .unwrap()
        .linkage
        .unwrap()
        .mission_id
}

fn pending_subset(error: IntakeError) -> (Option<Uuid>, Vec<IntakeStep>) {
    match error {
        IntakeError::Pending(receipt) => (receipt.mission_id, receipt.persisted),
        other => panic!("expected pending, got {other:?}"),
    }
}

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

struct Fixture {
    _dir: tempfile::TempDir,
    session_root: PathBuf,
    workspace: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::TempDir::new().unwrap();
    let session_root = dir.path().join("session-store");
    let workspace = dir.path().join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    SessionStoreConfig::new(&session_root)
        .init_runtime_context(session_api::SessionRuntimeInitRequest {
            session_id: Some(SESSION.to_string()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();
    Fixture {
        _dir: dir,
        session_root,
        workspace,
    }
}

impl Fixture {
    fn session(&self) -> SessionStoreConfig {
        SessionStoreConfig::new(&self.session_root)
    }

    fn submit(&self, request_id: &str, input: &[u8]) -> Result<ManagedIntake, IntakeError> {
        let session = self.session();
        submit_and_create(&ManagedIntakeRequest {
            session_store: &session,
            workspace_root: &self.workspace,
            session_id: SESSION,
            request_id,
            input,
        })
    }

    fn submit_with_fault(
        &self,
        request_id: &str,
        input: &[u8],
        fault: IntakeStep,
    ) -> Result<ManagedIntake, IntakeError> {
        let session = self.session();
        submit_and_create_with_fault(
            &ManagedIntakeRequest {
                session_store: &session,
                workspace_root: &self.workspace,
                session_id: SESSION,
                request_id,
                input,
            },
            Some(fault),
        )
    }

    fn find(&self, request_id: &str) -> Result<ManagedIntake, IntakeError> {
        find(&self.session(), &self.workspace, SESSION, request_id)
    }

    fn missions_root(&self) -> PathBuf {
        self.workspace.join(".workflow-tools/mission/missions")
    }

    fn mission_count(&self) -> usize {
        match std::fs::read_dir(self.missions_root()) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|entry| Uuid::parse_str(&entry.file_name().to_string_lossy()).is_ok())
                .count(),
            Err(_) => 0,
        }
    }
}

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
