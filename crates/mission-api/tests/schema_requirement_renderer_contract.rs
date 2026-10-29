use mission_api::*;
use serde_json::json;
use spec_api::{AcceptanceCriterion, CriterionArtifact, SpecManifest};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use ticket_api::model::ticket::TicketManifest;
use uuid::Uuid;

fn fixture() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(".w3-fixture-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap()
}

fn mission(workspace: &Path) -> AcceptedMission {
    MissionLifecycle::open(workspace)
        .create(
            "Mission".into(),
            "Objective".into(),
            vec![DirectWaypoint {
                waypoint_id: "verify".into(),
                title: "Verify".into(),
                depends_on: vec![],
                prompt: "Check".into(),
                validation: vec![ValidationGate {
                    command: "cargo test".into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: "Verify".into(),
                },
                non_goals: vec![],
            }],
        )
        .unwrap()
}

fn spec(workspace: &Path, v2: bool) -> (SpecManifest, CriterionReference) {
    let mut spec = SpecManifest::new("mission/contract", "Canonical specification", "mission");
    if v2 {
        spec.extra.insert("format_version".into(), json!(2));
        spec.extra.insert(
            "criteria".into(),
            json!([CriterionArtifact {
                criterion_id: "schema-check".into(),
                owner_component_id: "mission".into(),
                statement: "Authoritative <criterion> | [text]".into(),
                measurement: None,
                evidence_ids: vec![],
            }]),
        );
    } else {
        spec.extra.insert(
            "acceptance_criteria".into(),
            json!([AcceptanceCriterion {
                id: "schema-check".into(),
                statement: "Authoritative <criterion> | [text]".into(),
                expected_property_ids: vec![],
                required_evidence_ids: vec![],
            }]),
        );
    }
    write_spec(workspace, &spec);
    let reference = CriterionReference {
        specification_id: spec.id,
        criterion_id: CriterionId::new("schema-check").unwrap(),
    };
    (spec, reference)
}

fn spec_path(workspace: &Path, id: Uuid) -> PathBuf {
    workspace
        .join(".workflow-tools/spec/specs")
        .join(id.to_string())
        .join("spec.toml")
}

fn write_spec(workspace: &Path, spec: &SpecManifest) {
    let path = spec_path(workspace, spec.id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, toml::to_string(spec).unwrap()).unwrap();
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, values: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, values);
            } else {
                values.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut values = BTreeMap::new();
    walk(root, root, &mut values);
    values
}

fn requirement(reference: CriterionReference) -> Requirement {
    Requirement {
        id: "mission-contract".into(),
        text: "Canonical reference, not copied criteria".into(),
        deferred: false,
        canonical_criteria: vec![reference],
    }
}

#[test]
fn requirement_accepts_typed_canonical_criterion_references() {
    let value = json!({
        "id": "mission-contract",
        "text": "Use the canonical criterion",
        "canonical_criteria": [{
            "specification_id": Uuid::new_v4(),
            "criterion_id": "mission-schema"
        }]
    });
    assert!(
        serde_json::from_value::<Requirement>(value).is_ok(),
        "Requirements must accept typed canonical specification/criterion IDs"
    );
}

#[test]
fn shared_projection_enumerates_only_proven_fields_from_actual_models() {
    let workspace = fixture();
    let accepted = mission(workspace.path());
    let ticket: TicketManifest = serde_json::from_value(json!({
        "id": accepted.mission_id,
        "created_at": "2026-10-29T00:00:00Z",
        "title": accepted.bundle.manifest.title,
        "state": "in-review",
        "plan_revision": 7,
        "foreign_attribute": {"retain": true}
    }))
    .unwrap();
    let before = serde_json::to_value(&ticket).unwrap();
    let projected_ticket = SharedEntity::from_ticket(&ticket).unwrap();
    let projected_mission = SharedEntity::from_mission(&accepted).unwrap();
    assert_eq!(projected_ticket.id, projected_mission.id);
    assert_eq!(projected_ticket.title, projected_mission.title);
    assert_eq!(
        projected_ticket.source_schema,
        SharedSourceSchema::TicketUnversioned
    );
    assert_eq!(
        projected_mission.source_schema,
        SharedSourceSchema::MissionBundleV1
    );
    let mut keys = serde_json::to_value(projected_ticket)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(keys, ["id", "schema_version", "source_schema", "title"]);
    assert_eq!(serde_json::to_value(ticket).unwrap(), before);
}

