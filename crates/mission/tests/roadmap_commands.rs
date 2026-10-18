use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use mission::{Operation, execute_cli, execute_mcp};
use serde_json::json;
use tempfile::TempDir;
use ticket_api::storage::TicketStore;
use uuid::Uuid;

fn write_bundle(root: &Path, revision: u64, suffix: &str) -> PathBuf {
    let bundle = root.join(format!("bundle-{revision}"));
    fs::create_dir_all(&bundle).unwrap();
    fs::write(
        bundle.join("first.json"),
        serde_json::to_vec_pretty(&json!({
            "waypoint_id": "first",
            "title": format!("First {suffix}"),
            "mode": "execution-side-effect",
            "scope": { "kind": "single-session" },
            "state": "pending",
            "session_package": "first",
            "prompt": "implement first",
            "validation": [{ "command": "cargo test" }],
            "commit_checkpoint": { "message": "first checkpoint" },
            "part_markdown": "First generated Part."
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        bundle.join("second.json"),
        serde_json::to_vec_pretty(&json!({
            "waypoint_id": "second",
            "title": format!("Second {suffix}"),
            "depends_on": ["first"],
            "mode": "execution-side-effect",
            "scope": { "kind": "single-session" },
            "state": "pending",
            "session_package": "second",
            "prompt": "implement second",
            "validation": [{ "command": "cargo test" }],
            "commit_checkpoint": { "message": "second checkpoint" },
            "part_markdown": "Second generated Part."
        }))
        .unwrap(),
    )
    .unwrap();
    let manifest = bundle.join("manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "revision": revision,
            "title": format!("Roadmap {suffix}"),
            "objective": "Preserve generated roadmaps.",
            "waypoint_files": ["first.json", "second.json"],
            "execution_order": ["first", "second"]
        }))
        .unwrap(),
    )
    .unwrap();
    manifest
}

fn write_ticket_bundle(
    root: &Path,
    revision: u64,
    suffix: &str,
    ticket_id: Uuid,
) -> (PathBuf, String) {
    let manifest = write_bundle(root, revision, suffix);
    let reference = format!("ce://default/ticket/{ticket_id}");
    let first_fragment = manifest.parent().unwrap().join("first.json");
    let mut fragment: serde_json::Value =
        serde_json::from_slice(&fs::read(&first_fragment).unwrap()).unwrap();
    fragment["scope"] = json!({ "kind": "ticket-backed", "ticket_ref": reference });
    fragment.as_object_mut().unwrap().remove("state");
    fs::write(
        &first_fragment,
        serde_json::to_vec_pretty(&fragment).unwrap(),
    )
    .unwrap();
    (manifest, reference)
}

fn write_child_bundle(root: &Path, revision: u64, suffix: &str, child_id: Uuid) -> PathBuf {
    let manifest = write_bundle(root, revision, suffix);
    let first_fragment = manifest.parent().unwrap().join("first.json");
    let mut fragment: serde_json::Value =
        serde_json::from_slice(&fs::read(&first_fragment).unwrap()).unwrap();
    fragment["child_mission_ref"] = json!(format!("ce://default/mission/{child_id}"));
    fragment.as_object_mut().unwrap().remove("state");
    fs::write(
        &first_fragment,
        serde_json::to_vec_pretty(&fragment).unwrap(),
    )
    .unwrap();
    manifest
}

fn generated(root: &Path, mission_id: Uuid) -> PathBuf {
    root.join(".workflow-tools/mission/missions")
        .join(mission_id.to_string())
        .join("generated")
}

fn publish(root: &Path, mission_id: Uuid, manifest: PathBuf, expected: u64) {
    assert_eq!(
        execute_cli(
            root,
            Operation::Publish {
                mission_id,
                manifest_path: manifest,
                expected_current_revision: expected,
            }
        )
        .status,
        "ok"
    );
}

fn files(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().to_string(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
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
    assert!(preview.parts.contains_key("01-first.md"));
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
            .join(format!(".{mission_id}.publish.tmp")),
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
    assert!(generated.join("01-first.md").is_file());
    assert!(generated.join("02-second.md").is_file());
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
    assert_eq!(result.diagnostics[0].code, "generated-drift");
    assert_eq!(fs::read_to_string(roadmap).unwrap(), "manual edit");
}

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
