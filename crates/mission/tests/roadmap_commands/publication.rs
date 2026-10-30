use super::*;

#[test]
fn publish_failure_preserves_previous_file_set() {
    let workspace = TempDir::new().unwrap();
    let mission_id = Uuid::new_v4();
    publish(
        workspace.path(),
        mission_id,
        write_bundle(workspace.path(), 1, "before"),
        0,
    );
    let generated = generated(workspace.path(), mission_id);
    let previous = files(&generated);
    fs::create_dir(
        workspace
            .path()
            .join(".workflow-tools/mission/missions")
            .join(format!(".{mission_id}.typed.tmp")),
    )
    .unwrap();

    let result = execute_cli(
        workspace.path(),
        Operation::Publish {
            mission_id,
            manifest_path: write_bundle(workspace.path(), 2, "after"),
            expected_current_revision: 1,
        },
    );

    assert_eq!(result.diagnostics[0].code, "publication-recovery-required");
    assert_eq!(files(&generated), previous);
    let accepted: serde_json::Value = serde_json::from_slice(
        &fs::read(
            workspace
                .path()
                .join(".workflow-tools/mission/missions")
                .join(mission_id.to_string())
                .join("accepted.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(accepted["bundle"]["manifest"]["revision"], 1);
    assert!(generated.join("ROADMAP.md").is_file());
    assert!(generated.join("waypoints/01-first.md").is_file());
    assert!(generated.join("waypoints/02-second.md").is_file());
}

#[test]
fn check_generated_detects_document_drift() {
    let workspace = TempDir::new().unwrap();
    let mission_id = Uuid::new_v4();
    publish(
        workspace.path(),
        mission_id,
        write_bundle(workspace.path(), 1, "published"),
        0,
    );
    let roadmap = generated(workspace.path(), mission_id).join("ROADMAP.md");
    fs::write(&roadmap, "manual edit").unwrap();

    let result = execute_mcp(workspace.path(), Operation::CheckGenerated { mission_id });

    assert_eq!(result.status, "drift");
    assert_eq!(result.diagnostics[0].code, "generated-changed");
    assert_eq!(fs::read_to_string(roadmap).unwrap(), "manual edit");
}
