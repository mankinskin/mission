use super::*;

#[test]
fn stale_revision_import_is_rejected() {
    let workspace = TempDir::new().unwrap();
    let mission_id = Uuid::new_v4();
    assert_eq!(
        execute_cli(
            workspace.path(),
            Operation::Import {
                mission_id,
                manifest_path: write_bundle(workspace.path(), 1, "accepted"),
                expected_current_revision: 0,
            }
        )
        .status,
        "ok"
    );

    let result = execute_mcp(
        workspace.path(),
        Operation::Import {
            mission_id,
            manifest_path: write_bundle(workspace.path(), 2, "stale"),
            expected_current_revision: 0,
        },
    );

    assert_eq!(result.diagnostics[0].code, "stale-revision");
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
}