#[test]
fn shared_projection_strictly_rejects_bad_values_and_preserves_exact_source_bytes() {
    let good = json!({
        "schema_version": 1, "source_schema": "mission-bundle-v1",
        "id": Uuid::new_v4(), "title": "Title"
    });
    for (field, invalid) in [
        ("schema_version", json!(99)),
        ("schema_version", json!(0)),
        ("schema_version", json!("1")),
        ("schema_version", json!(-1)),
        ("source_schema", json!("mission-source-v99")),
        ("title", json!(" \n")),
        ("title", json!(false)),
        ("id", json!("not-a-uuid")),
        ("id", json!(Uuid::nil())),
        ("unknown", json!({"future": "must survive"})),
    ] {
        let mut value = good.clone();
        value[field] = invalid;
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        let failure = SharedEntity::from_json(&bytes).unwrap_err();
        assert_eq!(failure.source_bytes, bytes, "{field}");
        assert!(failure.diagnostic.code.starts_with("mission-shared-schema"));
    }
    for bytes in [b"{not json".as_slice(), b"{}".as_slice()] {
        assert_eq!(
            SharedEntity::from_json(bytes).unwrap_err().source_bytes,
            bytes
        );
    }
    assert!(SharedEntity::from_json(&serde_json::to_vec(&good).unwrap()).is_ok());
}

#[test]
fn typed_mission_and_native_versions_are_not_coerced() {
    let workspace = fixture();
    let accepted = mission(workspace.path());
    let mut typed = MissionLifecycle::open(workspace.path())
        .create_draft("Typed".into(), "Objective".into())
        .unwrap();
    assert_eq!(
        SharedEntity::from_typed_mission(&typed)
            .unwrap()
            .source_schema,
        SharedSourceSchema::MissionSourceV2
    );
    typed.schema_version = 1;
    assert!(SharedEntity::from_typed_mission(&typed).is_err());
    let mut unsupported = accepted;
    unsupported.bundle.manifest.schema_version = 2;
    assert!(SharedEntity::from_mission(&unsupported).is_err());
}

#[test]
fn canonical_references_resolve_legacy_and_v2_spec_owned_criteria_without_writes() {
    let workspace = fixture();
    for version in [false, true] {
        let (_, reference) = spec(workspace.path(), version);
        let before = snapshot(workspace.path());
        let criterion = reference.resolve(workspace.path()).unwrap();
        let rendered = render_criterion_entity(&reference, &criterion).unwrap();
        assert!(rendered.contains(&reference.specification_uri()));
        assert!(rendered.contains("Authoritative &lt;criterion&gt; \\| \\[text\\]"));
        assert_eq!(snapshot(workspace.path()), before);
    }
}

#[test]
fn dangling_wrong_spec_and_wrong_root_references_fail_without_fallback() {
    let workspace = fixture();
    let elsewhere = fixture();
    let (_, mut reference) = spec(elsewhere.path(), true);
    assert!(reference.resolve(workspace.path()).is_err());
    let (_, valid) = spec(workspace.path(), true);
    reference.specification_id = valid.specification_id;
    reference.criterion_id = CriterionId::new("other-criterion").unwrap();
    assert!(reference.resolve(workspace.path()).is_err());
    reference.specification_id = Uuid::new_v4();
    assert!(reference.resolve(workspace.path()).is_err());
    let (_, other_spec) = spec(workspace.path(), true);
    let resolved = valid.resolve(workspace.path()).unwrap();
    // Same criterion ID in another Spec is not the same criterion.
    assert!(render_criterion_entity(&other_spec, &resolved).is_err());
}

