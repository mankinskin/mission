use super::*;

#[test]
fn direct_authoring_persists_evidence_and_rejects_stale_writes() {
    let workspace = TempDir::new().unwrap();
    let created = execute_cli(
        workspace.path(),
        Operation::CreateMission {
            title: "Direct".into(),
            objective: "Ship it.".into(),
            waypoints: vec![mission::DirectWaypoint {
                waypoint_id: "install".into(),
                title: "Install".into(),
                depends_on: vec![],
                prompt: "Install into the isolated test directory.".into(),
                validation: vec![mission::ValidationGate {
                    command: "cargo test".into(),
                    cwd: None,
                }],
                commit_checkpoint: mission::CommitCheckpoint {
                    message: "Record fixture evidence".into(),
                },
                non_goals: vec![],
            }],
        },
    );
    let id = created.mission_id.unwrap();
    assert_eq!(created.revision, Some(1));
    assert_eq!(
        execute_mcp(
            workspace.path(),
            Operation::EditMissionObjective {
                mission_id: id,
                objective: "Edited.".into(),
                expected_current_revision: 1,
            }
        )
        .revision,
        Some(2)
    );
    assert_eq!(
        execute_cli(
            workspace.path(),
            Operation::RecordValidationResult {
                mission_id: id,
                expected_current_revision: 2,
                result: mission::OperatorResult {
                    waypoint_id: "install".into(),
                    outcome: "passed".into(),
                    operator_command: "cargo test".into(),
                    evidence_pointer: "ce://run/test/1".into()
                },
            }
        )
        .revision,
        Some(3)
    );
    let stale = execute_cli(
        workspace.path(),
        Operation::PublishMission {
            mission_id: id,
            expected_current_revision: 2,
        },
    );
    assert_eq!(stale.status, "error");
    let read_back = execute_mcp(workspace.path(), Operation::Get { mission_id: id });
    assert_eq!(read_back.revision, Some(3));
    assert_eq!(
        read_back.accepted.unwrap().bundle.manifest.operator_results[0].evidence_pointer,
        "ce://run/test/1"
    );
}

#[test]
fn validate_preview_does_not_mutate() {
    let workspace = TempDir::new().unwrap();
    let manifest = write_bundle(workspace.path(), 1, "preview");
    let result = execute_cli(
        workspace.path(),
        Operation::ValidatePreview {
            manifest_path: manifest,
        },
    );

    assert_eq!(result.status, "ok");
    assert!(
        !workspace
            .path()
            .join(".workflow-tools/mission/missions")
            .exists()
    );
}

#[test]
fn render_preview_returns_documents_without_mutating_store() {
    let workspace = TempDir::new().unwrap();
    let mission_id = Uuid::new_v4();
    let manifest = write_bundle(workspace.path(), 1, "preview");
    assert_eq!(
        execute_cli(
            workspace.path(),
            Operation::Import {
                mission_id,
                manifest_path: manifest,
                expected_current_revision: 0,
            }
        )
        .status,
        "ok"
    );
    let mission_root = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(mission_id.to_string());
    let snapshot = execute_mcp(workspace.path(), Operation::RenderPreview { mission_id });
    let preview = snapshot.rendered.unwrap();
    assert!(preview.roadmap.contains("Roadmap preview"));
    assert!(preview.parts.contains_key("waypoints/01-first.md"));
    assert!(!mission_root.join("generated").exists());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(mission_root.join("accepted.json")).unwrap()
        )
        .unwrap()["bundle"]["manifest"]["revision"],
        1
    );
}

#[test]
fn render_preview_missing_mission_does_not_create_store() {
    let workspace = TempDir::new().unwrap();
    let result = execute_cli(
        workspace.path(),
        Operation::RenderPreview {
            mission_id: Uuid::new_v4(),
        },
    );
    assert_eq!(result.status, "error");
    assert!(!workspace.path().join(".workflow-tools/mission").exists());
}

#[test]
fn cli_and_mcp_return_equivalent_diagnostics() {
    let workspace = TempDir::new().unwrap();
    let manifest = workspace.path().join("missing.json");
    let cli = execute_cli(
        workspace.path(),
        Operation::ValidatePreview {
            manifest_path: manifest.clone(),
        },
    );
    let mcp = execute_mcp(
        workspace.path(),
        Operation::ValidatePreview {
            manifest_path: manifest,
        },
    );

    assert_eq!(cli, mcp);
}
