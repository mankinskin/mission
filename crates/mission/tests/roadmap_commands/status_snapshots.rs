use super::*;

#[test]
fn ticket_status_snapshot_drives_rendering_and_check_generated_without_writes() {
    let workspace = TempDir::new().unwrap();
    let ticket_id = Uuid::new_v4();
    let tickets = TicketStore::init(workspace.path()).unwrap();
    tickets
        .create(
            Some(ticket_id),
            "tracker-improvement",
            Some("Externally owned mission status"),
            Some("planning"),
            BTreeMap::new(),
            None,
            Some("Track the status snapshot fixture."),
        )
        .unwrap();
    let mission_id = Uuid::new_v4();
    let (manifest, reference) =
        write_ticket_bundle(workspace.path(), 1, "ticket-backed", ticket_id);
    assert!(
        !fs::read_to_string(manifest.parent().unwrap().join("first.json"))
            .unwrap()
            .contains("\"state\"")
    );

    let published = execute_cli(
        workspace.path(),
        Operation::Publish {
            mission_id,
            manifest_path: manifest,
            expected_current_revision: 0,
        },
    );
    assert_eq!(published.status, "ok", "{:?}", published.diagnostics);
    assert_eq!(
        published.mission_state,
        Some(mission::WaypointState::Pending)
    );
    assert_eq!(
        published
            .external_status_snapshot
            .get(&reference)
            .map(String::as_str),
        Some("planning")
    );
    let previous_files = files(&generated(workspace.path(), mission_id));
    let roadmap = String::from_utf8(previous_files["ROADMAP.md"].clone()).unwrap();
    assert!(roadmap.contains("external-status-snapshot: ticket-status-v1:"));

    let preview = execute_mcp(workspace.path(), Operation::RenderPreview { mission_id });
    assert_eq!(
        preview.external_status_snapshot,
        published.external_status_snapshot
    );
    assert_eq!(preview.mission_state, Some(mission::WaypointState::Pending));
    assert_eq!(
        preview.rendered.unwrap().roadmap.as_bytes(),
        previous_files["ROADMAP.md"]
    );
    assert_eq!(
        execute_cli(workspace.path(), Operation::CheckGenerated { mission_id }).status,
        "ok"
    );

    tickets
        .update(
            &ticket_id,
            BTreeMap::new(),
            None,
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap();
    let changed = execute_mcp(workspace.path(), Operation::CheckGenerated { mission_id });
    assert_eq!(changed.status, "drift");
    assert_eq!(
        changed.mission_state,
        Some(mission::WaypointState::InProgress)
    );
    assert_eq!(
        changed
            .external_status_snapshot
            .get(&reference)
            .map(String::as_str),
        Some("in-implementation")
    );
    assert_eq!(
        files(&generated(workspace.path(), mission_id)),
        previous_files
    );
}

#[test]
fn unresolved_ticket_status_preserves_accepted_mission_and_generated_files() {
    let workspace = TempDir::new().unwrap();
    let mission_id = Uuid::new_v4();
    publish(
        workspace.path(),
        mission_id,
        write_bundle(workspace.path(), 1, "accepted-before-unresolved-status"),
        0,
    );
    let generated_before = files(&generated(workspace.path(), mission_id));
    let accepted_path = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(mission_id.to_string())
        .join("accepted.json");
    let accepted_before = fs::read(&accepted_path).unwrap();
    let (manifest, _) = write_ticket_bundle(workspace.path(), 2, "missing-ticket", Uuid::new_v4());

    let result = execute_cli(
        workspace.path(),
        Operation::Publish {
            mission_id,
            manifest_path: manifest,
            expected_current_revision: 1,
        },
    );

    assert_eq!(result.status, "error");
    assert_eq!(result.diagnostics[0].code, "external-status-unresolved");
    assert_eq!(fs::read(&accepted_path).unwrap(), accepted_before);
    assert_eq!(
        files(&generated(workspace.path(), mission_id)),
        generated_before
    );
}

#[test]
fn parent_publication_collects_ticket_status_from_nested_child_missions() {
    let workspace = TempDir::new().unwrap();
    let ticket_id = Uuid::new_v4();
    let tickets = TicketStore::init(workspace.path()).unwrap();
    tickets
        .create(
            Some(ticket_id),
            "tracker-improvement",
            Some("Nested ticket status"),
            Some("planning"),
            BTreeMap::new(),
            None,
            Some("Track the nested status fixture."),
        )
        .unwrap();

    let child_id = Uuid::new_v4();
    let (child_manifest, ticket_ref) =
        write_ticket_bundle(workspace.path(), 1, "nested-child", ticket_id);
    let child_import = execute_cli(
        workspace.path(),
        Operation::Import {
            mission_id: child_id,
            manifest_path: child_manifest,
            expected_current_revision: 0,
        },
    );
    assert_eq!(child_import.status, "ok", "{:?}", child_import.diagnostics);
    assert_eq!(
        child_import
            .external_status_snapshot
            .get(&ticket_ref)
            .map(String::as_str),
        Some("planning")
    );
    assert_eq!(
        child_import.mission_state,
        Some(mission::WaypointState::Pending)
    );

    let parent_id = Uuid::new_v4();
    let parent_manifest = write_child_bundle(workspace.path(), 1, "nested-parent", child_id);
    let published = execute_mcp(
        workspace.path(),
        Operation::Publish {
            mission_id: parent_id,
            manifest_path: parent_manifest,
            expected_current_revision: 0,
        },
    );

    assert_eq!(published.status, "ok", "{:?}", published.diagnostics);
    assert_eq!(
        published
            .external_status_snapshot
            .get(&ticket_ref)
            .map(String::as_str),
        Some("planning")
    );
    assert_eq!(
        published.mission_state,
        Some(mission::WaypointState::Pending)
    );
    let generated_roadmap =
        fs::read_to_string(generated(workspace.path(), parent_id).join("ROADMAP.md")).unwrap();
    assert!(generated_roadmap.contains("| **State** | pending |"));
    assert!(generated_roadmap.contains(&format!("ce://default/mission/{child_id}")));
}
