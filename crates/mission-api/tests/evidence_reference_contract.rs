use mission_api::{
    Block, CommitCheckpoint, Document, Inline, MissionArtifactProvenance, MissionError,
    MissionFileFormat, MissionFileRole, MissionLifecycle, MissionStore, PartSource, TypedWaypoint,
    ValidationGate, resolve_artifact_reference,
};
use std::{fs, path::Path};
use tempfile::TempDir;
use uuid::Uuid;

fn typed_waypoint() -> TypedWaypoint {
    TypedWaypoint {
        waypoint_id: "first".into(),
        title: "First".into(),
        prompt: "Implement the first step".into(),
        part: PartSource {
            waypoint_id: "first".into(),
            document: Document {
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Text {
                        text: "Authored source".into(),
                    }],
                }],
            },
        },
        validation: vec![ValidationGate {
            command: "true".into(),
            cwd: None,
        }],
        commit_checkpoint: CommitCheckpoint {
            message: "feat: first".into(),
        },
        ..Default::default()
    }
}

#[test]
fn publication_classifies_parts_beyond_two_digit_labels() {
    let workspace = TempDir::new().unwrap();
    let lifecycle = MissionLifecycle::open(workspace.path());
    let mut draft = lifecycle
        .create_draft("Large mission".into(), "Keep contiguous labels".into())
        .unwrap();
    for index in 1..=101 {
        let mut waypoint = typed_waypoint();
        waypoint.waypoint_id = format!("step-{index}");
        waypoint.part.waypoint_id = waypoint.waypoint_id.clone();
        draft.waypoints.push(waypoint);
    }
    let published = lifecycle.publish_draft(draft.mission_id, draft, 1).unwrap();
    let record = provenance(workspace.path(), published.mission_id, published.revision);
    assert!(record.validate().is_ok());
    for name in [
        "generated/99-step-99.md",
        "generated/100-step-100.md",
        "generated/101-step-101.md",
    ] {
        assert!(
            record
                .files
                .iter()
                .any(|file| file.path == name && file.role == MissionFileRole::Generated)
        );
        assert!(
            workspace
                .path()
                .join(".workflow-tools/mission/missions")
                .join(published.mission_id.to_string())
                .join(name)
                .is_file()
        );
    }
}

fn provenance(workspace: &Path, mission_id: Uuid, revision: u64) -> MissionArtifactProvenance {
    MissionStore::open(workspace)
        .get_artifact_provenance(mission_id, revision)
        .unwrap()
}

#[test]
fn each_new_output_has_validated_provenance_role_format_and_schema() {
    let workspace = TempDir::new().unwrap();
    let lifecycle = MissionLifecycle::open(workspace.path());
    let draft = lifecycle
        .create_draft("Provenance".into(), "Describe the output".into())
        .unwrap();
    let mut draft = draft;
    draft.waypoints.push(typed_waypoint());
    let saved = lifecycle.save_draft(draft.mission_id, draft, 1).unwrap();
    let first = provenance(workspace.path(), saved.mission_id, saved.revision);

    assert_eq!(first.producer, "workflow-tools/mission-api");
    assert!(!first.purpose.trim().is_empty());
    assert!(!first.invocation.trim().is_empty());
    assert!(first.inputs.iter().any(|input| input == "source.json"));
    assert_eq!(
        first.grouping,
        format!("mission:{}/revision:2", saved.mission_id)
    );
    assert!(first.validate().is_ok());

    let source = first
        .files
        .iter()
        .find(|file| file.path == "source.json")
        .unwrap();
    assert_eq!(source.role, MissionFileRole::Authored);
    assert_eq!(source.format, MissionFileFormat::Json);
    assert!(!source.schema_guarantees.is_empty());
    let part = first
        .files
        .iter()
        .find(|file| file.path == "parts/first.json")
        .unwrap();
    assert_eq!(part.role, MissionFileRole::Authored);
    let accepted = first
        .files
        .iter()
        .find(|file| file.path == "accepted.json")
        .unwrap();
    assert_eq!(accepted.role, MissionFileRole::Generated);
    assert_eq!(accepted.format, MissionFileFormat::Json);
    assert_eq!(
        first.outputs.len(),
        first.files.len(),
        "every output path is listed once"
    );
    assert_eq!(
        first
            .outputs
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        first.files.iter().map(|file| file.path.clone()).collect()
    );
    let mut unknown = first.clone();
    unknown.files[0].path = "unregistered.bin".into();
    unknown.outputs[0] = "unregistered.bin".into();
    assert!(
        unknown
            .validate()
            .unwrap_err()
            .iter()
            .any(|diagnostic| { diagnostic.code == "mission-artifact-unknown-file" })
    );
    let mut unsupported = first.clone();
    unsupported.files[0].format = MissionFileFormat::Markdown;
    assert!(
        unsupported
            .validate()
            .unwrap_err()
            .iter()
            .any(|diagnostic| { diagnostic.code == "mission-artifact-format-unsupported" })
    );

    let published = lifecycle.publish_draft(saved.mission_id, saved, 2).unwrap();
    let second = provenance(workspace.path(), published.mission_id, published.revision);
    assert!(second.validate().is_ok());
    assert!(second.outputs.iter().any(|output| output == "mission.md"));
    assert!(
        second
            .outputs
            .iter()
            .any(|output| output == "generated/README.md")
    );
    assert!(
        second
            .outputs
            .iter()
            .any(|output| output == "generated/ROADMAP.md")
    );
    assert!(
        second
            .files
            .iter()
            .all(|file| !file.schema_guarantees.is_empty())
    );
}

