use mission_api::*;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

fn bundle() -> MissionBundle {
    MissionBundle {
        manifest: BundleManifest {
            schema_version: 1,
            revision: 1,
            title: "Review entry".into(),
            objective: "Deliver a reviewable mission".into(),
            waypoint_files: vec![],
            execution_order: vec!["first".into()],
            artifacts: vec![
                "docs/evidence.md".into(),
                "ce://default/spec/example".into(),
            ],
            requirements: vec![],
            validation_gates: vec![],
            notes: vec![],
            operator_results: vec![],
        },
        waypoints: vec![WaypointFragment {
            waypoint_id: "first".into(),
            title: "First".into(),
            depends_on: vec![],
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Pending),
            ticket_ref: None,
            child_mission_ref: None,
            session_package: "first".into(),
            prompt: "Implement".into(),
            artifacts: vec![],
            requirement_ids: vec![],
            non_goals: vec![],
            validation: vec![ValidationGate {
                command: "true".into(),
                cwd: None,
            }],
            commit_checkpoint: CommitCheckpoint {
                message: "feat: first".into(),
            },
            part_markdown: "Evidence remains unknown".into(),
        }],
    }
}

#[test]
fn publication_includes_review_entry_with_empty_external_status() {
    let workspace = tempfile::tempdir().unwrap();
    let store = MissionStore::init(workspace.path()).unwrap();
    let id = Uuid::new_v4();
    store.publish(id, bundle(), 0).unwrap();
    let generated = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(id.to_string())
        .join("generated");
    assert!(
        generated.join("README.md").is_file(),
        "publication must include generated/README.md"
    );
}

fn published() -> (tempfile::TempDir, MissionStore, Uuid, PathBuf) {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir(workspace.path().join("docs")).unwrap();
    fs::write(workspace.path().join("docs/evidence.md"), "Evidence").unwrap();
    let store = MissionStore::init(workspace.path()).unwrap();
    let id = Uuid::new_v4();
    store.publish(id, bundle(), 0).unwrap();
    let generated = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(id.to_string())
        .join("generated");
    (workspace, store, id, generated)
}

fn projection(store: &MissionStore, id: Uuid) -> RenderedRoadmap {
    store
        .render_with_status_snapshot(id, &BTreeMap::new(), "ticket-status-v1:{}")
        .unwrap()
}

fn codes(rendered: &RenderedRoadmap, generated: &Path) -> Vec<&'static str> {
    rendered
        .check_generated(generated)
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn published_and_verified_inventory_match_with_empty_status_and_readme() {
    let (_workspace, store, id, generated) = published();
    let rendered = projection(&store, id);
    for (name, bytes) in rendered.files() {
        assert_eq!(fs::read(generated.join(name)).unwrap(), bytes);
    }
    let actual = fs::read_dir(&generated).unwrap().count();
    assert_eq!(actual, rendered.files().count());
    assert!(rendered.check_generated(&generated).unwrap().is_empty());
    let readme = String::from_utf8(rendered.readme).unwrap();
    assert!(readme.contains("External status: `{}`"));
    assert!(readme.contains("[W1](01-first.md)"));
    assert!(readme.contains("(../../../../../docs/evidence.md)"));
    assert!(readme.contains("(ce://default/spec/example)"));
    assert!(readme.contains("historical review acceptance are unknown"));
    assert!(readme.contains("What was verified?"));
}

#[test]
fn rendering_does_not_invent_external_status_or_acceptance() {
    let value = bundle();
    assert_eq!(
        render(&value, Uuid::nil()).unwrap(),
        render_with_snapshot(&value, Uuid::nil(), Some("ticket-status-v1:{}")).unwrap()
    );
    let readme = String::from_utf8(render(&value, Uuid::nil()).unwrap().readme).unwrap();
    assert!(readme.contains("not proof of acceptance"));
    assert!(readme.contains("missing results are unknown"));
}

#[test]
fn typed_publication_verifies_identically_and_detects_real_output_drift() {
    let workspace = tempfile::tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(workspace.path());
    let created = lifecycle
        .create_draft("Typed review".into(), "Review typed output".into())
        .unwrap();
    let mut draft = created.clone();
    draft.waypoints.push(TypedWaypoint {
        waypoint_id: "typed".into(),
        title: "Typed".into(),
        prompt: "Implement".into(),
        part: PartSource {
            waypoint_id: "typed".into(),
            document: Document {
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Text {
                        text: "Delivered facts are unknown".into(),
                    }],
                }],
            },
        },
        validation: vec![ValidationGate {
            command: "true".into(),
            cwd: None,
        }],
        commit_checkpoint: CommitCheckpoint {
            message: "feat: typed".into(),
        },
        ..Default::default()
    });
    lifecycle
        .publish_draft(created.mission_id, draft, 1)
        .unwrap();
    let store = MissionStore::open(workspace.path());
    let rendered = projection(&store, created.mission_id);
    let generated = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(created.mission_id.to_string())
        .join("generated");
    for (name, bytes) in rendered.files() {
        assert_eq!(fs::read(generated.join(name)).unwrap(), bytes);
    }
    assert!(rendered.check_generated(&generated).unwrap().is_empty());
    fs::write(generated.join("01-typed.md"), "real drift").unwrap();
    assert_eq!(codes(&rendered, &generated), vec!["generated-changed"]);
}