#[test]
fn malformed_canonical_spec_identity_version_and_duplicate_criteria_fail() {
    let workspace = fixture();
    let (original, reference) = spec(workspace.path(), true);
    let path = spec_path(workspace.path(), original.id);
    for invalid in [
        json!({"format_version": 99}),
        json!({"format_version": "2"}),
        json!({"criteria": [{"criterion_id": "schema-check"}]}),
        json!({"criteria": "not-an-array"}),
    ] {
        let mut changed = original.clone();
        for (key, value) in invalid.as_object().unwrap() {
            changed.extra.insert(key.clone(), value.clone());
        }
        write_spec(workspace.path(), &changed);
        assert!(reference.resolve(workspace.path()).is_err());
    }
    let mut changed = original.clone();
    changed.id = Uuid::new_v4();
    fs::write(&path, toml::to_string(&changed).unwrap()).unwrap();
    assert!(reference.resolve(workspace.path()).is_err());
    let mut changed = original;
    let criterion = changed.extra["criteria"][0].clone();
    changed
        .extra
        .insert("criteria".into(), json!([criterion, criterion]));
    write_spec(workspace.path(), &changed);
    assert!(reference.resolve(workspace.path()).is_err());
}

#[test]
fn invalid_requirement_ids_references_and_duplicates_are_rejected_at_write() {
    let workspace = fixture();
    let accepted = mission(workspace.path());
    let (_, reference) = spec(workspace.path(), true);
    let store = MissionStore::open(workspace.path());
    let before = snapshot(workspace.path());
    for invalid in [
        json!({"id": "not stable!"}),
        json!({"canonical_criteria": [{"specification_id": Uuid::nil(), "criterion_id": "schema-check"}]}),
        json!({"canonical_criteria": [{"specification_id": reference.specification_id, "criterion_id": ""}]}),
        json!({"canonical_criteria": [reference, reference]}),
    ] {
        let mut value = serde_json::to_value(requirement(reference.clone())).unwrap();
        for (key, field) in invalid.as_object().unwrap() {
            value[key] = field.clone();
        }
        let mut changed = accepted.bundle.clone();
        changed.manifest.revision += 1;
        changed.manifest.requirements = vec![serde_json::from_value(value).unwrap()];
        changed.waypoints[0].requirement_ids = vec!["mission-contract".into()];
        assert!(store.import(accepted.mission_id, changed, 1).is_err());
        assert_eq!(snapshot(workspace.path()), before);
    }
}

#[test]
fn canonical_reference_roundtrips_and_invalid_update_preserves_all_authority() {
    let workspace = fixture();
    let accepted = mission(workspace.path());
    let (_, reference) = spec(workspace.path(), true);
    let mut bundle = accepted.bundle.clone();
    bundle.manifest.revision += 1;
    bundle.manifest.requirements = vec![requirement(reference.clone())];
    bundle.waypoints[0].requirement_ids = vec!["mission-contract".into()];
    let store = MissionStore::open(workspace.path());
    let updated = store.publish(accepted.mission_id, bundle, 1).unwrap();
    assert_eq!(
        updated.bundle.manifest.requirements[0].canonical_criteria,
        [reference]
    );
    let before = snapshot(workspace.path());
    let mut invalid = updated.bundle.clone();
    invalid.manifest.revision += 1;
    invalid.manifest.requirements[0].canonical_criteria[0].specification_id = Uuid::new_v4();
    assert!(
        store
            .publish(updated.mission_id, invalid.clone(), 2)
            .is_err()
    );
    assert_eq!(snapshot(workspace.path()), before);
    assert!(store.import(updated.mission_id, invalid, 2).is_err());
    assert_eq!(snapshot(workspace.path()), before);
}

