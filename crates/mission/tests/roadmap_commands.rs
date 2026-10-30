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
    fn visit(root: &Path, path: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), files);
            } else {
                files.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(path, path, &mut result);
    result
}

#[path = "roadmap_commands/direct_authoring.rs"]
mod direct_authoring;
#[path = "roadmap_commands/publication.rs"]
mod publication;
#[path = "roadmap_commands/revision.rs"]
mod revision;
#[path = "roadmap_commands/status_snapshots.rs"]
mod status_snapshots;