#[test]
fn emitted_file_references_resolve_from_each_supported_consumer() {
    let workspace = TempDir::new().unwrap();
    fs::create_dir(workspace.path().join("docs")).unwrap();
    fs::write(workspace.path().join("docs/evidence.md"), "evidence").unwrap();
    let generated = workspace
        .path()
        .join(".workflow-tools/mission/missions/example/generated");
    fs::create_dir_all(&generated).unwrap();
    let readme = generated.join("README.md");
    let roadmap = generated.join("ROADMAP.md");
    for consumer in [&readme, &roadmap] {
        let link =
            resolve_artifact_reference(workspace.path(), consumer, "docs/evidence.md").unwrap();
        assert_eq!(
            fs::canonicalize(consumer.parent().unwrap().join(link)).unwrap(),
            fs::canonicalize(workspace.path().join("docs/evidence.md")).unwrap()
        );
    }
    assert_eq!(
        resolve_artifact_reference(workspace.path(), &readme, "ce://default/spec/example").unwrap(),
        "ce://default/spec/example"
    );
}

#[cfg(unix)]
#[test]
fn symlink_escape_is_rejected_by_reference_resolution() {
    use std::os::unix::fs::symlink;
    let workspace = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("secret.md"), "private").unwrap();
    fs::create_dir(workspace.path().join("docs")).unwrap();
    symlink(
        outside.path().join("secret.md"),
        workspace.path().join("docs/secret.md"),
    )
    .unwrap();
    let error = resolve_artifact_reference(
        workspace.path(),
        Path::new("generated/README.md"),
        "docs/secret.md",
    )
    .unwrap_err();
    assert_eq!(error.code, "mission-reference-escaping");
}

#[test]
fn missing_wrong_root_escaping_and_unknown_references_fail() {
    let workspace = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("outside.md"), "outside").unwrap();
    let consumer = workspace.path().join("consumer/README.md");

    let missing =
        resolve_artifact_reference(workspace.path(), &consumer, "missing.md").unwrap_err();
    assert_eq!(missing.code, "mission-reference-missing");
    let escaping =
        resolve_artifact_reference(workspace.path(), &consumer, "../outside.md").unwrap_err();
    assert_eq!(escaping.code, "mission-reference-escaping");
    let wrong_root = resolve_artifact_reference(
        workspace.path(),
        &consumer,
        &outside.path().join("outside.md").display().to_string(),
    )
    .unwrap_err();
    assert_eq!(wrong_root.code, "mission-reference-wrong-root");
    let unknown = resolve_artifact_reference(workspace.path(), &consumer, "https://example.test/a")
        .unwrap_err();
    assert_eq!(unknown.code, "mission-reference-unknown");
    let unknown_store =
        resolve_artifact_reference(workspace.path(), &consumer, "ce://default/unknown/id")
            .unwrap_err();
    assert_eq!(unknown_store.code, "mission-reference-unknown");
}

#[test]
fn rejected_reference_does_not_write_source_or_new_accepted_revision() {
    let workspace = TempDir::new().unwrap();
    let lifecycle = MissionLifecycle::open(workspace.path());
    let draft = lifecycle
        .create_draft("Atomic references".into(), "Keep source intact".into())
        .unwrap();
    let mut draft = draft;
    draft.waypoints.push(typed_waypoint());
    let saved = lifecycle.save_draft(draft.mission_id, draft, 1).unwrap();
    let directory = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(saved.mission_id.to_string());
    let source_before = fs::read(directory.join("source.json")).unwrap();
    let accepted_before = fs::read(directory.join("accepted.json")).unwrap();
    let mut invalid = lifecycle.get(saved.mission_id).unwrap().bundle;
    invalid.manifest.revision = saved.revision + 1;
    invalid
        .manifest
        .artifacts
        .push("missing/evidence.md".into());

    let error = MissionStore::open(workspace.path())
        .publish(saved.mission_id, invalid, saved.revision)
        .unwrap_err();
    let MissionError::Validation(_, diagnostics) = error else {
        panic!("invalid references must be validation errors");
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "mission-reference-missing")
    );
    assert_eq!(
        fs::read(directory.join("source.json")).unwrap(),
        source_before
    );
    assert_eq!(
        fs::read(directory.join("accepted.json")).unwrap(),
        accepted_before
    );
    assert!(
        !directory
            .join(format!("history/revision-{}.json", saved.revision + 1))
            .exists()
    );
}

#[test]
fn historical_provenance_is_immutable_and_missing_metadata_is_not_backfilled() {
    let workspace = TempDir::new().unwrap();
    let lifecycle = MissionLifecycle::open(workspace.path());
    let draft = lifecycle
        .create_draft("History".into(), "Preserve evidence".into())
        .unwrap();
    let mut draft = draft;
    draft.waypoints.push(typed_waypoint());
    let saved = lifecycle.save_draft(draft.mission_id, draft, 1).unwrap();
    let directory = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(saved.mission_id.to_string());
    let old_provenance_path = directory.join("history/provenance-revision-2.json");
    let old_provenance = fs::read(&old_provenance_path).unwrap();
    lifecycle
        .save_draft(saved.mission_id, saved.clone(), 2)
        .unwrap();
    assert_eq!(fs::read(&old_provenance_path).unwrap(), old_provenance);

    let accepted_path = directory.join("accepted.json");
    let accepted = fs::read(&accepted_path).unwrap();
    fs::remove_file(directory.join("history/provenance-revision-1.json")).unwrap();
    assert!(matches!(
        MissionStore::open(workspace.path()).get_artifact_provenance(saved.mission_id, 1),
        Err(MissionError::NotFound(_))
    ));
    assert_eq!(fs::read(&accepted_path).unwrap(), accepted);
    assert!(
        !directory
            .join("history/provenance-revision-1.json")
            .exists()
    );
}