#[test]
fn deterministic_entity_composition_reuses_native_requirement_rows_and_escapes_text() {
    let workspace = fixture();
    let mut accepted = mission(workspace.path());
    let (mut specification, reference) = spec(workspace.path(), true);
    let payload = "<script>alert(1)</script> | [link](javascript:bad)\n# **markup** `code`";
    accepted.bundle.manifest.title = payload.into();
    accepted.bundle.manifest.objective = payload.into();
    accepted.bundle.manifest.requirements = vec![requirement(reference.clone())];
    accepted.bundle.manifest.requirements[0].text = payload.into();
    accepted.bundle.waypoints[0].requirement_ids = vec!["mission-contract".into()];
    let ticket: TicketManifest = serde_json::from_value(json!({
        "id": Uuid::new_v4(), "created_at": "2026-10-29T00:00:00Z", "title": payload
    }))
    .unwrap();
    specification.extra.insert("title".into(), json!(payload));
    let resolved = reference.resolve(workspace.path()).unwrap();
    let before = snapshot(workspace.path());
    let compose = || {
        [
            render_mission_entity(&accepted).unwrap(),
            render_roadmap_entity(&accepted).unwrap(),
            render_ticket_entity(&ticket).unwrap(),
            render_specification_entity(&specification),
            render_criterion_entity(&reference, &resolved).unwrap(),
            render_requirement_entity(&accepted.bundle.manifest.requirements[0]),
        ]
        .concat()
    };
    let first = compose();
    assert_eq!(first, compose());
    assert!(first.contains("&lt;script&gt;"));
    assert!(!first.contains("<script>"));
    assert!(!first.contains("[link]("));
    assert!(first.contains(&reference.specification_uri()));
    assert!(
        render_roadmap_entity(&accepted)
            .unwrap()
            .contains(&render_requirement_row(
                &accepted.bundle.manifest.requirements[0]
            ))
    );
    assert_eq!(snapshot(workspace.path()), before);
    assert_eq!(accepted.bundle.manifest.title, payload);
}

#[test]
fn legacy_requirements_without_links_roundtrip_without_fabricated_authority() {
    let input = json!({"id": "mission-legacy", "text": "Legacy", "deferred": true});
    let value: Requirement = serde_json::from_value(input.clone()).unwrap();
    assert!(value.canonical_criteria.is_empty());
    assert_eq!(serde_json::to_value(value).unwrap(), input);
}

#[cfg(unix)]
#[test]
fn canonical_spec_symlinks_cannot_redirect_into_another_root() {
    use std::os::unix::fs::symlink;
    let workspace = fixture();
    let nested = workspace.path().join("nested-workspace");
    let (specification, reference) = spec(&nested, true);
    let destination = spec_path(workspace.path(), specification.id);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    symlink(spec_path(&nested, specification.id), &destination).unwrap();
    assert!(reference.resolve(workspace.path()).is_err());
}

#[test]
fn migration_rejects_unsupported_native_version_without_writing() {
    let workspace = fixture();
    let mut accepted = mission(workspace.path());
    accepted.bundle.manifest.schema_version = 99;
    let path = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(accepted.mission_id.to_string())
        .join("accepted.json");
    fs::write(path, serde_json::to_vec_pretty(&accepted).unwrap()).unwrap();
    let before = snapshot(workspace.path());
    assert!(MissionLifecycle::plan_migration(&[workspace.path().to_path_buf()]).is_err());
    assert_eq!(snapshot(workspace.path()), before);
}

#[test]
fn migration_does_not_replace_malformed_or_unsupported_existing_typed_source() {
    let workspace = fixture();
    let accepted = mission(workspace.path());
    let source = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(accepted.mission_id.to_string())
        .join("source.json");
    for bytes in [
        b"{malformed".as_slice(),
        br#"{"schema_version":99,"future_data":"retain"}"#.as_slice(),
    ] {
        fs::write(&source, bytes).unwrap();
        let before = snapshot(workspace.path());
        assert!(MissionLifecycle::plan_migration(&[workspace.path().to_path_buf()]).is_err());
        assert_eq!(snapshot(workspace.path()), before);
    }
}
#[test]
fn forward_migration_preserves_native_history_identity_and_canonical_references() {
    let workspace = fixture();
    let accepted = mission(workspace.path());
    let (_, reference) = spec(workspace.path(), true);
    let store = MissionStore::open(workspace.path());
    let mut bundle = accepted.bundle;
    bundle.manifest.revision += 1;
    bundle.manifest.requirements = vec![requirement(reference)];
    bundle.waypoints[0].requirement_ids = vec!["mission-contract".into()];
    let accepted = store.import(accepted.mission_id, bundle, 1).unwrap();
    let root = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(accepted.mission_id.to_string());
    let history = fs::read(root.join("history/revision-2.json")).unwrap();
    let mut journal = MissionLifecycle::plan_migration(&[workspace.path().to_path_buf()]).unwrap();
    assert_eq!(journal.records.len(), 1);
    MissionLifecycle::apply_migration(&mut journal).unwrap();
    assert_eq!(journal.records[0].state, MigrationState::Applied);
    let after = store.get(accepted.mission_id).unwrap();
    assert_eq!(after.mission_id, accepted.mission_id);
    assert_eq!(after.bundle.manifest.schema_version, 1);
    assert_eq!(
        after.bundle.manifest.requirements,
        accepted.bundle.manifest.requirements
    );
    assert_eq!(
        fs::read(root.join("history/revision-2.json")).unwrap(),
        history
    );
    let source = MissionLifecycle::open(workspace.path())
        .get_mission_source(after.mission_id)
        .unwrap();
    assert_eq!(source.schema_version, 2);
    assert_eq!(source.revision, after.bundle.manifest.revision);
}

