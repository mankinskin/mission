use mission::{DirectWaypoint, MissionStore, Operation, execute};
use serde_json::json;
use tempfile::TempDir;

fn waypoint() -> DirectWaypoint {
    serde_json::from_value(json!({
        "waypoint_id": "verify",
        "title": "Verify installation",
        "prompt": "Run the installed binaries and retain their actual output.",
        "validation": [{"command": "mission --help", "cwd": "/isolated/install"}],
        "commit_checkpoint": {"message": "Record installation evidence; no code commit"},
        "non_goals": ["Do not change the stable installation"]
    }))
    .unwrap()
}

fn create(workspace: &TempDir) -> uuid::Uuid {
    let result = execute(
        workspace.path(),
        Operation::CreateMission {
            title: "Installation".into(),
            objective: "Verify the isolated installation.".into(),
            waypoints: vec![waypoint()],
        },
    );
    assert_eq!(result.status, "ok", "{:?}", result.diagnostics);
    result.mission_id.unwrap()
}

#[test]
fn direct_authoring_retains_executable_waypoint_content() {
    let workspace = TempDir::new().unwrap();
    let id = create(&workspace);
    let accepted = MissionStore::open(workspace.path()).get(id).unwrap();
    let waypoint = &accepted.bundle.waypoints[0];
    assert_eq!(
        waypoint.prompt,
        "Run the installed binaries and retain their actual output."
    );
    assert_eq!(waypoint.validation[0].command, "mission --help");
    assert_eq!(
        waypoint.validation[0].cwd.as_deref(),
        Some("/isolated/install")
    );
    assert_eq!(
        waypoint.commit_checkpoint.message,
        "Record installation evidence; no code commit"
    );
    assert_eq!(
        waypoint.non_goals,
        ["Do not change the stable installation"]
    );
}

#[test]
fn stored_id_publication_generates_verified_projection_with_operator_results() {
    let workspace = TempDir::new().unwrap();
    let id = create(&workspace);
    let result = execute(
        workspace.path(),
        Operation::RecordValidationResult {
            mission_id: id,
            expected_current_revision: 1,
            result: mission::OperatorResult {
                waypoint_id: "verify".into(),
                outcome: "passed".into(),
                operator_command: "mission --help".into(),
                evidence_pointer: "ce://installation/test/output".into(),
            },
        },
    );
    assert_eq!(result.status, "ok");
    let published = execute(
        workspace.path(),
        Operation::PublishMission {
            mission_id: id,
            expected_current_revision: 2,
        },
    );
    assert_eq!(published.status, "ok", "{:?}", published.diagnostics);
    let checked = execute(
        workspace.path(),
        Operation::CheckGenerated { mission_id: id },
    );
    assert_eq!(checked.status, "ok", "{:?}", checked.diagnostics);
    let path = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(id.to_string())
        .join("generated/ROADMAP.md");
    let roadmap = std::fs::read_to_string(path).unwrap();
    assert!(roadmap.contains("ce://installation/test/output"));
    assert!(roadmap.contains("mission --help"));
    assert!(roadmap.contains("passed"));
    let read_back =
        serde_json::to_value(execute(workspace.path(), Operation::Get { mission_id: id })).unwrap();
    assert_eq!(read_back["publication_state"], "current");
    std::fs::write(
        workspace
            .path()
            .join(".workflow-tools/mission/missions")
            .join(id.to_string())
            .join("generated/ROADMAP.md"),
        "changed outside Mission tools",
    )
    .unwrap();
    let drifted =
        serde_json::to_value(execute(workspace.path(), Operation::Get { mission_id: id })).unwrap();
    assert_eq!(drifted["publication_state"], "drift");
}

#[test]
fn unsupported_evidence_reference_is_rejected_without_writing_a_revision() {
    let workspace = TempDir::new().unwrap();
    let id = create(&workspace);
    let before = MissionStore::open(workspace.path()).get(id).unwrap();
    let rejected = execute(
        workspace.path(),
        Operation::RecordValidationResult {
            mission_id: id,
            expected_current_revision: 1,
            result: mission::OperatorResult {
                waypoint_id: "verify".into(),
                outcome: "passed".into(),
                operator_command: "mission --help".into(),
                evidence_pointer: "test://installation/output".into(),
            },
        },
    );
    assert_eq!(rejected.status, "error");
    assert!(rejected
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "mission-reference-unknown"));
    assert_eq!(MissionStore::open(workspace.path()).get(id).unwrap(), before);
}

