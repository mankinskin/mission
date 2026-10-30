use super::*;

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