#[test]
fn applying_migration_does_not_overwrite_source_that_appeared_after_planning() {
    let workspace = fixture();
    let accepted = mission(workspace.path());
    let mut journal = MissionLifecycle::plan_migration(&[workspace.path().to_path_buf()]).unwrap();
    let root = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(accepted.mission_id.to_string());
    let source = root.join("source.json");
    let bytes = br#"{"schema_version":99,"future":"preserve"}"#;
    fs::write(&source, bytes).unwrap();
    let accepted_before = fs::read(root.join("accepted.json")).unwrap();
    MissionLifecycle::apply_migration(&mut journal).unwrap();
    assert_eq!(journal.records[0].state, MigrationState::Failed);
    assert!(
        journal.records[0]
            .diagnostic
            .as_ref()
            .unwrap()
            .contains("appeared after planning")
    );
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert_eq!(
        fs::read(root.join("accepted.json")).unwrap(),
        accepted_before
    );
}

#[test]
fn rejected_typed_versions_and_attributes_preserve_source_and_accepted_history() {
    let workspace = fixture();
    let lifecycle = MissionLifecycle::open(workspace.path());
    let source = lifecycle
        .create_draft("Typed".into(), "Objective".into())
        .unwrap();
    let before = snapshot(workspace.path());
    for (field, value) in [
        ("schema_version", json!(99)),
        ("mission_id", json!(Uuid::nil())),
    ] {
        let mut candidate = serde_json::to_value(&source).unwrap();
        candidate[field] = value;
        let candidate = serde_json::from_value(candidate).unwrap();
        assert!(
            lifecycle
                .save_draft(source.mission_id, candidate, source.revision)
                .is_err()
        );
        assert_eq!(snapshot(workspace.path()), before);
    }
}

#[test]
fn criterion_identity_remains_spec_owned_and_is_not_restricted_to_mission_slugs() {
    let workspace = fixture();
    let (mut specification, mut reference) = spec(workspace.path(), true);
    let id = "API.R1_022";
    specification.extra.get_mut("criteria").unwrap()[0]["criterion_id"] = json!(id);
    reference.criterion_id = CriterionId::new(id).unwrap();
    write_spec(workspace.path(), &specification);
    assert!(reference.resolve(workspace.path()).is_ok());
}

#[test]
fn renderer_escaping_prevents_html_links_and_markdown_structure_in_plain_text() {
    use pulldown_cmark::{Event, Parser, Tag};
    let workspace = fixture();
    let mut accepted = mission(workspace.path());
    accepted.bundle.manifest.title = "<img src=x onerror=alert(1)>".into();
    accepted.bundle.manifest.objective =
        "- item\n# heading\n[jump](javascript:bad)\n1. numbered\n+ item\n& <script> `*text*`"
            .into();
    let rendered = render_mission_entity(&accepted).unwrap();
    for event in Parser::new(&rendered) {
        match event {
            Event::Html(value) | Event::InlineHtml(value) => assert_eq!(value.as_ref(), "<br>"),
            Event::Start(Tag::Link { .. } | Tag::Image { .. } | Tag::List(_)) => {
                panic!("plain-text values must not inject Markdown structure")
            }
            _ => {}
        }
    }
}