#[test]
fn references_are_workspace_relative_encoded_or_explicitly_unavailable() {
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut value = bundle();
    fs::create_dir(workspace.path().join("docs")).unwrap();
    fs::write(workspace.path().join("docs/a (b)#.md"), "Evidence").unwrap();
    value.manifest.artifacts.extend([
        workspace
            .path()
            .join("docs/a (b)#.md")
            .display()
            .to_string(),
        outside.path().join("private.md").display().to_string(),
        "../outside.md".into(),
        "https://example.test/unverified".into(),
        "C:\\outside.md".into(),
    ]);
    value.waypoints[0].artifacts.push("docs/a (b)#.md".into());
    value.manifest.operator_results.push(OperatorResult {
        waypoint_id: "first".into(),
        outcome: "passed".into(),
        operator_command: "check".into(),
        evidence_pointer: "ce://default/test/receipt".into(),
    });
    let store = MissionStore::init(workspace.path()).unwrap();
    let id = Uuid::new_v4();
    store.publish(id, value, 0).unwrap();
    let output = String::from_utf8(projection(&store, id).readme).unwrap();
    assert!(output.contains("(../../../../../docs/a%20%28b%29%23.md)"));
    assert!(output.contains("(ce://default/test/receipt)"));
    assert!(!output.contains("](/"));
    assert!(!output.contains("](https://"));
    assert!(!output.contains("](../../../../../../"));
    assert_eq!(output.matches(": unavailable").count(), 4);
}

#[test]
fn changed_missing_extra_and_dangling_are_distinct_and_read_only() {
    let (workspace, store, id, generated) = published();
    let rendered = projection(&store, id);
    let accepted_path = generated.parent().unwrap().join("accepted.json");
    let accepted = fs::read(&accepted_path).unwrap();
    fs::write(generated.join("ROADMAP.md"), "changed").unwrap();
    fs::remove_file(generated.join("01-first.md")).unwrap();
    fs::write(generated.join("extra.md"), "extra").unwrap();
    fs::remove_file(workspace.path().join("docs/evidence.md")).unwrap();
    let result = codes(&rendered, &generated);
    for code in [
        "generated-changed",
        "generated-missing",
        "generated-extra",
        "generated-dangling",
    ] {
        assert!(result.contains(&code), "{result:?} must include {code}");
    }
    assert_eq!(fs::read(&accepted_path).unwrap(), accepted);
    assert_eq!(fs::read(generated.join("ROADMAP.md")).unwrap(), b"changed");
}

#[test]
fn missing_readme_and_directory_instead_of_part_are_distinct() {
    let (_workspace, store, id, generated) = published();
    fs::remove_file(generated.join("README.md")).unwrap();
    fs::remove_file(generated.join("01-first.md")).unwrap();
    fs::create_dir(generated.join("01-first.md")).unwrap();
    let result = codes(&projection(&store, id), &generated);
    assert!(result.contains(&"generated-missing"));
    assert!(result.contains(&"generated-destination-invalid"));
}

#[cfg(unix)]
#[test]
fn broken_and_redirected_symlinks_are_not_valid_generated_files() {
    use std::os::unix::fs::symlink;
    let (_workspace, store, id, generated) = published();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("roadmap.md"), "external").unwrap();
    fs::remove_file(generated.join("ROADMAP.md")).unwrap();
    fs::remove_file(generated.join("01-first.md")).unwrap();
    symlink(
        outside.path().join("roadmap.md"),
        generated.join("ROADMAP.md"),
    )
    .unwrap();
    symlink(
        outside.path().join("missing.md"),
        generated.join("01-first.md"),
    )
    .unwrap();
    let result = codes(&projection(&store, id), &generated);
    assert!(result.contains(&"generated-destination-invalid"));
    assert!(result.contains(&"generated-dangling"));
}

#[cfg(unix)]
#[test]
fn symlink_escape_is_unavailable_at_render_and_invalid_if_destination_changes() {
    use std::os::unix::fs::symlink;
    let (workspace, store, id, generated) = published();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("evidence.md"), "private").unwrap();
    fs::remove_file(workspace.path().join("docs/evidence.md")).unwrap();
    symlink(
        outside.path().join("evidence.md"),
        workspace.path().join("docs/evidence.md"),
    )
    .unwrap();
    // The old bytes retain the previously valid link; verification must reject the new target.
    let previous = render(&bundle(), id).unwrap();
    assert!(codes(&previous, &generated).contains(&"generated-destination-invalid"));
    let refreshed = projection(&store, id);
    assert!(codes(&refreshed, &generated).contains(&"generated-destination-invalid"));
    let current = String::from_utf8(refreshed.readme).unwrap();
    assert!(current.contains("docs/evidence.md: unavailable"));
}