#[test]
fn invalid_operator_result_has_actionable_diagnostic_and_preserves_record() {
    let workspace = TempDir::new().unwrap();
    let id = create(&workspace);
    let before = MissionStore::open(workspace.path()).get(id).unwrap();
    let rejected = execute(
        workspace.path(),
        Operation::RecordValidationResult {
            mission_id: id,
            expected_current_revision: 1,
            result: mission::OperatorResult {
                waypoint_id: "missing".into(),
                outcome: "passed".into(),
                operator_command: "mission --help".into(),
                evidence_pointer: "test://installation/output".into(),
            },
        },
    );
    assert_eq!(rejected.status, "error");
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(
                |diagnostic| diagnostic.waypoint_id.as_deref() == Some("missing")
                    && !diagnostic.message.is_empty()
            ),
        "{:?}",
        rejected.diagnostics
    );
    assert_eq!(
        MissionStore::open(workspace.path()).get(id).unwrap(),
        before
    );
}

#[test]
fn rejected_create_returns_its_allocated_id_without_an_accepted_record() {
    let workspace = TempDir::new().unwrap();
    let mut invalid_waypoint = waypoint();
    invalid_waypoint.prompt.clear();
    let rejected = execute(
        workspace.path(),
        Operation::CreateMission {
            title: "Rejected".into(),
            objective: "This create is intentionally invalid.".into(),
            waypoints: vec![invalid_waypoint],
        },
    );

    assert_eq!(rejected.status, "error");
    let mission_id = rejected
        .mission_id
        .expect("rejected create must retain its API-allocated mission UUID");
    assert!(uuid::Uuid::parse_str(&mission_id.to_string()).is_ok());
    assert!(
        MissionStore::open(workspace.path())
            .get(mission_id)
            .is_err()
    );
}

#[test]
fn direct_edits_cannot_override_ticket_or_child_authority() {
    for owner in ["ticket", "child"] {
        let workspace = TempDir::new().unwrap();
        let child_id = create(&workspace);
        let id = create(&workspace);
        let store = MissionStore::open(workspace.path());
        let mut bundle = store.get(id).unwrap().bundle;
        bundle.manifest.revision = 2;
        bundle.waypoints[0].state = None;
        if owner == "ticket" {
            let ticket_id = uuid::Uuid::new_v4();
            let path = workspace.path().join(".workflow-tools/ticket/tickets").join(ticket_id.to_string());
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("ticket.toml"), format!(
                "id = \"{ticket_id}\"\ncreated_at = \"2026-10-29T00:00:00Z\"\ntitle = \"Existing Ticket\"\nstate = \"ready\"\n"
            )).unwrap();
            bundle.waypoints[0].scope = mission::Scope::TicketBacked {
                ticket_ref: format!("ce://default/ticket/{ticket_id}"),
            };
        } else {
            bundle.waypoints[0].child_mission_ref =
                Some(format!("ce://default/mission/{child_id}"));
        }
        let before = store.import(id, bundle, 1).unwrap();
        for operation in [
            Operation::RecordProgress {
                mission_id: id,
                waypoint_id: "verify".into(),
                state: mission::WaypointState::Completed,
                expected_current_revision: 2,
            },
            Operation::EditWaypoint {
                mission_id: id,
                waypoint: waypoint(),
                expected_current_revision: 2,
            },
        ] {
            let result = execute(workspace.path(), operation);
            assert_eq!(result.status, "error", "{owner}");
            assert_eq!(result.diagnostics[0].code, "externally-owned-waypoint");
            assert_eq!(store.get(id).unwrap(), before);
        }
    }
}

#[test]
fn invalid_waypoint_content_never_replaces_accepted_data() {
    let workspace = TempDir::new().unwrap();
    let id = create(&workspace);
    let store = MissionStore::open(workspace.path());
    let before = store.get(id).unwrap();
    let mut empty_prompt = waypoint();
    empty_prompt.prompt.clear();
    let mut empty_validation = waypoint();
    empty_validation.validation.clear();
    let mut blank_checkpoint = waypoint();
    blank_checkpoint.commit_checkpoint.message = " ".into();
    let mut unknown_dependency = waypoint();
    unknown_dependency.depends_on = vec!["missing".into()];
    let mut unknown_waypoint = waypoint();
    unknown_waypoint.waypoint_id = "missing".into();
    for waypoint in [
        empty_prompt,
        empty_validation,
        blank_checkpoint,
        unknown_dependency,
        unknown_waypoint,
    ] {
        let result = execute(
            workspace.path(),
            Operation::EditWaypoint {
                mission_id: id,
                waypoint,
                expected_current_revision: 1,
            },
        );
        assert_eq!(result.status, "error");
        assert!(!result.diagnostics.is_empty());
        assert_eq!(store.get(id).unwrap(), before);
    }
    let duplicate = execute(
        workspace.path(),
        Operation::AddWaypoint {
            mission_id: id,
            waypoint: waypoint(),
            expected_current_revision: 1,
        },
    );
    assert_eq!(duplicate.status, "error");
    assert!(!duplicate.diagnostics.is_empty());
    assert_eq!(store.get(id).unwrap(), before);
}
